//! Fixed authoring target for one pointer gesture. Selection and evaluated pixels
//! never become implicit write destinations during a drag.
use crate::{
    CanvasScene, ControlGeometry, ControlKind, Document, ModelPoint, ObjectKind, ObjectRef,
};
use anyhow::{Result, bail, ensure};
use zipzi_core::{Mesh, MeshBinding, SceneBinding, SceneTrack, Transform, Vec2};

fn keyform_edit_hint(
    doc: &zipzi_core::Document,
    parameters: &[zipzi_core::EvaluatedParameter],
    axes: &[zipzi_core::BindingAxis],
) -> String {
    let active: Vec<_> = axes.iter().filter(|a| !a.parameter_id.is_empty()).collect();
    let value = |id: &str| parameters.iter().find(|p| p.id == id).map(|p| p.value);
    let off_keys: Vec<_> = active
        .iter()
        .copied()
        .filter(|axis| {
            !value(&axis.parameter_id)
                .is_some_and(|v| axis.keys.iter().any(|k| (v - k).abs() < 1e-6))
        })
        .collect();
    // If each axis is at a key but the combination is missing, identify all
    // binding axes rather than suggesting that no parameter needs adjusting.
    let targets = if off_keys.is_empty() {
        &active
    } else {
        &off_keys
    };
    let names = targets
        .iter()
        .map(|axis| {
            let id = axis.parameter_id.as_str();
            let name = doc.get_parameter(id).map_or(id, |p| {
                if !p.name.trim().is_empty() {
                    p.name.as_str()
                } else if !p.runtime_id.is_empty() {
                    p.runtime_id.as_str()
                } else {
                    id
                }
            });
            match value(id) {
                Some(v) => format!("\"{name}\" ({v})"),
                None => format!("\"{name}\""),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("Set {names} to stored keyform values before editing this object")
}

#[cfg(test)]
mod hint_tests {
    use super::keyform_edit_hint;
    use zipzi_core::{BindingAxis, Canvas, EvaluatedParameter, Parameter};

    #[test]
    fn keyform_hint_names_only_parameters_away_from_keys() -> anyhow::Result<()> {
        let ids = [
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ];
        let mut session = zipzi_sdk::AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(256., 256., Default::default(), 1.),
        )?;
        session.edit("Hint parameters", None, |edit| {
            for (id, name, runtime) in [
                (&ids[0], "Angle X", "ParamAngleX"),
                (&ids[1], "", "ParamAngleY"),
            ] {
                edit.create_parameter(Parameter {
                    id: id.clone(),
                    name: name.into(),
                    runtime_id: runtime.into(),
                    minimum: 0.,
                    maximum: 1.,
                    default_value: 0.,
                    ..Default::default()
                })?;
            }
            Ok(())
        })?;
        let axes: Vec<_> = ids
            .iter()
            .map(|id| BindingAxis {
                parameter_id: id.clone(),
                keys: vec![0., 1.],
                ..Default::default()
            })
            .collect();
        let mut parameters: Vec<_> = ids
            .iter()
            .map(|id| EvaluatedParameter {
                id: id.clone(),
                value: 0.,
                ..Default::default()
            })
            .collect();
        parameters[0].value = 0.5;
        let hint = keyform_edit_hint(session.document(), &parameters, &axes);
        assert!(hint.contains("\"Angle X\" (0.5)"));
        assert!(!hint.contains("ParamAngleY"));
        parameters[1].value = 0.25;
        let hint = keyform_edit_hint(session.document(), &parameters, &axes);
        assert!(hint.contains("\"Angle X\" (0.5)"));
        assert!(hint.contains("\"ParamAngleY\" (0.25)"));
        parameters[0].value = 0.;
        parameters[1].value = 0.;
        let hint = keyform_edit_hint(session.document(), &parameters, &axes);
        assert!(hint.contains("Angle X") && hint.contains("ParamAngleY"));
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ControlChange {
    Translate(crate::ModelVector),
    Transform {
        pivot: ModelPoint,
        scale: [f32; 2],
        radians: f32,
        translation: crate::ModelVector,
    },
    Rotation {
        angle: f32,
        scale: f32,
        translation: crate::ModelVector,
    },
    MoveAll(crate::ModelVector),
}

impl ControlChange {
    pub(crate) fn apply_to(self, p: ModelPoint) -> ModelPoint {
        match self {
            Self::Translate(d) | Self::MoveAll(d) | Self::Rotation { translation: d, .. } => {
                ModelPoint {
                    x: p.x + d.x,
                    y: p.y + d.y,
                }
            }
            Self::Transform {
                pivot,
                scale,
                radians,
                translation,
            } => {
                let (s, c) = radians.sin_cos();
                let x = (p.x - pivot.x) * scale[0];
                let y = (p.y - pivot.y) * scale[1];
                ModelPoint {
                    x: pivot.x + x * c - y * s + translation.x,
                    y: pivot.y + x * s + y * c + translation.y,
                }
            }
        }
    }
    pub(crate) fn moves_all(self) -> bool {
        !matches!(self, Self::Translate(_))
    }
}

#[derive(Clone)]
pub(crate) enum EditOriginal {
    Mesh(Mesh),
    Transform(Transform),
    MeshBinding(MeshBinding),
    BlendBinding(zipzi_core::BlendShapeBinding),
    SceneBinding(SceneBinding),
    Batch(Vec<EditOriginal>),
}
impl EditOriginal {
    pub fn id(&self) -> &str {
        match self {
            Self::Mesh(m) => &m.id,
            Self::Transform(t) => &t.id,
            Self::MeshBinding(b) => &b.id,
            Self::BlendBinding(b) => &b.id,
            Self::SceneBinding(b) => &b.id,
            Self::Batch(v) => v[0].id(),
        }
    }
    pub fn unchanged(&self, doc: &zipzi_core::Document) -> bool {
        match self {
            Self::Mesh(v) => doc.get_mesh(&v.id) == Some(v),
            Self::Transform(v) => doc.get_transform(&v.id) == Some(v),
            Self::MeshBinding(v) => doc.get_binding(&v.id) == Some(v),
            Self::BlendBinding(v) => doc.get_blend_binding(&v.id) == Some(v),
            Self::SceneBinding(v) => doc.get_scene_binding(&v.id) == Some(v),
            Self::Batch(v) => v.iter().all(|b| b.unchanged(doc)),
        }
    }
}
#[derive(Clone)]
pub(crate) struct ControlEdit {
    pub object: ObjectRef,
    pub original: EditOriginal,
    pub ids: Vec<u32>,
    pub positions: Vec<Vec2>,
    pub slot: Option<usize>,
    pub rotation: bool,
    pub space: ControlSpace,
    pub warp: Option<WarpEdit>,
    pub mesh: Option<crate::mesh_edit::MeshEdit>,
}
#[derive(Clone)]
pub(crate) struct WarpEdit {
    transform: Transform,
    binding: Option<SceneBinding>,
    controller: zipzi_core::WarpBezier,
}
#[derive(Clone, Debug)]
pub(crate) struct ControlSpace {
    pub parent: Option<ControlGeometry>,
    pub parent_shape: Option<(usize, usize, bool)>,
    parent_points: Vec<f32>,
}

/// Prefer the focused parameter or opened binding. Without either, one
/// contributing BlendShape on an exact stored key is unambiguous. Mixed or
/// multiple contributions still require an explicit authoring destination.
fn mesh_blend_target<'a>(
    document: &Document,
    doc: &'a zipzi_core::Document,
    scene: &CanvasScene,
    object: &ObjectRef,
) -> Result<Option<(&'a zipzi_core::BlendShapeBinding, usize, f32)>> {
    #[cfg(feature = "bench-instrumentation")]
    let _span = zipzi_core::benchmark::span("overlay.mesh_blend_target");
    let active = document.context().active_binding.as_ref();
    let focus = document.context().parameter_focus.as_ref();
    let bindings: Vec<_> = doc
        .iter_blend_bindings_for_target(&object.id)
        .filter(|b| doc.blend_binding_active(b))
        .collect();
    if bindings.is_empty() {
        return Ok(None);
    }
    let values = scene.parameter_values();
    for &b in &bindings {
        let table = doc.get_blend_key_table(&b.key_table_id).unwrap();
        if active.is_some_and(|r| r.kind == ObjectKind::BlendBinding && r.id == b.id)
            && focus.is_none_or(|r| r.id == table.parameter_id)
            || focus.is_some_and(|r| r.id == table.parameter_id)
        {
            let p = doc.get_parameter(&table.parameter_id).unwrap();
            let value = values[&p.id];
            let hint = || {
                format!(
                    "Set \"{}\" ({value}) to a stored BlendShape keyform before editing this object",
                    p.name
                )
            };
            let slot = table
                .keys
                .iter()
                .position(|k| (value - k).abs() < 1e-6)
                .ok_or_else(|| anyhow::anyhow!("{}", hint()))?;
            // Selecting the neutral sample explicitly chooses the underlying
            // base/ordinary form, even while other BlendShapes contribute.
            // Its stored zero delta is never a writable pose destination.
            if slot == table.base_key_idx {
                return Ok(None);
            }
            let weight = zipzi_core::evaluation::evaluate_blend_binding(doc, values, b)
                .into_iter()
                .find(|(i, _)| *i == slot)
                .map_or(0., |(_, w)| w);
            ensure!(
                weight >= 0.05,
                "BlendShape \"{}\" has insufficient effective influence ({weight}); adjust its constraints before editing",
                p.name
            );
            return Ok(Some((b, slot, weight)));
        }
    }
    let contributing: Vec<_> = bindings
        .iter()
        .copied()
        .filter(|b| !zipzi_core::evaluation::evaluate_blend_binding(doc, values, b).is_empty())
        .collect();
    // A newly opened model has no parameter focus. One contributing delta at
    // an exact stored key is already an unambiguous authoring destination.
    // Keep explicit binding choices above and ambiguous/mixed poses below.
    if active.is_none() && contributing.len() == 1 {
        let b = contributing[0];
        let table = doc.get_blend_key_table(&b.key_table_id).unwrap();
        if let Some(slot) = table
            .keys
            .iter()
            .position(|key| (values[&table.parameter_id] - key).abs() < 1e-6)
        {
            let weight = zipzi_core::evaluation::evaluate_blend_binding(doc, values, b)
                .into_iter()
                .find(|(i, _)| *i == slot)
                .map_or(0., |(_, w)| w);
            let p = doc.get_parameter(&table.parameter_id).unwrap();
            ensure!(
                weight >= 0.05,
                "BlendShape \"{}\" has insufficient effective influence ({weight}); adjust its constraints before editing",
                p.name
            );
            return Ok(Some((b, slot, weight)));
        }
    }
    let contributing: Vec<_> = contributing
        .iter()
        .map(|b| {
            let t = doc.get_blend_key_table(&b.key_table_id).unwrap();
            let p = doc.get_parameter(&t.parameter_id).unwrap();
            format!("\"{}\" ({})", p.name, values[&p.id])
        })
        .collect();
    ensure!(
        contributing.is_empty(),
        "Select a BlendShape keyform for {} before editing this object",
        contributing.join(", ")
    );
    Ok(None)
}

/// Base deformer poses remain editable when their BlendShapes are neutral.
/// Nonzero deltas need their own authoring path rather than an implicit base edit.
fn deformer_blend_edit_hint(
    doc: &zipzi_core::Document,
    scene: &CanvasScene,
    object: &ObjectRef,
) -> Option<String> {
    let values = scene.parameter_values();
    let parameters: Vec<_> = doc
        .iter_blend_bindings_for_target(&object.id)
        .filter(|b| !zipzi_core::evaluation::evaluate_blend_binding(doc, values, b).is_empty())
        .map(|b| {
            let table = doc.get_blend_key_table(&b.key_table_id).unwrap();
            let p = doc.get_parameter(&table.parameter_id).unwrap();
            let name = if p.name.trim().is_empty() {
                &p.runtime_id
            } else {
                &p.name
            };
            format!("\"{name}\" ({})", values[&p.id])
        })
        .collect();
    (!parameters.is_empty()).then(|| {
        format!(
            "Set {} to their BlendShape base keyforms before editing this deformer",
            parameters.join(", ")
        )
    })
}

pub(crate) enum ControlDescription {
    Writable(WritableControl),
    ReadOnly(String),
}

pub(crate) struct WritableControl {
    pub label: String,
    pub mesh_weight: Option<f32>,
}

impl ControlEdit {
    /// Read-only capability query for the overlay. Never clones authoring
    /// meshes or entire keyform tables on pointer/hover redraws.
    pub fn describe(
        document: &Document,
        scene: &CanvasScene,
        object: &ObjectRef,
    ) -> Result<ControlDescription> {
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("overlay.describe");
        document.ensure_editable(std::slice::from_ref(object))?;
        let doc = document.model();
        let writable = |label, weight| -> Result<ControlDescription> {
            let mesh_weight = if object.kind == ObjectKind::Mesh {
                let mesh = doc
                    .get_mesh(&object.id)
                    .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
                if !mesh.deformer_id.is_empty()
                    && !scene.control(&mesh.deformer_id).is_some_and(|c| c.enabled)
                {
                    return Ok(ControlDescription::ReadOnly(
                        "Parent deformer is unavailable".into(),
                    ));
                }
                Some(weight)
            } else {
                None
            };
            Ok(ControlDescription::Writable(WritableControl {
                label,
                mesh_weight,
            }))
        };
        if object.kind == ObjectKind::Mesh {
            match mesh_blend_target(document, &doc, scene, object) {
                Ok(Some((b, slot, weight))) => {
                    let table = doc.get_blend_key_table(&b.key_table_id).unwrap();
                    let p = doc.get_parameter(&table.parameter_id).unwrap();
                    return writable(
                        format!("BlendShape delta · {} = {}", p.name, table.keys[slot]),
                        weight,
                    );
                }
                Ok(None) => (),
                Err(error) => return Ok(ControlDescription::ReadOnly(error.to_string())),
            }
        } else if let Some(hint) = deformer_blend_edit_hint(&doc, scene, object) {
            return Ok(ControlDescription::ReadOnly(hint));
        }
        let describe_binding =
            |axes: &[zipzi_core::BindingAxis], matches: bool| -> Result<ControlDescription> {
                if !matches {
                    return Ok(ControlDescription::ReadOnly(keyform_edit_hint(
                        &doc,
                        &scene.source.viewport_frame.parameters,
                        axes,
                    )));
                }
                writable(
                    format!(
                        "Keyform · {}",
                        axes.iter()
                            .filter(|a| !a.parameter_id.is_empty())
                            .map(|axis| {
                                let value = scene
                                    .parameter_values()
                                    .get(&axis.parameter_id)
                                    .copied()
                                    .unwrap_or(0.);
                                let name = doc
                                    .get_parameter(&axis.parameter_id)
                                    .map_or(axis.parameter_id.as_str(), |p| p.name.as_str());
                                format!("{name} = {value}")
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    1.,
                )
            };
        let at_keys = |axes: &[zipzi_core::BindingAxis], keys: &[f32]| {
            axes.iter().filter(|a| !a.parameter_id.is_empty()).count() == keys.len()
                && axes
                    .iter()
                    .filter(|a| !a.parameter_id.is_empty())
                    .zip(keys)
                    .all(|(axis, key)| {
                        scene
                            .parameter_values()
                            .get(&axis.parameter_id)
                            .is_some_and(|value| (value - key).abs() < 1e-6)
                    })
        };
        match object.kind {
            ObjectKind::Mesh => {
                ensure!(doc.get_mesh(&object.id).is_some(), "Mesh no longer exists");
                if let Some(b) = doc
                    .binding_for_mesh(&object.id)
                    .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                {
                    return describe_binding(
                        &b.axes,
                        b.keyforms.iter().any(|f| at_keys(&b.axes, &f.keys)),
                    );
                }
            }
            ObjectKind::Deformer => {
                ensure!(
                    doc.get_transform(&object.id).is_some(),
                    "Deformer no longer exists"
                );
                if let Some(b) = doc
                    .binding_for_scene(&object.id)
                    .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                {
                    return describe_binding(
                        &b.axes,
                        b.track.samples().any(|f| at_keys(&b.axes, f.keys)),
                    );
                }
            }
            _ => bail!("This canvas object has no editable control points"),
        }
        writable("Base shape".into(), 1.)
    }
    pub fn prepare(document: &Document, scene: &CanvasScene, object: &ObjectRef) -> Result<Self> {
        document.ensure_editable(std::slice::from_ref(object))?;
        let doc = document.model();
        let blend = if object.kind == ObjectKind::Mesh {
            mesh_blend_target(document, &doc, scene, object)?
        } else {
            if let Some(hint) = deformer_blend_edit_hint(&doc, scene, object) {
                bail!(hint);
            }
            None
        };
        let slot_for = |axes: &[zipzi_core::BindingAxis], points: Vec<Vec<f32>>| -> Result<usize> {
            points
                .iter()
                .position(|keys| {
                    keys.len() == axes.iter().filter(|a| !a.parameter_id.is_empty()).count()
                        && axes
                            .iter()
                            .filter(|a| !a.parameter_id.is_empty())
                            .zip(keys)
                            .all(|(a, k)| {
                                scene
                                    .source
                                    .viewport_frame
                                    .parameters
                                    .iter()
                                    .find(|p| p.id == a.parameter_id)
                                    .is_some_and(|p| (p.value - k).abs() < 1e-6)
                            })
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}",
                        keyform_edit_hint(&doc, &scene.source.viewport_frame.parameters, axes)
                    )
                })
        };
        let (original, ids, positions, slot, parent_id, rotation) = match object.kind {
            ObjectKind::Mesh => {
                let mesh = doc
                    .get_mesh(&object.id)
                    .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
                if let Some((binding, slot, _)) = blend {
                    let zipzi_core::DeltaKeyforms::Mesh(forms) = &binding.keyforms else {
                        unreachable!()
                    };
                    (
                        EditOriginal::BlendBinding(binding.clone()),
                        mesh.vertex_ids.clone(),
                        forms[slot].positions.clone(),
                        Some(slot),
                        mesh.deformer_id.clone(),
                        false,
                    )
                } else if let Some(binding) = doc
                    .binding_for_mesh(&object.id)
                    .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                {
                    let slot = slot_for(
                        &binding.axes,
                        binding.keyforms.iter().map(|f| f.keys.clone()).collect(),
                    )?;
                    (
                        EditOriginal::MeshBinding(binding.clone()),
                        mesh.vertex_ids.clone(),
                        binding.keyforms[slot].positions.clone(),
                        Some(slot),
                        mesh.deformer_id.clone(),
                        false,
                    )
                } else {
                    (
                        EditOriginal::Mesh(mesh.clone()),
                        mesh.vertex_ids.clone(),
                        mesh.base_positions.clone(),
                        None,
                        mesh.deformer_id.clone(),
                        false,
                    )
                }
            }
            ObjectKind::Deformer => {
                let t = doc
                    .get_transform(&object.id)
                    .ok_or_else(|| anyhow::anyhow!("Deformer no longer exists"))?;
                let (original, slot, positions, rotation) = if let Some(binding) = doc
                    .binding_for_scene(&object.id)
                    .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                {
                    let slot = slot_for(
                        &binding.axes,
                        binding.track.samples().map(|s| s.keys.to_vec()).collect(),
                    )?;
                    let sample = binding.track.sample(slot);
                    let rotation = t.rotation().is_some();
                    let positions = if rotation {
                        vec![Vec2::new(
                            sample.rotation.origin.x as f32,
                            sample.rotation.origin.y as f32,
                        )]
                    } else {
                        sample.positions.to_vec()
                    };
                    (
                        EditOriginal::SceneBinding(binding.clone()),
                        Some(slot),
                        positions,
                        rotation,
                    )
                } else if let Some(w) = t.warp() {
                    (
                        EditOriginal::Transform(t.clone()),
                        None,
                        w.points.clone(),
                        false,
                    )
                } else {
                    let p = t.rotation().unwrap().pose.origin;
                    (
                        EditOriginal::Transform(t.clone()),
                        None,
                        vec![Vec2::new(p.x as f32, p.y as f32)],
                        true,
                    )
                };
                let ids = (0..positions.len() as u32).collect();
                (
                    original,
                    ids,
                    positions,
                    slot,
                    t.parent().to_owned(),
                    rotation,
                )
            }
            _ => bail!("This canvas object has no editable control points"),
        };
        let warp = if object.kind == ObjectKind::Deformer && !rotation {
            let transform = doc.get_transform(&object.id).unwrap().clone();
            // Keep undriven bindings in the write set; only the selected form
            // determines whether this gesture edits a keyform or the base Warp.
            let binding = doc.binding_for_scene(&object.id).cloned();
            let warp = transform.warp().unwrap();
            let stored = binding
                .as_ref()
                .zip(slot)
                .map_or(&warp.bezier, |(b, slot)| {
                    let SceneTrack::Warp { keyforms, .. } = &b.track else {
                        unreachable!("a Warp edit has Warp keyforms")
                    };
                    &keyforms[slot].bezier
                });
            let controller = stored.clone().unwrap_or_else(|| {
                zipzi_core::WarpBezier::from_lattice(warp.rows, warp.columns, warp.quad, &positions)
            });
            Some(WarpEdit {
                transform,
                binding,
                controller,
            })
        } else {
            None
        };
        let original = if let Some(w) = &warp {
            let mut all = vec![EditOriginal::Transform(w.transform.clone())];
            if let Some(b) = &w.binding {
                all.push(EditOriginal::SceneBinding(b.clone()));
            }
            EditOriginal::Batch(all)
        } else {
            original
        };
        let mesh = if object.kind == ObjectKind::Mesh {
            let values = scene
                .source
                .viewport_frame
                .parameters
                .iter()
                .map(|p| (p.id.clone(), p.value))
                .collect();
            let local = zipzi_core::evaluation::authoring_mesh_positions(&doc, &object.id, &values)
                .map_err(|e| anyhow::anyhow!("{}", e.message))?;
            let geometry = scene
                .mesh(&object.id)
                .ok_or_else(|| anyhow::anyhow!("Mesh preview is unavailable"))?;
            Some(crate::mesh_edit::MeshEdit {
                local,
                displayed: geometry.vertices.iter().map(|(_, p)| *p).collect(),
                weight: blend.map_or(1., |(_, _, w)| w),
                glue: scene.glue_map.clone(),
                original_points: false,
            })
        } else {
            None
        };
        Ok(Self {
            object: object.clone(),
            original,
            ids,
            positions,
            slot,
            rotation,
            space: ControlSpace::new(document, scene, &parent_id)?,
            warp,
            mesh,
        })
    }
    pub fn to_model(&self, p: Vec2) -> ModelPoint {
        self.space.to_model(p)
    }
    fn from_model(&self, p: ModelPoint, initial: Vec2) -> Result<Vec2> {
        self.space.from_model(p, initial)
    }
    pub(crate) fn mesh_positions(
        &self,
        displacement: &[(usize, crate::ModelVector)],
    ) -> Result<Vec<Vec2>> {
        let mesh = self.mesh.as_ref().unwrap();
        let mut positions = self.positions.clone();
        for &(slot, delta) in displacement {
            if delta.x == 0. && delta.y == 0. {
                continue;
            }
            let initial = mesh.local[slot];
            let before = self.to_model(initial);
            let local = self.from_model(
                ModelPoint {
                    x: before.x + delta.x,
                    y: before.y + delta.y,
                },
                initial,
            )?;
            positions[slot].x += (local.x - initial.x) / mesh.weight;
            positions[slot].y += (local.y - initial.y) / mesh.weight;
        }
        ensure!(
            positions.iter().all(|p| p.x.is_finite() && p.y.is_finite()),
            "Cannot map this drag to finite source positions"
        );
        Ok(positions)
    }
    pub(crate) fn write_mesh(
        &self,
        edit: &mut zipzi_sdk::EditSession<'_>,
        positions: Vec<Vec2>,
    ) -> Result<()> {
        match &self.original {
            EditOriginal::Mesh(m) => {
                edit.update_positions(&m.id, &self.ids, &positions)?;
            }
            EditOriginal::MeshBinding(b) => {
                let mut form = b.keyforms[self.slot.unwrap()].clone();
                form.positions = positions;
                edit.set_mesh_keyform(&b.id, form)?;
            }
            EditOriginal::BlendBinding(b) => {
                let mut b = b.clone();
                let zipzi_core::DeltaKeyforms::Mesh(forms) = &mut b.keyforms else {
                    unreachable!()
                };
                forms[self.slot.unwrap()].positions = positions;
                edit.replace_blend_binding(b)?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    pub fn preview(
        &self,
        edit: &mut zipzi_sdk::EditSession<'_>,
        selected: &[u32],
        change: ControlChange,
    ) -> Result<()> {
        if let Some(warp) = &self.warp {
            return self.preview_warp(edit, selected, change, warp);
        }
        let mut positions = self.positions.clone();
        for (id, p) in self.ids.iter().zip(&mut positions) {
            if selected.contains(id)
                || matches!(
                    change,
                    ControlChange::Transform { .. }
                        | ControlChange::Rotation { .. }
                        | ControlChange::MoveAll(_)
                )
            {
                let before = self.to_model(*p);
                let desired = match change {
                    ControlChange::Translate(delta) | ControlChange::MoveAll(delta) => ModelPoint {
                        x: before.x + delta.x,
                        y: before.y + delta.y,
                    },
                    ControlChange::Transform {
                        pivot,
                        scale,
                        radians,
                        translation,
                    } => {
                        let (sin, cos) = radians.sin_cos();
                        let x = (before.x - pivot.x) * scale[0];
                        let y = (before.y - pivot.y) * scale[1];
                        ModelPoint {
                            x: pivot.x + x * cos - y * sin + translation.x,
                            y: pivot.y + x * sin + y * cos + translation.y,
                        }
                    }
                    ControlChange::Rotation { translation, .. } => ModelPoint {
                        x: before.x + translation.x,
                        y: before.y + translation.y,
                    },
                };
                // Preserve f64 origins and no-op history when no translation is requested.
                if desired != before {
                    *p = self.from_model(desired, *p)?;
                }
            }
        }
        let update_rotation = |pose: &mut zipzi_core::RotationPose| {
            pose.origin.x += (positions[0].x - self.positions[0].x) as f64;
            pose.origin.y += (positions[0].y - self.positions[0].y) as f64;
            if let ControlChange::Rotation { angle, scale, .. } = change {
                pose.angle += angle;
                pose.scale *= scale;
            }
        };
        if let ControlChange::Rotation { angle, scale, .. } = change {
            ensure!(
                angle.is_finite() && scale.is_finite() && scale > 0.,
                "Invalid rotation or scale"
            );
            let pose = match &self.original {
                EditOriginal::Transform(t) => t.rotation().unwrap().pose,
                EditOriginal::SceneBinding(b) => b.track.sample(self.slot.unwrap()).rotation,
                _ => bail!("Not a rotation deformer"),
            };
            ensure!(
                (0.0001..=100.).contains(&(pose.scale * scale)),
                "Rotation scale must be between 0.0001 and 100"
            );
            ensure!(
                (-3600. ..=3600.).contains(&(pose.angle + angle)),
                "Rotation angle must be between -3600 and 3600 degrees"
            );
        }
        match &self.original {
            EditOriginal::Batch(_) => unreachable!("a control target has a single baseline"),
            EditOriginal::BlendBinding(_) => return self.write_mesh(edit, positions),
            EditOriginal::Mesh(m) => {
                edit.update_positions(&m.id, &self.ids, &positions)?;
            }
            EditOriginal::MeshBinding(b) => {
                let mut form = b.keyforms[self.slot.unwrap()].clone();
                form.positions = positions;
                edit.set_mesh_keyform(&b.id, form)?;
            }
            EditOriginal::Transform(t) => {
                if self.rotation {
                    let mut r = t.rotation().unwrap().clone();
                    update_rotation(&mut r.pose);
                    edit.update_rotation(&t.id, r)?;
                } else {
                    edit.update_warp_points(&t.id, positions)?;
                }
            }
            EditOriginal::SceneBinding(b) => {
                let form = match &b.track {
                    SceneTrack::Warp { keyforms, .. } => {
                        let mut f = keyforms[self.slot.unwrap()].clone();
                        f.positions = positions;
                        zipzi_core::SceneKeyform::Warp(f)
                    }
                    SceneTrack::Rotation { keyforms, .. } => {
                        let mut f = keyforms[self.slot.unwrap()].clone();
                        update_rotation(&mut f.rotation);
                        zipzi_core::SceneKeyform::Rotation(f)
                    }
                    _ => unreachable!(),
                };
                edit.set_scene_keyform(&b.id, form)?;
            }
        }
        Ok(())
    }
    pub fn rotation_delta(&self, start: ModelPoint, end: ModelPoint) -> Result<f32> {
        let origin = self.to_model(self.positions[0]);
        let a = (start.y - origin.y).atan2(start.x - origin.x);
        let b = (end.y - origin.y).atan2(end.x - origin.x);
        let mut delta = (b - a).to_degrees();
        if delta > 180. {
            delta -= 360.
        }
        if delta < -180. {
            delta += 360.
        }
        Ok(delta)
    }
}

impl ControlSpace {
    pub fn new(document: &Document, scene: &CanvasScene, parent_id: &str) -> Result<Self> {
        let parent_shape = document
            .model()
            .get_transform(parent_id)
            .and_then(|t| t.warp())
            .map(|w| (w.rows as usize, w.columns as usize, w.quad));
        Self::from_scene(scene, parent_id, parent_shape)
    }
    pub(crate) fn from_scene(
        scene: &CanvasScene,
        parent_id: &str,
        parent_shape: Option<(usize, usize, bool)>,
    ) -> Result<Self> {
        let parent = if parent_id.is_empty() {
            None
        } else {
            Some(
                scene
                    .control(parent_id)
                    .filter(|c| c.enabled)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Parent deformer is unavailable"))?,
            )
        };
        let parent_points = parent.as_ref().map_or_else(Vec::new, |p| {
            p.points.iter().flat_map(|p| [p.x, p.y]).collect()
        });
        Ok(Self {
            parent,
            parent_shape,
            parent_points,
        })
    }
    pub fn to_model(&self, p: Vec2) -> ModelPoint {
        let Some(parent) = &self.parent else {
            return ModelPoint { x: p.x, y: p.y };
        };
        if parent.kind == ControlKind::Rotation {
            let [o, x, y, ..] = parent.points.as_slice() else {
                return ModelPoint {
                    x: f32::NAN,
                    y: f32::NAN,
                };
            };
            ModelPoint {
                x: o.x + p.x * (x.x - o.x) + p.y * (y.x - o.x),
                y: o.y + p.x * (x.y - o.y) + p.y * (y.y - o.y),
            }
        } else {
            let (rows, columns, quad) = self.parent_shape.unwrap();
            let mut p = [p];
            let input = [p[0].x, p[0].y];
            let mut output = [0., 0.];
            zipzi_core::deformers::warp_points(
                rows as i32,
                columns as i32,
                quad,
                &self.parent_points,
                &input,
                &mut output,
                1,
            );
            p[0] = Vec2::new(output[0], output[1]);
            ModelPoint {
                x: p[0].x,
                y: p[0].y,
            }
        }
    }
    pub fn from_model(&self, desired: ModelPoint, initial: Vec2) -> Result<Vec2> {
        self.from_model_with_tolerance(desired, initial, 0.001)
    }
    pub(crate) fn from_model_with_tolerance(
        &self,
        desired: ModelPoint,
        initial: Vec2,
        tolerance: f32,
    ) -> Result<Vec2> {
        if self.parent.is_none() {
            return Ok(Vec2::new(desired.x, desired.y));
        }
        let mut p = initial;
        for _ in 0..24 {
            let a = self.to_model(p);
            let (dx, dy) = (desired.x - a.x, desired.y - a.y);
            if dx.hypot(dy) < tolerance {
                return Ok(p);
            }
            let h = 0.001;
            let x = self.to_model(Vec2::new(p.x + h, p.y));
            let y = self.to_model(Vec2::new(p.x, p.y + h));
            let (xx, xy, yx, yy) = (
                (x.x - a.x) / h,
                (x.y - a.y) / h,
                (y.x - a.x) / h,
                (y.y - a.y) / h,
            );
            let det = xx * yy - xy * yx;
            ensure!(
                det.is_finite() && det.abs() > 1e-8,
                "The parent deformation is singular at this point"
            );
            p.x += (dx * yy - dy * yx) / det;
            p.y += (dy * xx - dx * xy) / det;
            ensure!(p.x.is_finite() && p.y.is_finite(), "Invalid local position");
        }
        bail!("Cannot map this drag through the parent deformation")
    }
}

impl ControlEdit {
    fn changed_point(&self, p: Vec2, change: ControlChange) -> Result<Vec2> {
        let before = self.to_model(p);
        let desired = match change {
            ControlChange::Translate(d) | ControlChange::MoveAll(d) => ModelPoint {
                x: before.x + d.x,
                y: before.y + d.y,
            },
            ControlChange::Transform {
                pivot,
                scale,
                radians,
                translation,
            } => {
                let (s, c) = radians.sin_cos();
                let x = (before.x - pivot.x) * scale[0];
                let y = (before.y - pivot.y) * scale[1];
                ModelPoint {
                    x: pivot.x + x * c - y * s + translation.x,
                    y: pivot.y + x * s + y * c + translation.y,
                }
            }
            _ => bail!("Invalid Warp operation"),
        };
        if desired == before {
            Ok(p)
        } else {
            self.from_model(desired, p)
        }
    }
    fn preview_warp(
        &self,
        edit: &mut zipzi_sdk::EditSession<'_>,
        selected: &[u32],
        change: ControlChange,
        original: &WarpEdit,
    ) -> Result<()> {
        let mut controller = original.controller.clone();
        let all = matches!(
            change,
            ControlChange::MoveAll(_) | ControlChange::Transform { .. }
        );
        let bezier = selected
            .iter()
            .any(|id| crate::warp_bezier::component(*id).is_some());
        let handles = selected
            .iter()
            .any(|id| crate::warp_bezier::component(*id).is_some_and(|(_, part)| part > 0));
        let mut transform = original.transform.clone();
        let mut binding = original.binding.clone();
        let old = original.transform.warp().unwrap();
        for (i, n) in controller.nodes.iter_mut().enumerate() {
            let anchor = all || selected.contains(&crate::warp_bezier::component_id(i, 0));
            if anchor {
                n.position = self.changed_point(n.position, change)?;
            }
            for h in 0..4 {
                if anchor || selected.contains(&crate::warp_bezier::component_id(i, h + 1)) {
                    n.handles[h] = self.changed_point(n.handles[h], change)?;
                }
            }
        }
        // Returning to the press position restores absent metadata and grid size too.
        if (bezier || all) && controller == original.controller {
            return Ok(edit.replace_warp_authoring(transform, binding)?);
        }
        // Uniform integer refinement preserves existing bilinear cells and
        // triangle diagonals, including untouched keyforms.
        let factor = if handles {
            4u32.div_ceil(old.rows)
                .max(4u32.div_ceil(old.columns))
                .max(1)
        } else {
            1
        };
        let rows = old.rows * factor;
        let columns = old.columns * factor;
        let resample = |points: &[Vec2]| -> Vec<Vec2> {
            if rows == old.rows && columns == old.columns {
                return points.to_vec();
            }
            let source: Vec<_> = points.iter().flat_map(|p| [p.x, p.y]).collect();
            let input: Vec<_> = (0..=rows)
                .flat_map(|y| {
                    (0..=columns)
                        .flat_map(move |x| [x as f32 / columns as f32, y as f32 / rows as f32])
                })
                .collect();
            let mut out = vec![0.; input.len()];
            zipzi_core::deformers::warp_points(
                old.rows as i32,
                old.columns as i32,
                old.quad,
                &source,
                &input,
                &mut out,
                input.len() / 2,
            );
            out.chunks_exact(2).map(|p| Vec2::new(p[0], p[1])).collect()
        };
        let baseline = resample(&self.positions);
        let positions = if all {
            baseline
                .into_iter()
                .map(|p| self.changed_point(p, change))
                .collect::<Result<Vec<_>>>()?
        } else if bezier {
            controller.apply_delta(&original.controller, rows, columns, &baseline)
        } else {
            baseline
                .into_iter()
                .enumerate()
                .map(|(i, p)| {
                    if selected.contains(&(i as u32)) {
                        self.changed_point(p, change)
                    } else {
                        Ok(p)
                    }
                })
                .collect::<Result<Vec<_>>>()?
        };
        if positions == self.positions
            && rows == old.rows
            && columns == old.columns
            && !all
            && !bezier
        {
            return Ok(edit.replace_warp_authoring(transform, binding)?);
        }
        let stored = if bezier || all {
            Some(controller)
        } else {
            None
        };
        let zipzi_core::TransformData::Warp(w) = &mut transform.data else {
            unreachable!()
        };
        w.rows = rows;
        w.columns = columns;
        w.points = resample(&old.points);
        if let Some((b, slot)) = binding.as_mut().zip(self.slot) {
            let SceneTrack::Warp { keyforms, .. } = &mut b.track else {
                unreachable!()
            };
            for f in keyforms.iter_mut() {
                f.positions = resample(&f.positions);
            }
            let f = &mut keyforms[slot];
            f.positions = positions;
            f.bezier = stored;
        } else {
            w.points = positions;
            w.bezier = stored;
        }
        Ok(edit.replace_warp_authoring(transform, binding)?)
    }
}
