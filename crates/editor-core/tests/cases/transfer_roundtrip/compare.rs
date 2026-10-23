use anyhow::{Result, anyhow};
use kasane_core::{Document, DrawableFrame, Vec2};
use kasane_project::{content_sha256, encode_project};
use kasane_sdk::AuthoringSession;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Serialize)]
pub(super) struct Difference {
    pub changed: usize,
    pub fields: BTreeMap<String, usize>,
    pub max_numeric_error: f64,
    pub examples: Vec<Value>,
}

impl Difference {
    fn record(&mut self, path: &str, before: &Value, after: &Value) {
        self.changed += 1;
        let field = path.split('/').nth(1).unwrap_or(path).to_owned();
        let count = self.fields.entry(field).or_default();
        *count += 1;
        if *count <= 3 && self.examples.len() < 64 {
            fn brief(value: &Value) -> Value {
                match value {
                    Value::Array(items) => json!({"array_length": items.len()}),
                    Value::Object(items) => json!({"keys": items.keys().collect::<Vec<_>>()}),
                    _ => value.clone(),
                }
            }
            self.examples
                .push(json!({"path": path, "before": brief(before), "after": brief(after)}));
        }
    }
}

pub(super) fn difference(before: &Value, after: &Value, tolerance: f64) -> Difference {
    fn visit(a: &Value, b: &Value, path: &str, epsilon: f64, result: &mut Difference) {
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
                for key in keys {
                    let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                    match (a.get(key), b.get(key)) {
                        (Some(a), Some(b)) => visit(a, b, &next, epsilon, result),
                        (a, b) => result.record(
                            &next,
                            a.unwrap_or(&Value::Null),
                            b.unwrap_or(&Value::Null),
                        ),
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    result.record(&format!("{path}/length"), &json!(a.len()), &json!(b.len()));
                }
                for (index, (a, b)) in a.iter().zip(b).enumerate() {
                    visit(a, b, &format!("{path}/{index}"), epsilon, result);
                }
            }
            (Value::Number(a), Value::Number(b)) => {
                let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
                let delta = (a - b).abs();
                result.max_numeric_error = result.max_numeric_error.max(delta);
                if delta > epsilon {
                    result.record(path, &json!(a), &json!(b));
                }
            }
            _ if a != b => result.record(path, a, b),
            _ => {}
        }
    }
    let mut result = Difference::default();
    visit(before, after, "", tolerance, &mut result);
    result
}

