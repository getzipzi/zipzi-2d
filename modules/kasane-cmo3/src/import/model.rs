use super::{
    graph::{attr, Graph, Node},
    invalid, ImportedModel, Result,
};
use kasane_core::{types::*, Document};
use std::collections::HashMap;
pub(super) fn edit(result: EditResult) -> Result<()> {
    status(result.status)
}
fn status(s: Status) -> Result<()> {
    if s.is_ok() {
        Ok(())
    } else {
        Err(invalid(format!("{}: {}", s.code, s.message)))
    }
}
pub(super) fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(super) fn read(
    g: &Graph<'_>,
    root: Node<'_>,
    resources: &HashMap<String, Vec<u8>>,
) -> Result<ImportedModel> {
    if !root.has_tag_name("CModelSource") {
        return Err(invalid("Expected a CMO3 model"));
    }
    let canvas_node = g.req(root, "canvas")?;
    let info = g.req(root, "modelInfo")?;
    let origin = g.req(info, "originInPixels")?;
    let canvas = Canvas::new(
        g.number(canvas_node, "pixelWidth")?,
        g.number(canvas_node, "pixelHeight")?,
        Vec2::new(g.number(origin, "x")?, g.number(origin, "y")?),
        g.number(info, "pixelsPerUnit")?,
    );
    let mut document = Document::new();
    status(document.initialize(
        &g.guid(root, "guid")?,
        ModelSetup {
            source_canvas: Some(canvas),
            export: ExportSettings {
                pixels_per_unit: canvas.pixels_per_unit,
                bounds: Some([
                    -canvas.origin.x,
                    canvas.origin.y - canvas.height,
                    canvas.width - canvas.origin.x,
                    canvas.origin.y,
                ]),
                ..Default::default()
            },
        },
    ))?;
    if g.field(root, "name")?.is_some() {
        let mut settings = document.model3_settings().clone();
        settings
            .extensions
            .insert("Name".into(), g.text(root, "name")?.into());
        edit(document.set_model3_settings(settings))?;
    }
    let parameters = g.list(g.req(root, "parameterSourceSet")?, "_sources")?;
    for n in parameters {
        let kind = g
            .field(n, "paramType")?
            .and_then(|n| n.attribute("v"))
            .unwrap_or("NORMAL");
        if kind != "NORMAL" && kind != "MORPH_TARGET" {
            return Err(invalid(format!("Unsupported parameter type {kind}")));
        }
        edit(document.create_parameter(Parameter {
            id: g.guid(n, "guid")?,
            runtime_id: g.runtime(n)?,
            name: g.text(n, "name")?,
            minimum: g.number(n, "minValue")?,
            maximum: g.number(n, "maxValue")?,
            default_value: g.number(n, "defaultValue")?,
            decimal_places: g.optional(n, "decimalPlaces", 3)?,
            repeat: g.optional(n, "isRepeat", false)?,
            kind: if kind == "MORPH_TARGET" {
                ParameterKind::BlendShape
            } else {
                ParameterKind::Normal
            },
            ..Default::default()
        }))?;
    }
    let parts = g.list(g.req(root, "partSourceSet")?, "_sources")?;
    let root_part = g.guid(g.req(root, "rootPart")?, "guid")?;
    let part_parent = |n| -> Result<String> {
        let p = g.guid(n, "parentGuid")?;
        Ok(if p == root_part { String::new() } else { p })
    };
    let mut pending: Vec<_> = parts
        .iter()
        .copied()
        .filter(|n| g.guid(*n, "guid").ok().as_ref() != Some(&root_part))
        .collect();
    while !pending.is_empty() {
        let before = pending.len();
        let mut remaining = Vec::new();
        for n in pending {
            let parent = part_parent(n)?;
            if !parent.is_empty() && document.get_part(&parent).is_none() {
                remaining.push(n);
                continue;
            }
            if g.optional(n, "useOffscreen", false)? {
                return Err(invalid("CMO3 offscreen import is not yet representable"));
            }
            edit(document.create_part(Part {
                id: g.guid(n, "guid")?,
                runtime_id: g.runtime(n)?,
                name: g.text(n, "localName")?,
                parent_id: parent,
                enabled: g.optional(n, "isVisible", true)?,
                draw_order: g.optional(n, "defaultOrder_forEditor", 500.)?,
            }))?;
        }
        if remaining.len() == before {
            return Err(invalid("Missing or cyclic Part parent"));
        }
        pending = remaining;
    }
    let transforms = g.list(g.req(root, "deformerSourceSet")?, "_sources")?;
    let transform_ids: Vec<_> = transforms
        .iter()
        .map(|n| g.guid(*n, "guid"))
        .collect::<Result<_>>()?;
    let parent_transform = |n| -> Result<String> {
        let parent = g.guid(n, "targetDeformerGuid")?;
        Ok(if transform_ids.contains(&parent) {
            parent
        } else if parent == "71fae776-e218-4aee-873e-78e8ac0cb48a"
            || parent == "00000000-0000-0000-0000-000000000000"
        {
            String::new()
        } else {
            return Err(invalid(format!("Unknown parent deformer {parent}")));
        })
    };
    let mut scene_bindings = Vec::new();
    let mut pending = transforms;
    while !pending.is_empty() {
        let before = pending.len();
        let mut remaining = Vec::new();
        for n in pending {
            let parent = parent_transform(n)?;
            if !parent.is_empty() && document.get_transform(&parent).is_none() {
                remaining.push(n);
                continue;
            }
            let target = g.guid(n, "guid")?;
            let (axes, forms) = grid(g, n)?;
            let base = default_form(&document, &axes, &forms)?;
            let base_appearance = appearance(g, base.1)?;
            let track;
            let data = match n.tag_name().name() {
                "CWarpDeformerSource" => {
                    let keyforms: Vec<_> = forms
                        .iter()
                        .map(|(keys, form)| {
                            Ok(WarpKeyform {
                                keys: keys.clone(),
                                positions: positions(g, *form, &parent, canvas)?,
                                appearance: appearance(g, *form)?,
                                bezier: None,
                            })
                        })
                        .collect::<Result<_>>()?;
                    let points = positions(g, base.1, &parent, canvas)?;
                    track = SceneTrack::Warp {
                        target_id: target.clone().into(),
                        keyforms,
                    };
                    TransformData::Warp(WarpTransform {
                        rows: g.number(n, "row")?,
                        columns: g.number(n, "col")?,
                        quad: g.optional(n, "isQuadTransform", false)?,
                        points,
                        bezier: None,
                    })
                }
                "CRotationDeformerSource" => {
                    let keyforms: Vec<_> = forms
                        .iter()
                        .map(|(keys, form)| {
                            Ok(RotationKeyform {
                                keys: keys.clone(),
                                rotation: rotation(*form, &parent, canvas)?,
                                appearance: appearance(g, *form)?,
                            })
                        })
                        .collect::<Result<_>>()?;
                    track = SceneTrack::Rotation {
                        target_id: target.clone().into(),
                        keyforms,
                    };
                    TransformData::Rotation(RotationTransform {
                        base_angle: -g.optional::<f32>(n, "baseAngle", 0.)?
                            - if parent.is_empty() { 0. } else { 180. },
                        pose: rotation(base.1, &parent, canvas)?,
                    })
                }
                tag => return Err(invalid(format!("Unsupported deformer {tag}"))),
            };
            edit(document.create_transform(Transform {
                id: target,
                runtime_id: g.runtime(n)?,
                name: g.text(n, "localName")?,
                parent_id: TransformId::optional(parent),
                part_id: PartId::optional(part_parent(n)?),
                data,
                enabled: g.optional(n, "isVisible", true)?,
                appearance: base_appearance,
            }))?;
            if !axes.is_empty() {
                scene_bindings.push(SceneBinding {
                    id: id(),
                    axes,
                    track,
                });
            }
        }
        if remaining.len() == before {
            return Err(invalid("Missing or cyclic deformer parent"));
        }
        pending = remaining;
    }
    for n in &parts {
        let target = g.guid(*n, "guid")?;
        if target == root_part {
            continue;
        }
        let (axes, forms) = grid(g, *n)?;
        if !axes.is_empty() {
            scene_bindings.push(SceneBinding {
                id: id(),
                axes,
                track: SceneTrack::Part {
                    target_id: target.into(),
                    keyforms: forms
                        .iter()
                        .map(|(keys, n)| {
                            Ok(PartKeyform {
                                keys: keys.clone(),
                                draw_order: g.number(*n, "drawOrder")?,
                            })
                        })
                        .collect::<Result<_>>()?,
                },
            });
        }
    }
    for binding in scene_bindings {
        edit(document.create_scene_binding(binding))?;
    }
    let meshes = g.list(g.req(root, "drawableSourceSet")?, "_sources")?;
    let mut decoded_budget = 0u64;
    let mut images = HashMap::new();
    let mut texture_ids: HashMap<String, String> = HashMap::new();
    let mut deferred_masks = Vec::new();
    let mut vertex_uids = HashMap::new();
    for n in meshes {
        if !n.has_tag_name("CArtMeshSource") {
            return Err(invalid(format!(
                "Unsupported drawable {}",
                n.tag_name().name()
            )));
        }
        let target = g.guid(n, "guid")?;
        let parent = parent_transform(n)?;
        let texture = g.req(n, "texture")?;
        let resource = g.req(texture, "srcImageResource")?;
        let file = g.req(resource, "imageFileBuf")?;
        let path = file
            .attribute("path")
            .ok_or_else(|| invalid("Missing embedded texture path"))?;
        let texture_id = if let Some(id) = texture_ids.get(path) {
            id.clone()
        } else {
            let texture_id = id();
            let bytes = resources
                .get(path)
                .ok_or_else(|| invalid(format!("Missing embedded texture {path}")))?
                .clone();
            if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n".as_slice()) || bytes.len() < 24 {
                return Err(invalid("Invalid embedded PNG header"));
            }
            let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            if width == 0 || height == 0 || width > 16384 || height > 16384 {
                return Err(invalid(
                    "Cubism texture dimensions exceed the supported range",
                ));
            }
            decoded_budget = decoded_budget
                .checked_add(u64::from(width) * u64::from(height) * 4)
                .ok_or_else(|| invalid("Texture budget overflow"))?;
            if width > 16384 || height > 16384 || decoded_budget > 512 * 1024 * 1024 {
                return Err(invalid("Cubism textures exceed the decoded memory budget"));
            }
            let decoded = kasane_core::image::decode_png(&bytes).map_err(|s| invalid(s.message))?;
            let source = format!("assets/.memory-cubism/{texture_id}.png");
            edit(document.add_asset(ImageAsset {
                id: texture_id.clone(),
                name: path.into(),
                source: source.clone(),
                width: decoded.width,
                height: decoded.height,
                sha256: String::new(),
            }))?;
            images.insert(source, bytes);
            texture_ids.insert(path.to_string(), texture_id.clone());
            texture_id
        };
        let (axes, forms) = grid(g, n)?;
        let base = default_form(&document, &axes, &forms)?;
        let points = positions(g, base.1, &parent, canvas)?;
        let raw_uvs = g.floats(n, "uvs")?;
        if raw_uvs.len() != points.len() * 2 {
            return Err(invalid("UV/vertex length mismatch"));
        }
        let values = if let Some(t) = g.field(texture, "transformImageResource01toLogical01")? {
            [
                attr::<f32>(t, "m00")?,
                attr(t, "m01")?,
                attr(t, "m02")?,
                attr(t, "m10")?,
                attr(t, "m11")?,
                attr(t, "m12")?,
            ]
        } else {
            [1., 0., 0., 0., 1., 0.]
        };
        let [a, b, tx, c, d, ty] = values;
        let det = a * d - b * c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return Err(invalid("Singular texture transform"));
        }
        let uvs = raw_uvs
            .chunks_exact(2)
            .map(|v| {
                let x = v[0] - tx;
                let y = v[1] - ty;
                Vec2::new((d * x - b * y) / det, 1. - (-c * x + a * y) / det)
            })
            .collect();
        let indices: Vec<u32> = g.array(n, "indices")?;
        if !indices.len().is_multiple_of(3) || indices.iter().any(|i| *i as usize >= points.len()) {
            return Err(invalid("Invalid triangle indices"));
        }
        let vertices: Vec<u32> =
            (0..u32::try_from(points.len()).map_err(|_| invalid("Too many vertices"))?).collect();
        let mut uids = vertices.clone();
        for extension in g.list(n, "_extensions")? {
            if extension.has_tag_name("CEditableMeshExtension") {
                let editable = g.req(extension, "editableMesh")?;
                uids = g.array(editable, "pointUid")?;
            }
        }
        if uids.len() != vertices.len() {
            return Err(invalid("Editable vertex identity length mismatch"));
        }
        vertex_uids.insert(target.clone(), uids);
        let color = g
            .req(n, "colorComposition")?
            .attribute("v")
            .unwrap_or("NORMAL");
        let color_modes = [
            "NORMAL",
            "ADD",
            "MULTIPLY",
            "ADD_R2_TSL",
            "ADD_R2",
            "DARKEN",
            "MULTIPLY_R2",
            "COLORBURN_TSL",
            "LINEARBURN_TSL",
            "LIGHTEN",
            "SCREEN",
            "COLORDODGE_TSL",
            "OVERLAY",
            "SOFTLIGHT",
            "HARDLIGHT",
            "LINEARLIGHT_TSL",
            "HSL_HUE",
            "HSL_COLOR",
        ];
        let color_mode = color_modes
            .iter()
            .position(|v| *v == color)
            .ok_or_else(|| invalid(format!("Unsupported color composition {color}")))?
            as u32;
        let alpha = g
            .field(n, "alphaComposition")?
            .and_then(|n| n.attribute("v"))
            .unwrap_or("OVER");
        let alpha_mode = ["OVER", "ATOP", "OUT", "CONJOINT", "DISJOINT"]
            .iter()
            .position(|v| *v == alpha)
            .ok_or_else(|| invalid(format!("Unsupported alpha composition {alpha}")))?
            as u32;
        let packed = color_mode | (alpha_mode << 8);
        let blend = match color_mode {
            1 => BlendMode::Additive,
            2 => BlendMode::Multiplicative,
            _ => BlendMode::Normal,
        };
        let mut masks = g
            .list(n, "clipGuidList")?
            .iter()
            .map(|n| n.attribute("uuid").unwrap_or("").to_string())
            .collect::<Vec<_>>();
        let mut seen_masks = std::collections::HashSet::new();
        masks.retain(|id| seen_masks.insert(id.clone()));
        edit(
            document.create_mesh(Mesh {
                id: target.clone(),
                runtime_id: g.runtime(n)?,
                name: g.text(n, "localName")?,
                texture_asset_id: texture_id,
                vertex_ids: vertices,
                base_positions: points,
                uvs,
                triangles: indices
                    .chunks_exact(3)
                    .map(|v| [v[0], v[2], v[1]])
                    .collect(),
                part_id: part_parent(n)?,
                deformer_id: parent.clone(),
                appearance: appearance(g, base.1)?,
                draw_order: Some(g.number(base.1, "drawOrder")?),
                blend_mode: blend,
                raw_blend_mode: (alpha_mode != 0 || color_mode >= 3).then_some(packed),
                enabled: g.optional(n, "isVisible", true)?,
                double_sided: !g.optional(n, "culling", false)?,
                inverted_mask: g.optional(n, "invertClippingMask", false)?,
                ..Default::default()
            }),
        )?;
        if !masks.is_empty() {
            deferred_masks.push((target.clone(), masks));
        }
        if !axes.is_empty() {
            let keyforms = forms
                .iter()
                .map(|(keys, n)| {
                    Ok(MeshKeyform {
                        keys: keys.clone(),
                        positions: positions(g, *n, &parent, canvas)?,
                        appearance: appearance(g, *n)?,
                        draw_order: Some(g.number(*n, "drawOrder")?),
                    })
                })
                .collect::<Result<_>>()?;
            edit(document.create_binding(MeshBinding {
                id: id(),
                mesh_id: target,
                axes,
                keyforms,
            }))?;
        }
    }
    for (id, masks) in deferred_masks {
        let mut mesh = document.get_mesh(&id).unwrap().clone();
        mesh.masks = masks;
        edit(document.replace_mesh(mesh))?;
    }
    for n in g.list(g.req(root, "affecterSourceSet")?, "_sources")? {
        if !n.has_tag_name("CGlueSource") {
            return Err(invalid("Unsupported affecter"));
        }
        let a = g.guid(n, "targetArtMeshA_guid")?;
        let b = g.guid(n, "targetArtMeshB_guid")?;
        let uids: Vec<u32> = g.array(n, "bindVertexUids")?;
        let weights = g.floats(n, "weights")?;
        if uids.len() != weights.len() || !uids.len().is_multiple_of(2) {
            return Err(invalid("Invalid Glue vertex/weight pairs"));
        }
        let mut pairs = Vec::new();
        for (uv, w) in uids.chunks_exact(2).zip(weights.chunks_exact(2)) {
            let vertex = |mesh: &str, uid: u32| -> Result<u32> {
                vertex_uids
                    .get(mesh)
                    .and_then(|v| v.iter().position(|x| *x == uid))
                    .map(|v| v as u32)
                    .ok_or_else(|| invalid("Missing Glue vertex identity"))
            };
            pairs.push(GlueVertexPair {
                vertex_a: vertex(&a, uv[0])?,
                vertex_b: vertex(&b, uv[1])?,
                weight_a: w[0],
                weight_b: w[1],
            });
        }
        let (axes, forms) = grid(g, n)?;
        let default = default_form(&document, &axes, &forms)?;
        let binding = if axes.is_empty() {
            None
        } else {
            Some(GlueBinding {
                axes,
                keyforms: forms
                    .iter()
                    .map(|(_, n)| {
                        Ok(GlueKeyform {
                            intensity: g.number(*n, "intensity")?,
                        })
                    })
                    .collect::<Result<_>>()?,
            })
        };
        edit(document.create_glue(Glue {
            id: g.guid(n, "guid")?,
            runtime_id: g.runtime(n)?,
            name: g.text(n, "localName")?,
            mesh_a_id: a,
            mesh_b_id: b,
            pairs,
            intensity: g.number(default.1, "intensity")?,
            binding,
        }))?;
    }
    super::morph::read(g, root, &mut document, canvas)?;
    super::metadata::read(g, root, &mut document)?;
    if let Some(issue) = document.validate_structure().first() {
        return Err(invalid(format!(
            "{}: {}",
            issue.object_id, issue.status.message
        )));
    }
    Ok(ImportedModel { document, images })
}

