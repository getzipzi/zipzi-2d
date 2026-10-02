//! Fixed authoring target for one pointer gesture. Selection and evaluated pixels
//! never become implicit write destinations during a drag.
use crate::{
    CanvasScene, ControlGeometry, ControlKind, Document, ModelPoint, ObjectKind, ObjectRef,
};
use anyhow::{Result, bail, ensure};
use kasane_core::{Mesh, MeshBinding, SceneBinding, SceneTrack, Transform, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ControlChange {
    Translate(ModelPoint),
    Transform {
        pivot: ModelPoint,
        scale: [f32; 2],
        radians: f32,
        translation: ModelPoint,
    },
    Rotation {
        angle: f32,
        scale: f32,
        translation: ModelPoint,
    },
    MoveAll(ModelPoint),
}

#[derive(Clone)]
pub(crate) enum EditOriginal {
    Mesh(Mesh),
    Transform(Transform),
    MeshBinding(MeshBinding),
    SceneBinding(SceneBinding),
    Batch(Vec<EditOriginal>),
}
impl EditOriginal {
    pub fn id(&self) -> &str {
        match self {
            Self::Mesh(m) => &m.id,
            Self::Transform(t) => &t.id,
            Self::MeshBinding(b) => &b.id,
            Self::SceneBinding(b) => &b.id,
            Self::Batch(v) => v[0].id(),
        }
    }
    pub fn unchanged(&self, doc: &kasane_core::Document) -> bool {
        match self {
            Self::Mesh(v) => doc.get_mesh(&v.id) == Some(v),
            Self::Transform(v) => doc.get_transform(&v.id) == Some(v),
            Self::MeshBinding(v) => doc.get_binding(&v.id) == Some(v),
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
}
#[derive(Clone)]
pub(crate) struct WarpEdit {
    transform: Transform,
    binding: Option<SceneBinding>,
    controller: kasane_core::WarpBezier,
}
#[derive(Clone)]
pub(crate) struct ControlSpace {
    pub parent: Option<ControlGeometry>,
    pub parent_shape: Option<(usize, usize, bool)>,
    parent_points: Vec<f32>,
    canvas: kasane_core::Canvas,
}

impl ControlEdit {
    /// Read-only capability query for the overlay. Never clones authoring
    /// meshes or entire keyform tables on pointer/hover redraws.
    pub fn describe(
        document: &Document,
        scene: &CanvasScene,
        object: &ObjectRef,
    ) -> Result<String> {
        let doc = document.model();
        ensure!(
            doc.blend_bindings_for_target(&object.id).is_empty(),
            "BlendShape editing requires an explicit blend target"
        );
        let describe_binding =
            |axes: &[kasane_core::BindingAxis], matches: bool| -> Result<String> {
                ensure!(axes.len() <= 2, "This legacy binding is read-only");
                ensure!(
                    matches,
                    "Select a stored keyform before editing this object"
                );
                Ok(format!(
                    "Keyform · {}",
                    axes.iter()
                        .map(|axis| {
                            let value = scene
                                .source
                                .frame
                                .parameters
                                .iter()
                                .find(|p| p.id == axis.parameter_id)
                                .map_or(0., |p| p.value);
                            let name = doc
                                .get_parameter(&axis.parameter_id)
                                .map_or(axis.parameter_id.as_str(), |p| p.name.as_str());
                            format!("{name} = {value}")
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            };
        let at_keys = |axes: &[kasane_core::BindingAxis], keys: &[f32]| {
            axes.len() == keys.len()
                && axes.iter().zip(keys).all(|(axis, key)| {
                    scene
                        .source
                        .frame
                        .parameters
                        .iter()
                        .find(|p| p.id == axis.parameter_id)
                        .is_some_and(|p| (p.value - key).abs() < 1e-6)
                })
        };
        match object.kind {
            ObjectKind::Mesh => {
                ensure!(doc.get_mesh(&object.id).is_some(), "Mesh no longer exists");
                ensure!(
                    doc.glues_for_mesh(&object.id).is_empty(),
                    "Meshes connected by Glue are read-only in the vertex tool"
                );
                if let Some(b) = doc.binding_for_mesh(&object.id) {
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
                if let Some(b) = doc.binding_for_scene(&object.id) {
                    return describe_binding(
                        &b.axes,
                        b.track.samples().any(|f| at_keys(&b.axes, f.keys)),
                    );
                }
            }
            _ => bail!("This canvas object has no editable control points"),
        }
        Ok("Base shape".into())
    }
    pub fn prepare(document: &Document, scene: &CanvasScene, object: &ObjectRef) -> Result<Self> {
        let doc = document.model();
        ensure!(
            doc.blend_bindings_for_target(&object.id).is_empty(),
            "BlendShape editing requires an explicit blend target"
        );
        let slot_for =
            |axes: &[kasane_core::BindingAxis], points: Vec<Vec<f32>>| -> Result<usize> {
                ensure!(axes.len() <= 2, "This legacy binding is read-only");
                points
                    .iter()
                    .position(|keys| {
                        keys.len() == axes.len()
                            && axes.iter().zip(keys).all(|(a, k)| {
                                scene
                                    .source
                                    .frame
                                    .parameters
                                    .iter()
                                    .find(|p| p.id == a.parameter_id)
                                    .is_some_and(|p| (p.value - k).abs() < 1e-6)
                            })
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!("Select a stored keyform before editing this object")
                    })
            };
        let (original, ids, positions, slot, parent_id, rotation) = match object.kind {
            ObjectKind::Mesh => {
                let mesh = doc
                    .get_mesh(&object.id)
                    .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
                ensure!(
                    doc.glues_for_mesh(&object.id).is_empty(),
                    "Meshes connected by Glue are read-only in the vertex tool"
                );
                if let Some(binding) = doc.binding_for_mesh(&object.id) {
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
                let (original, slot, positions, rotation) =
                    if let Some(binding) = doc.binding_for_scene(&object.id) {
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
            let binding = doc.binding_for_scene(&object.id).cloned();
            let controller = crate::warp_bezier::authored(document, scene, object)
                .unwrap()
                .0;
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
        Ok(Self {
            object: object.clone(),
            original,
            ids,
            positions,
            slot,
            rotation,
            space: ControlSpace::new(document, scene, &parent_id)?,
            warp,
        })
    }
    pub fn to_model(&self, p: Vec2) -> ModelPoint {
        self.space.to_model(p)
    }
    fn from_model(&self, p: ModelPoint, initial: Vec2) -> Result<Vec2> {
        self.space.from_model(p, initial)
    }
    pub fn preview(
        &self,
        edit: &mut kasane_sdk::EditSession<'_>,
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
        let update_rotation = |pose: &mut kasane_core::RotationPose| {
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
                        kasane_core::SceneKeyform::Warp(f)
                    }
                    SceneTrack::Rotation { keyforms, .. } => {
                        let mut f = keyforms[self.slot.unwrap()].clone();
                        update_rotation(&mut f.rotation);
                        kasane_core::SceneKeyform::Rotation(f)
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
        // The evaluated rotation adds the parent angle; changing its authored
        // angle changes the displayed angle by the same amount, even under warp.
        // Its screen-space sign also includes the final canvas Y reversal.
        Ok(if self.space.canvas.flag & 1 == 0 {
            delta
        } else {
            -delta
        })
    }
}

impl ControlSpace {
    pub fn new(document: &Document, scene: &CanvasScene, parent_id: &str) -> Result<Self> {
        let doc = document.model();
        let parent = if parent_id.is_empty() {
            None
        } else {
            Some(
                scene
                    .controls
                    .iter()
                    .find(|c| c.object.id == parent_id && c.enabled)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Parent deformer is unavailable"))?,
            )
        };
        let parent_shape = doc
            .get_transform(parent_id)
            .and_then(|t| t.warp())
            .map(|w| (w.rows as usize, w.columns as usize, w.quad));
        let parent_points = parent.as_ref().map_or_else(Vec::new, |p| {
            p.points.iter().flat_map(|p| [p.x, p.y]).collect()
        });
        Ok(Self {
            parent,
            parent_shape,
            parent_points,
            canvas: scene.source.frame.canvas,
        })
    }
    pub fn to_model(&self, p: Vec2) -> ModelPoint {
        let Some(parent) = &self.parent else {
            return ModelPoint {
                x: p.x,
                y: if self.canvas.flag & 1 == 0 {
                    2. * self.canvas.origin.y - p.y
                } else {
                    p.y
                },
            };
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
            kasane_core::deformers::warp_points(
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
        if self.parent.is_none() {
            // Canvas Y reversal is its own inverse for root authored positions.
            let source = self.to_model(Vec2::new(desired.x, desired.y));
            return Ok(Vec2::new(source.x, source.y));
        }
        let mut p = initial;
        for _ in 0..24 {
            let a = self.to_model(p);
            let (dx, dy) = (desired.x - a.x, desired.y - a.y);
            if dx.hypot(dy) < 0.001 {
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
        edit: &mut kasane_sdk::EditSession<'_>,
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
            kasane_core::deformers::warp_points(
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
        let kasane_core::TransformData::Warp(w) = &mut transform.data else {
            unreachable!()
        };
        w.rows = rows;
        w.columns = columns;
        w.points = resample(&old.points);
        if let Some(b) = &mut binding {
            let SceneTrack::Warp { keyforms, .. } = &mut b.track else {
                unreachable!()
            };
            for f in keyforms.iter_mut() {
                f.positions = resample(&f.positions);
            }
            let f = &mut keyforms[self.slot.unwrap()];
            f.positions = positions;
            f.bezier = stored;
        } else {
            w.points = positions;
            w.bezier = stored;
        }
        Ok(edit.replace_warp_authoring(transform, binding)?)
    }
}