/// Compare all persisted fields. Only generated identities, asset locations,
/// PNG encoding and schema envelope are normalized, never authoring content.
pub(super) fn canonical_document(session: &AuthoringSession) -> Result<Value> {
    let document = session.document();
    let mut wire: Value = serde_json::from_str(
        &encode_project(document).map_err(|s| anyhow!("{}: {}", s.code, s.message))?,
    )?;
    let wire = wire.get_mut("document").unwrap();
    let mut ids = BTreeMap::from([(document.id().to_owned(), "@document".to_owned())]);
    for field in ["parameters", "parts", "transforms", "meshes", "offscreens"] {
        if let Some(items) = wire[field].as_array() {
            for item in items {
                ids.insert(
                    item["id"].as_str().unwrap().into(),
                    format!("@{field}/{}", item["runtime_id"].as_str().unwrap()),
                );
            }
        }
    }
    for field in ["motions", "expressions"] {
        if let Some(items) = wire["animation_assets"][field].as_array() {
            for item in items {
                ids.insert(
                    item["id"].as_str().unwrap().into(),
                    format!("@{field}/{}", item["name"].as_str().unwrap()),
                );
            }
        }
    }
    for asset in wire["assets"].as_array_mut().unwrap() {
        let id = asset["id"].as_str().unwrap();
        let pixels = session.read_texture(id)?;
        let hash = content_sha256(&pixels.rgba);
        ids.insert(id.into(), format!("@texture/{hash}"));
        asset["source"] = json!(format!("@pixels/{hash}"));
        asset["sha256"] = json!(hash);
    }
    fn register(value: &Value, path: &str, ids: &mut BTreeMap<String, String>) {
        match value {
            Value::Object(object) => {
                if let Some(id) = object.get("id").and_then(Value::as_str) {
                    if uuid::Uuid::parse_str(id).is_ok() {
                        ids.entry(id.into()).or_insert_with(|| format!("@{path}"));
                    }
                }
                for (key, value) in object {
                    register(value, &format!("{path}/{key}"), ids);
                }
            }
            Value::Array(items) => {
                for (i, value) in items.iter().enumerate() {
                    register(value, &format!("{path}/{i}"), ids);
                }
            }
            _ => {}
        }
    }
    fn replace(value: &mut Value, ids: &BTreeMap<String, String>) {
        match value {
            Value::String(value) => {
                if let Some(id) = ids.get(value) {
                    *value = id.clone();
                }
            }
            Value::Array(items) => {
                for value in items {
                    replace(value, ids);
                }
            }
            Value::Object(object) => {
                *object = std::mem::take(object)
                    .into_iter()
                    .map(|(key, mut value)| {
                        replace(&mut value, ids);
                        (ids.get(&key).cloned().unwrap_or(key), value)
                    })
                    .collect();
            }
            _ => {}
        }
    }
    // Morph constraints/tables are emitted from hash maps by native import.
    // Their order is not an authoring order; identify them by their dependency
    // and table content, then identify bindings by their target and driver.
    for field in [
        "blend_key_tables",
        "blend_constraints",
        "bindings",
        "scene_bindings",
        "blend_bindings",
    ] {
        if let Some(items) = wire[field].as_array() {
            for item in items {
                let mut identity = match field {
                    "bindings" => json!({"mesh":item["mesh_id"]}),
                    "scene_bindings" => json!({"target":item["target_id"]}),
                    "blend_bindings" => {
                        json!({"target":item["target_id"],"kind":item["target_kind"],"table":item["key_table_id"]})
                    }
                    _ => {
                        let mut value = item.clone();
                        value.as_object_mut().unwrap().remove("id");
                        value
                    }
                };
                replace(&mut identity, &ids);
                ids.insert(
                    item["id"].as_str().unwrap().into(),
                    format!(
                        "@{field}/{}",
                        content_sha256(identity.to_string().as_bytes())
                    ),
                );
            }
        }
    }
    register(wire, "document", &mut ids);
    replace(wire, &ids);
    for field in [
        "blend_key_tables",
        "blend_constraints",
        "bindings",
        "scene_bindings",
        "blend_bindings",
    ] {
        if let Some(items) = wire[field].as_array_mut() {
            for item in items.iter_mut() {
                if let Some(constraints) =
                    item.get_mut("constraint_ids").and_then(Value::as_array_mut)
                {
                    constraints.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                }
            }
            items.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        }
    }
    Ok(wire.clone())
}

pub(super) struct Pose {
    pub name: String,
    pub values: BTreeMap<String, f32>,
}

pub(super) fn poses(document: &Document) -> Vec<Pose> {
    let mut result = vec![Pose {
        name: "default".into(),
        values: BTreeMap::new(),
    }];
    for id in document.parameter_order() {
        let p = document.get_parameter(id).unwrap();
        for (label, value) in [("min", p.minimum), ("max", p.maximum)] {
            result.push(Pose {
                name: format!("{}/{label}", p.runtime_id),
                values: BTreeMap::from([(p.runtime_id.clone(), value)]),
            });
        }
    }
    result.push(Pose {
        name: "combined-head-mouth".into(),
        values: BTreeMap::from([
            ("ParamAngleX".into(), 30.),
            ("ParamAngleY".into(), -30.),
            ("ParamMouthA".into(), 1.),
            ("ParamEyeLOpen".into(), 0.),
            ("ParamEyeROpen".into(), 0.),
        ])
        .into_iter()
        .filter(|(name, _)| {
            document
                .parameter_order()
                .iter()
                .any(|id| document.get_parameter(id).unwrap().runtime_id == *name)
        })
        .collect(),
    });
    result
}

pub(super) struct Geometry {
    poses: BTreeMap<String, BTreeMap<String, Vec<Vec2>>>,
    pub metadata: Value,
}

