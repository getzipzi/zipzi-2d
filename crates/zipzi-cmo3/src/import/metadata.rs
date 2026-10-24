use super::{
    graph::{Graph, Node},
    invalid,
    model::{edit, id},
    Result,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use zipzi_core::{
    document::{
        CdiParameterEntry, CdiParameterGroup, CdiPartEntry, DisplayInfo, DisplayInfoOrigin,
        PhysicsAsset,
    },
    draw_order::DrawOrderGroup,
    physics::PhysicsDefinition,
    Document,
};
pub fn read(g: &Graph<'_>, root: Node<'_>, document: &mut Document) -> Result<()> {
    if let Some(effect_set) = g.field(g.req(root, "modelInfo")?, "_effectParameterGroups")? {
        let mut groups = Vec::new();
        for entry in g.list(effect_set, "_parameterGroups")? {
            let key = g.req(entry, "key")?;
            let name = match key.attribute("v").unwrap_or("") {
                "EYE_BLINK" => "EyeBlink",
                "LIPSYNC" => "LipSync",
                name => {
                    return Err(invalid(format!(
                        "Unsupported effect parameter group {name}"
                    )))
                }
            };
            let parameters = g
                .list(entry, "value")?
                .iter()
                .map(|n| {
                    zipzi_core::document::ReferenceEntry::new(
                        zipzi_core::document::ModelTargetRef::Resolved {
                            object_id: n.attribute("uuid").unwrap_or("").to_string(),
                        },
                    )
                })
                .collect();
            groups.push(zipzi_core::document::ModelParameterGroup {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.into(),
                parameters,
                extensions: Default::default(),
            });
        }
        let mut settings = document.model3_settings().clone();
        settings.groups = Some(groups);
        edit(document.set_model3_settings(settings))?;
    }
    let root_group = g.guid(g.req(root, "rootParameterGroup")?, "guid")?;
    let mut groups = Vec::new();
    for n in g.list(g.req(root, "parameterGroupSet")?, "_groups")? {
        let guid = g.guid(n, "guid")?;
        if guid == root_group {
            continue;
        }
        let parent = g.guid(n, "parentGroupGuid")?;
        groups.push(CdiParameterGroup {
            id: guid,
            runtime_id: g.runtime(n)?,
            name: g.text(n, "name")?,
            parent_id: (parent != root_group && !parent.is_empty()).then_some(parent),
            extensions: Default::default(),
        });
    }
    let mut parameters = Vec::new();
    for n in g.list(g.req(root, "parameterSourceSet")?, "_sources")? {
        let parent = g.guid(n, "parentGroupGuid")?;
        parameters.push(CdiParameterEntry::Resolved {
            parameter_id: g.guid(n, "guid")?,
            group_id: (parent != root_group && !parent.is_empty()).then_some(parent),
            extensions: Default::default(),
        });
    }
    edit(
        document.replace_display_info(DisplayInfo {
            origin: DisplayInfoOrigin::Imported,
            parameters: Some(parameters),
            parameter_groups: Some(groups),
            parts: Some(
                document
                    .part_order()
                    .iter()
                    .map(|id| CdiPartEntry::Resolved {
                        part_id: id.clone(),
                        extensions: Default::default(),
                    })
                    .collect(),
            ),
            ..Default::default()
        }),
    )?;
    let parts = g.list(g.req(root, "partSourceSet")?, "_sources")?;
    let root_part = g.guid(g.req(root, "rootPart")?, "guid")?;
    let by_id: HashMap<_, _> = parts
        .iter()
        .map(|n| Ok((g.guid(*n, "guid")?, *n)))
        .collect::<Result<_>>()?;
    let mut owners = HashSet::from([root_part.clone()]);
    for n in &parts {
        if g.optional(*n, "enableDrawOrderGroup", false)? {
            owners.insert(g.guid(*n, "guid")?);
        }
    }
    fn children(
        g: &Graph<'_>,
        part: &str,
        parts: &HashMap<String, Node<'_>>,
        owners: &HashSet<String>,
        doc: &Document,
        visited: &mut HashSet<String>,
    ) -> Result<Vec<String>> {
        if !visited.insert(part.into()) {
            return Err(invalid("Cyclic Part child list"));
        }
        let mut items = Vec::new();
        for n in g
            .list(
                *parts
                    .get(part)
                    .ok_or_else(|| invalid("Missing drawing group Part"))?,
                "_childGuids",
            )?
            .iter()
            .rev()
        {
            let guid = n.attribute("uuid").unwrap_or("");
            if doc.get_mesh(guid).is_some() {
                items.push(guid.into());
            } else if doc.get_part(guid).is_some() {
                if owners.contains(guid) {
                    items.push(guid.into());
                } else {
                    items.extend(children(g, guid, parts, owners, doc, visited)?);
                }
            }
        }
        visited.remove(part);
        Ok(items)
    }
    let mut order = vec![root_part.clone()];
    order.extend(
        parts
            .iter()
            .filter_map(|n| g.guid(*n, "guid").ok())
            .filter(|id| id != &root_part && owners.contains(id)),
    );
    let mut draw_groups = Vec::new();
    for owner in order {
        let items = children(g, &owner, &by_id, &owners, document, &mut HashSet::new())?;
        draw_groups.push(DrawOrderGroup {
            owner: if owner == root_part {
                String::new()
            } else {
                owner
            },
            items,
            min_order: -32768,
            max_order: 32767,
        });
    }
    edit(document.replace_draw_order_groups(draw_groups))?;
    physics(g, root, document)
}
fn physics(g: &Graph<'_>, root: Node<'_>, document: &mut Document) -> Result<()> {
    let set = g.req(root, "physicsSettingsSourceSet")?;
    let settings = g.list(set, "_sourceCubismPhysics")?;
    if settings.is_empty() {
        return Ok(());
    }
    let mut rigs = Vec::new();
    let mut dictionary = Vec::new();
    let mut parameter_bindings = BTreeMap::new();
    let mut input_count = 0;
    let mut output_count = 0;
    let mut vertex_count = 0;
    for n in settings {
        let runtime = g.runtime(n)?;
        dictionary.push(json!({"Id":runtime,"Name":g.text(n,"name")?}));
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let kind = |n| -> Result<&str> {
            match g.req(n, "type")?.attribute("v").unwrap_or("") {
                "SRC_TO_X" => Ok("X"),
                "SRC_TO_Y" => Ok("Y"),
                "SRC_TO_G_ANGLE" => Ok("Angle"),
                name => Err(invalid(format!("Unknown physics axis {name}"))),
            }
        };
        for input in g.list(n, "inputs")? {
            let guid = g.guid(input, "source")?;
            let p = document
                .get_parameter(&guid)
                .ok_or_else(|| invalid("Physics source identity mismatch"))?;
            parameter_bindings.insert(p.runtime_id.clone(), guid);
            inputs.push(json!({"Source":{"Target":"Parameter","Id":p.runtime_id},"Weight":g.number::<f32>(input,"weight")?,"Type":kind(input)?,"Reflect":g.optional(input,"isReverse",false)?}));
        }
        for output in g.list(n, "outputs")? {
            let guid = g.guid(output, "destination")?;
            let p = document
                .get_parameter(&guid)
                .ok_or_else(|| invalid("Physics destination identity mismatch"))?;
            parameter_bindings.insert(p.runtime_id.clone(), guid);
            let axis = kind(output)?;
            let scale = if axis == "Angle" {
                g.number::<f32>(output, "angleScale")?
            } else {
                g.number::<f32>(
                    g.req(output, "translationScale")?,
                    if axis == "X" { "x" } else { "y" },
                )?
            };
            outputs.push(json!({"Destination":{"Target":"Parameter","Id":p.runtime_id},"VertexIndex":g.number::<usize>(output,"vertexIndex")?,"Scale":scale,"Weight":g.number::<f32>(output,"weight")?,"Type":axis,"Reflect":g.optional(output,"isReverse",false)?}));
        }
        let mut vertices = Vec::new();
        for v in g.list(n, "vertices")? {
            let p = g.req(v, "position")?;
            vertices.push(json!({"Position":{"X":g.number::<f32>(p,"x")?,"Y":g.number::<f32>(p,"y")?},"Mobility":g.number::<f32>(v,"mobility")?,"Delay":g.number::<f32>(v,"delay")?,"Acceleration":g.number::<f32>(v,"acceleration")?,"Radius":g.number::<f32>(v,"radius")?}));
        }
        let range = |name: &str| -> Result<Value> {
            Ok(
                json!({"Minimum":g.number::<f32>(n,&format!("normalized{name}ValueMin"))?,"Maximum":g.number::<f32>(n,&format!("normalized{name}ValueMax"))?,"Default":g.number::<f32>(n,&format!("normalized{name}DefaultValue"))?}),
            )
        };
        input_count += inputs.len();
        output_count += outputs.len();
        vertex_count += vertices.len();
        rigs.push(json!({"Id":runtime,"Input":inputs,"Output":outputs,"Vertices":vertices,"Normalization":{"Position":range("Position")?,"Angle":range("Angle")?}}));
    }
    let mut meta = json!({"PhysicsSettingCount":rigs.len(),"TotalInputCount":input_count,"TotalOutputCount":output_count,"VertexCount":vertex_count,"EffectiveForces":{"Gravity":{"X":0.,"Y":-1.},"Wind":{"X":0.,"Y":0.}},"PhysicsDictionary":dictionary});
    if let Some(fps) = g.field(set, "settingFPS")? {
        if fps.tag_name().name() != "null" {
            let value: f32 = super::graph::parse(fps.text().unwrap_or(""))?;
            meta["Fps"] = json!(value);
        }
    }
    let data: PhysicsDefinition =
        serde_json::from_value(json!({"Version":3,"Meta":meta,"PhysicsSettings":rigs}))
            .map_err(|e| invalid(e.to_string()))?;
    edit(
        document.set_physics(
            PhysicsAsset::from_wire(id(), data, |runtime| {
                parameter_bindings.get(runtime).cloned()
            })
            .map_err(|s| invalid(s.message))?,
        ),
    )
}