pub(super) fn appearance(g: &Graph<'_>, n: Node<'_>) -> Result<Appearance> {
    let color = |name: &str, default: [f32; 3]| -> Result<[f32; 3]> {
        match g.field(n, name)? {
            Some(c) => Ok([attr(c, "red")?, attr(c, "green")?, attr(c, "blue")?]),
            None => Ok(default),
        }
    };
    Ok(Appearance {
        opacity: g.optional(n, "opacity", 1.)?,
        multiply: color("multiplyColor", [1.; 3])?,
        screen: color("screenColor", [0.; 3])?,
    })
}
pub(super) fn positions(
    g: &Graph<'_>,
    n: Node<'_>,
    parent: &str,
    canvas: Canvas,
) -> Result<Vec<Vec2>> {
    let raw = g.floats(n, "positions")?;
    if !raw.len().is_multiple_of(2) {
        return Err(invalid("Odd position array length"));
    }
    Ok(raw
        .chunks_exact(2)
        .map(|p| {
            if parent.is_empty() {
                Vec2::new(p[0] - canvas.origin.x, canvas.origin.y - p[1])
            } else {
                Vec2::new(p[0], p[1])
            }
        })
        .collect())
}
pub(super) fn rotation(n: Node<'_>, parent: &str, canvas: Canvas) -> Result<RotationPose> {
    let x: f64 = attr(n, "originX")?;
    let y: f64 = attr(n, "originY")?;
    Ok(RotationPose {
        origin: PreciseVec2::new(
            if parent.is_empty() {
                x - canvas.origin.x as f64
            } else {
                x
            },
            if parent.is_empty() {
                canvas.origin.y as f64 - y
            } else {
                y
            },
        ),
        angle: -attr::<f32>(n, "angle")?,
        scale: attr(n, "scale")?,
        reflect_x: attr(n, "isReflectX")?,
        reflect_y: !attr::<bool>(n, "isReflectY")?,
    })
}
type Forms<'a> = Vec<(Vec<f32>, Node<'a>)>;
pub(super) fn grid<'a>(g: &Graph<'a>, source: Node<'a>) -> Result<(Vec<BindingAxis>, Forms<'a>)> {
    let grid = g.req(source, "keyformGridSource")?;
    let bindings = g.list(grid, "keyformBindings")?;
    let mut axes = Vec::new();
    for b in &bindings {
        let interpolation = g
            .req(*b, "interpolationType")?
            .attribute("v")
            .unwrap_or("LINEAR");
        if interpolation != "LINEAR" {
            return Err(invalid(format!(
                "Unsupported binding interpolation {interpolation}"
            )));
        }
        let keys = g
            .list(*b, "keys")?
            .iter()
            .map(|n| super::graph::parse(n.text().unwrap_or("")))
            .collect::<Result<_>>()?;
        axes.push(BindingAxis {
            parameter_id: g.guid(*b, "parameterGuid")?,
            keys,
        });
    }
    let mut by_guid = HashMap::new();
    for form in g.list(source, "keyforms")? {
        by_guid.insert(g.guid(form, "guid")?, form);
    }
    let mut forms = Vec::new();
    for record in g.list(grid, "keyformsOnGrid")? {
        let mut keys = vec![f32::NAN; axes.len()];
        for on in g.list(g.req(record, "accessKey")?, "_keyOnParameterList")? {
            let binding = g.req(on, "binding")?;
            let parameter = g.guid(binding, "parameterGuid")?;
            let axis = axes
                .iter()
                .position(|a| a.parameter_id == parameter)
                .ok_or_else(|| invalid("Unknown keyform axis"))?;
            let index: usize = g.number(on, "keyIndex")?;
            keys[axis] = *axes[axis]
                .keys
                .get(index)
                .ok_or_else(|| invalid("Invalid keyform index"))?;
        }
        if keys.iter().any(|v| !v.is_finite()) {
            return Err(invalid("Incomplete keyform grid"));
        }
        let guid = g.guid(record, "keyformGuid")?;
        forms.push((
            keys,
            *by_guid
                .get(&guid)
                .ok_or_else(|| invalid("Missing keyform"))?,
        ));
    }
    if forms.is_empty() {
        return Err(invalid("Empty keyform grid"));
    }
    if axes.is_empty() && forms.len() != 1 {
        return Err(invalid("Ambiguous unbound keyform"));
    }
    Ok((axes, forms))
}
pub(super) fn default_form<'a>(
    document: &Document,
    axes: &[BindingAxis],
    forms: &Forms<'a>,
) -> Result<(Vec<f32>, Node<'a>)> {
    let defaults = axes
        .iter()
        .map(|a| {
            document
                .get_parameter(&a.parameter_id)
                .map(|p| p.default_value)
                .ok_or_else(|| invalid("Missing grid parameter"))
        })
        .collect::<Result<Vec<_>>>()?;
    let distance = |keys: &[f32]| {
        keys.iter()
            .zip(&defaults)
            .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
            .sum::<f64>()
    };
    Ok(forms
        .iter()
        .min_by(|a, b| distance(&a.0).total_cmp(&distance(&b.0)))
        .ok_or_else(|| invalid("Empty grid"))?
        .clone())
}