pub(super) fn geometry(document: &Document, poses: &[Pose]) -> Result<Geometry> {
    let mut positions = BTreeMap::new();
    let mut metadata = serde_json::Map::new();
    let parameters: BTreeMap<_, _> = document
        .parameter_order()
        .iter()
        .map(|id| {
            (
                document.get_parameter(id).unwrap().runtime_id.as_str(),
                id.as_str(),
            )
        })
        .collect();
    for pose in poses {
        let values = pose
            .values
            .iter()
            .filter_map(|(name, value)| {
                parameters
                    .get(name.as_str())
                    .map(|id| ((*id).to_owned(), *value))
            })
            .collect();
        let mut frame = DrawableFrame::default();
        let status =
            kasane_core::evaluation::evaluate_frame_including_hidden(document, &values, &mut frame);
        anyhow::ensure!(status.is_ok(), "{}: {}", status.code, status.message);
        let mut meshes = BTreeMap::new();
        let mut mesh_metadata = serde_json::Map::new();
        for drawable in &frame.drawables {
            meshes.insert(drawable.runtime_id.clone(), drawable.positions.clone());
            let mut data = serde_json::to_value(drawable)?;
            for field in [
                "id",
                "part_id",
                "texture_asset_id",
                "texture_slot",
                "positions",
                "masks",
            ] {
                data.as_object_mut().unwrap().remove(field);
            }
            data["masks"] = json!(
                drawable
                    .masks
                    .iter()
                    .map(|id| document
                        .get_mesh(id)
                        .map(|m| m.runtime_id.clone())
                        .unwrap_or_else(|| format!("missing:{id}")))
                    .collect::<Vec<_>>()
            );
            // UVs/topology are static; retain them once, appearance/order for every pose.
            if pose.name != "default" {
                data.as_object_mut().unwrap().remove("uvs");
                data.as_object_mut().unwrap().remove("indices");
            }
            mesh_metadata.insert(drawable.runtime_id.clone(), data);
        }
        positions.insert(pose.name.clone(), meshes);
        metadata.insert(pose.name.clone(), Value::Object(mesh_metadata));
    }
    Ok(Geometry {
        poses: positions,
        metadata: Value::Object(metadata),
    })
}

pub(super) fn geometry_difference(before: &Geometry, after: &Geometry) -> Value {
    let mut max_error = 0_f64;
    let mut exceeding = 0usize;
    let mut missing = Vec::new();
    let mut worst = Value::Null;
    for (pose, meshes) in &before.poses {
        for (mesh, vertices) in meshes {
            let Some(other) = after.poses.get(pose).and_then(|m| m.get(mesh)) else {
                missing.push(format!("{pose}/{mesh}"));
                continue;
            };
            if vertices.len() != other.len() {
                missing.push(format!("{pose}/{mesh}/vertex_count"));
                continue;
            }
            for (index, (a, b)) in vertices.iter().zip(other).enumerate() {
                let error = f64::from(a.x - b.x).hypot(f64::from(a.y - b.y));
                if error > 0.1 {
                    exceeding += 1;
                }
                if error > max_error {
                    max_error = error;
                    worst = json!({"pose":pose,"mesh":mesh,"vertex":index,"before":a,"after":b});
                }
            }
        }
    }
    json!({"max_vertex_error_px":max_error,"vertices_over_0_1px":exceeding,"missing":missing,"worst":worst,"appearance_topology":difference(&before.metadata,&after.metadata,1e-6)})
}

#[test]
fn geometry_comparison_detects_displacement_and_appearance_loss() {
    let make = |x, opacity| Geometry {
        poses: BTreeMap::from([(
            "default".into(),
            BTreeMap::from([("mesh".into(), vec![Vec2::new(x, 0.)])]),
        )]),
        metadata: json!({"default":{"mesh":{"opacity":opacity}}}),
    };
    let baseline = make(0., 1.);
    let within = geometry_difference(&baseline, &make(0.05, 1.));
    assert_eq!(within["vertices_over_0_1px"], 0);
    assert!(within["max_vertex_error_px"].as_f64().unwrap() > 0.);
    let changed = geometry_difference(&baseline, &make(0.2, 0.5));
    assert_eq!(changed["vertices_over_0_1px"], 1);
    assert_eq!(changed["appearance_topology"]["changed"], 1);
}
