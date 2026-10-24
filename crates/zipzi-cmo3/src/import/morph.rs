use super::{
    graph::{Graph, Node},
    invalid,
    model::{appearance, edit, grid, id, positions, rotation},
    Result,
};
use std::collections::HashMap;
use zipzi_core::{types::*, Document};
pub fn read(g: &Graph<'_>, root: Node<'_>, document: &mut Document, canvas: Canvas) -> Result<()> {
    for set in [
        "drawableSourceSet",
        "deformerSourceSet",
        "partSourceSet",
        "affecterSourceSet",
    ] {
        for source in g.list(g.req(root, set)?, "_sources")? {
            let Some(morphs) = g.field(source, "keyformMorphTargetSet")? else {
                continue;
            };
            let records = g.list(morphs, "_morphTargets")?;
            if records.is_empty() {
                continue;
            }
            let target = g.guid(source, "guid")?;
            let mut pool = HashMap::new();
            for form in g.list(source, "keyforms")? {
                pool.insert(g.guid(form, "guid")?, form);
            }
            let mut parameters: HashMap<String, Vec<(f32, Node<'_>)>> = HashMap::new();
            for record in records {
                if let Some(map) = g.field(record, "editBaseParameterMap")? {
                    if let Some(map) = g.field(map, "baseParameterMap")? {
                        if map.tag_name().name() != "null" && map.children().any(|n| n.is_element())
                        {
                            return Err(invalid("Morph edit base differs from default parameters"));
                        }
                    }
                }
                let parameter = g.guid(record, "parameterGuid")?;
                let key: f32 = g.number(record, "keyValue")?;
                if !key.is_finite() || key == 0. {
                    return Err(invalid("Invalid morph key"));
                }
                let form = *pool
                    .get(&g.guid(record, "keyformGuid")?)
                    .ok_or_else(|| invalid("Missing morph form"))?;
                parameters.entry(parameter).or_default().push((key, form));
            }
            let (axes, forms) = grid(g, source)?;
            let base = DefaultForm::new(document, &axes, &forms)?;
            let base_app = base.appearance(g)?;
            for (parameter, mut entries) in parameters {
                let mut keys: Vec<_> = entries.iter().map(|v| v.0).chain([0.]).collect();
                keys.sort_by(f32::total_cmp);
                if keys.windows(2).any(|w| w[0] == w[1]) {
                    return Err(invalid("Duplicate morph keys"));
                }
                let base_key_idx = keys.iter().position(|k| *k == 0.).unwrap();
                let table = id();
                edit(document.create_blend_key_table(BlendShapeKeyTable {
                    samples: Default::default(),
                    base_sample_id: Default::default(),
                    id: table.clone(),
                    parameter_id: parameter.clone(),
                    keys: keys.clone(),
                    base_key_idx,
                }))?;
                let mut constraint_ids = Vec::new();
                if let Some(constraints) = g.field(morphs, "blendWeightConstraintSet")? {
                    let mut groups: HashMap<String, Vec<(f32, f32)>> = HashMap::new();
                    for c in g.list(constraints, "_constraints")? {
                        if g.guid(c, "morphTargetParameterGuid")? == parameter {
                            groups
                                .entry(g.guid(c, "constraintParameterGuid")?)
                                .or_default()
                                .push((
                                    g.number(c, "constraintParameterValue")?,
                                    g.number(c, "blendWeight")?,
                                ));
                        }
                    }
                    for (parameter, mut values) in groups {
                        values.sort_by(|a, b| a.0.total_cmp(&b.0));
                        values.dedup();
                        let constraint = id();
                        edit(document.create_blend_constraint(BlendShapeConstraint {
                            samples: Default::default(),
                            id: constraint.clone(),
                            parameter_id: parameter,
                            keys: values.iter().map(|v| v.0).collect(),
                            weights: values.iter().map(|v| v.1).collect(),
                        }))?;
                        constraint_ids.push(constraint);
                    }
                }
                // The zero slot is constructed as a neutral delta, not as a source keyform.
                entries.push((0., forms[0].1));
                entries.sort_by(|a, b| a.0.total_cmp(&b.0));
                let color_delta = |a: [f32; 3], b: [f32; 3]| -> Option<[f32; 3]> {
                    Some(std::array::from_fn(|i| a[i] - b[i]))
                };
                let (kind, keyforms) = if let Some(mesh) = document.get_mesh(&target) {
                    let parent = &mesh.deformer_id;
                    let base_points = base.positions(g, parent, canvas)?;
                    let order = base.number(g, "drawOrder")?.round();
                    let deltas = entries
                        .iter()
                        .map(|(key, n)| {
                            if *key == 0. {
                                return Ok(DeltaMeshKeyform {
                                    positions: vec![Vec2::default(); base_points.len()],
                                    ..Default::default()
                                });
                            }
                            let app = appearance(g, *n)?;
                            let pts = positions(g, *n, parent, canvas)?;
                            if pts.len() != base_points.len() {
                                return Err(invalid("Morph vertex count differs"));
                            }
                            Ok(DeltaMeshKeyform {
                                sample_id: Default::default(),
                                positions: pts
                                    .iter()
                                    .zip(&base_points)
                                    .map(|(a, b)| Vec2::new(a.x - b.x, a.y - b.y))
                                    .collect(),
                                opacity: Some(app.opacity - base_app.opacity),
                                draw_order: Some(g.number::<f32>(*n, "drawOrder")? - order),
                                multiply: color_delta(app.multiply, base_app.multiply),
                                screen: color_delta(app.screen, base_app.screen),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    (BlendShapeTargetKind::Mesh, DeltaKeyforms::Mesh(deltas))
                } else if let Some(transform) = document.get_transform(&target) {
                    let parent = transform.parent();
                    match transform.kind() {
                        TransformKind::Warp => {
                            let base_points = base.positions(g, parent, canvas)?;
                            let deltas = entries
                                .iter()
                                .map(|(key, n)| {
                                    if *key == 0. {
                                        return Ok(DeltaWarpKeyform {
                                            points: vec![Vec2::default(); base_points.len()],
                                            ..Default::default()
                                        });
                                    }
                                    let app = appearance(g, *n)?;
                                    let pts = positions(g, *n, parent, canvas)?;
                                    if pts.len() != base_points.len() {
                                        return Err(invalid("Morph control point count differs"));
                                    }
                                    Ok(DeltaWarpKeyform {
                                        sample_id: Default::default(),
                                        points: pts
                                            .iter()
                                            .zip(&base_points)
                                            .map(|(a, b)| Vec2::new(a.x - b.x, a.y - b.y))
                                            .collect(),
                                        opacity: Some(app.opacity - base_app.opacity),
                                        multiply: color_delta(app.multiply, base_app.multiply),
                                        screen: color_delta(app.screen, base_app.screen),
                                    })
                                })
                                .collect::<Result<Vec<_>>>()?;
                            (BlendShapeTargetKind::Warp, DeltaKeyforms::Warp(deltas))
                        }
                        TransformKind::Rotation => {
                            let rest = base.rotation(parent, canvas)?;
                            let deltas = entries
                                .iter()
                                .map(|(key, n)| {
                                    if *key == 0. {
                                        return Ok(DeltaRotationKeyform::default());
                                    }
                                    let app = appearance(g, *n)?;
                                    let r = rotation(*n, parent, canvas)?;
                                    if r.reflect_x != rest.reflect_x
                                        || r.reflect_y != rest.reflect_y
                                    {
                                        return Err(invalid("Morph changes reflection flags"));
                                    }
                                    Ok(DeltaRotationKeyform {
                                        sample_id: Default::default(),
                                        origin: Some(Vec2::new(
                                            (r.origin.x - rest.origin.x) as f32,
                                            (r.origin.y - rest.origin.y) as f32,
                                        )),
                                        angle: Some(r.angle - rest.angle),
                                        scale: Some(r.scale - rest.scale),
                                        opacity: Some(app.opacity - base_app.opacity),
                                        multiply: color_delta(app.multiply, base_app.multiply),
                                        screen: color_delta(app.screen, base_app.screen),
                                    })
                                })
                                .collect::<Result<Vec<_>>>()?;
                            (
                                BlendShapeTargetKind::Rotation,
                                DeltaKeyforms::Rotation(deltas),
                            )
                        }
                    }
                } else if document.get_part(&target).is_some() {
                    let rest = base.number(g, "drawOrder")?.round();
                    (
                        BlendShapeTargetKind::Part,
                        DeltaKeyforms::Part(
                            entries
                                .iter()
                                .map(|(key, n)| {
                                    if *key == 0. {
                                        return Ok(DeltaPartKeyform::default());
                                    }
                                    Ok(DeltaPartKeyform {
                                        sample_id: Default::default(),
                                        draw_order: g.number::<f32>(*n, "drawOrder")? - rest,
                                    })
                                })
                                .collect::<Result<_>>()?,
                        ),
                    )
                } else if document.get_glue(&target).is_some() {
                    let rest = base.number(g, "intensity")?;
                    (
                        BlendShapeTargetKind::Glue,
                        DeltaKeyforms::Glue(
                            entries
                                .iter()
                                .map(|(key, n)| {
                                    if *key == 0. {
                                        return Ok(DeltaGlueKeyform::default());
                                    }
                                    Ok(DeltaGlueKeyform {
                                        sample_id: Default::default(),
                                        intensity: g.number::<f32>(*n, "intensity")? - rest,
                                    })
                                })
                                .collect::<Result<_>>()?,
                        ),
                    )
                } else {
                    return Err(invalid("Unknown morph target"));
                };
                edit(document.create_blend_binding(BlendShapeBinding {
                    id: id(),
                    target_id: target.clone(),
                    target_kind: kind,
                    key_table_id: table,
                    constraint_ids,
                    keyforms,
                }))?;
            }
        }
    }
    Ok(())
}

/// A morph is an absolute form at the ordinary grid's default parameter pose.
/// The default need not coincide with any authored keyform. Match grid sampling
/// (including the parameter's key snapping tolerance), using explicit keys so
/// the XML record order does not affect the result.
struct DefaultForm<'a>(Vec<(Node<'a>, f32)>);
impl<'a> DefaultForm<'a> {
    fn new(
        document: &Document,
        axes: &[BindingAxis],
        forms: &[(Vec<f32>, Node<'a>)],
    ) -> Result<Self> {
        let segments = axes
            .iter()
            .map(|axis| {
                let p = document
                    .get_parameter(&axis.parameter_id)
                    .ok_or_else(|| invalid("Missing grid parameter"))?;
                let epsilon = 0.1f32.powi(p.decimal_places);
                Ok(zipzi_core::keyforms::find_key_segment(
                    p.default_value,
                    &axis.keys,
                    epsilon,
                    epsilon * 1.5,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut weighted = Vec::new();
        // Match the grid's axis-zero-fastest order, also for discrete reflection flags.
        let mut forms: Vec<_> = forms.iter().collect();
        forms.sort_by(|(a, _), (b, _)| {
            a.iter()
                .rev()
                .zip(b.iter().rev())
                .map(|(a, b)| a.total_cmp(b))
                .find(|order| !order.is_eq())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (keys, form) in forms {
            let mut weight = 1.;
            for ((key, axis), segment) in keys.iter().zip(axes).zip(&segments) {
                let index = segment.index as usize;
                weight *= if *key == axis.keys[index] {
                    1. - segment.weight
                } else if segment.weight > 0. && axis.keys.get(index + 1) == Some(key) {
                    segment.weight
                } else {
                    0.
                };
            }
            if weight > 0. {
                weighted.push((*form, weight));
            }
        }
        if weighted.is_empty() {
            return Err(invalid("Missing default grid pose"));
        }
        Ok(Self(weighted))
    }
    fn number(&self, g: &Graph<'_>, name: &str) -> Result<f32> {
        self.0.iter().try_fold(0., |value, (n, w)| {
            Ok(value + g.number::<f32>(*n, name)? * w)
        })
    }
    fn appearance(&self, g: &Graph<'_>) -> Result<Appearance> {
        let mut value = Appearance {
            opacity: 0.,
            multiply: [0.; 3],
            screen: [0.; 3],
        };
        for (n, w) in &self.0 {
            let a = appearance(g, *n)?;
            value.opacity += a.opacity * w;
            for i in 0..3 {
                value.multiply[i] += a.multiply[i] * w;
                value.screen[i] += a.screen[i] * w;
            }
        }
        Ok(value)
    }
    fn positions(&self, g: &Graph<'_>, parent: &str, canvas: Canvas) -> Result<Vec<Vec2>> {
        let mut value: Vec<Vec2> = Vec::new();
        for (n, w) in &self.0 {
            let points = positions(g, *n, parent, canvas)?;
            if value.is_empty() {
                value.resize(points.len(), Vec2::default());
            }
            if value.len() != points.len() {
                return Err(invalid("Grid position count differs"));
            }
            for (p, q) in value.iter_mut().zip(points) {
                p.x += q.x * w;
                p.y += q.y * w;
            }
        }
        Ok(value)
    }
    fn rotation(&self, parent: &str, canvas: Canvas) -> Result<RotationPose> {
        let first = rotation(self.0[0].0, parent, canvas)?;
        let mut value = RotationPose {
            origin: PreciseVec2::default(),
            angle: 0.,
            scale: 0.,
            ..first
        };
        for (n, w) in &self.0 {
            let r = rotation(*n, parent, canvas)?;
            value.origin.x += r.origin.x * f64::from(*w);
            value.origin.y += r.origin.y * f64::from(*w);
            value.angle += r.angle * w;
            value.scale += r.scale * w;
        }
        Ok(value)
    }
}
