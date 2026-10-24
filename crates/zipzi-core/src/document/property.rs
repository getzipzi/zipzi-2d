//! Atomic scalar authoring patches. Geometry, identities and relation edges are fixed.
use super::*;
use crate::preview::PreviewState;
use crate::{Appearance, PreviewValues};

#[derive(Debug, Clone)]
pub enum PropertyPatch {
    Mesh {
        id: String,
        keyform: Option<(String, Vec<String>)>,
        appearance: Appearance,
        draw_order: Option<f32>,
    },
    Parameter {
        id: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        decimal_places: i32,
    },
    Part {
        id: String,
        draw_order: f32,
    },
    Glue {
        id: String,
        intensity: f32,
    },
    Motion {
        id: String,
        duration: f32,
        fps: f32,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    },
    Expression {
        id: String,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    },
    Export {
        padding_pixels: f32,
        minimum_extent: f32,
    },
}

impl PropertyPatch {
    fn current(&self, doc: &Document) -> Result<Self, Status> {
        let missing = || {
            Status::error(
                "MISSING_PROPERTY_TARGET",
                "Property target no longer exists",
            )
        };
        Ok(match self {
            Self::Mesh { id, keyform, .. } => {
                let mesh = doc.get_mesh(id).ok_or_else(missing)?;
                let (appearance, draw_order) = if let Some((binding, samples)) = keyform {
                    let binding = doc
                        .get_binding(binding)
                        .filter(|b| b.mesh_id == *id)
                        .ok_or_else(missing)?;
                    let form = binding
                        .keyforms
                        .iter()
                        .find(|f| {
                            f.samples.len() == samples.len()
                                && f.samples.iter().all(|id| samples.contains(id))
                        })
                        .ok_or_else(missing)?;
                    (form.appearance, form.draw_order)
                } else {
                    (mesh.appearance, mesh.draw_order)
                };
                Self::Mesh {
                    id: id.clone(),
                    keyform: keyform.clone(),
                    appearance,
                    draw_order,
                }
            }
            Self::Parameter { id, .. } => {
                let p = doc.get_parameter(id).ok_or_else(missing)?;
                Self::Parameter {
                    id: id.clone(),
                    minimum: p.minimum,
                    maximum: p.maximum,
                    default_value: p.default_value,
                    decimal_places: p.decimal_places,
                }
            }
            Self::Part { id, .. } => Self::Part {
                id: id.clone(),
                draw_order: doc.get_part(id).ok_or_else(missing)?.draw_order,
            },
            Self::Glue { id, .. } => Self::Glue {
                id: id.clone(),
                intensity: doc.get_glue(id).ok_or_else(missing)?.intensity,
            },
            Self::Motion { id, .. } => {
                let c = doc.get_motion(id).ok_or_else(missing)?;
                Self::Motion {
                    id: id.clone(),
                    duration: c.duration,
                    fps: c.fps,
                    fade_in: c.fade_in,
                    fade_out: c.fade_out,
                }
            }
            Self::Expression { id, .. } => {
                let e = doc.get_expression(id).ok_or_else(missing)?;
                Self::Expression {
                    id: id.clone(),
                    fade_in: e.fade_in,
                    fade_out: e.fade_out,
                }
            }
            Self::Export { .. } => {
                let e = doc.export_settings();
                Self::Export {
                    padding_pixels: e.padding_pixels,
                    minimum_extent: e.minimum_extent,
                }
            }
        })
    }
}

impl Document {
    /// Apply a typed field patch without copying unrelated persistent data.
    pub fn patch_property(&mut self, patch: &PropertyPatch) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if let Err(status) = patch.current(self) {
            return self.failed(status);
        }
        let target = match patch {
            PropertyPatch::Mesh { id, keyform, .. } => {
                if let Some((binding, _)) = keyform {
                    if let Err(s) = self.authoring_unlocked(binding) {
                        return self.failed(s);
                    }
                }
                Some(id)
            }
            PropertyPatch::Parameter { id, .. }
            | PropertyPatch::Part { id, .. }
            | PropertyPatch::Glue { id, .. } => Some(id),
            _ => None,
        };
        if let Some(id) = target {
            if let Err(s) = self.authoring_unlocked(id) {
                return self.failed(s);
            }
        }
        match patch {
            PropertyPatch::Mesh {
                id,
                keyform,
                appearance,
                draw_order,
            } => {
                let s = validate_appearance(appearance, id);
                if !s.is_ok() {
                    return self.failed(s);
                }
                if let Some(order) = draw_order {
                    let s = validate_draw_order(*order, id);
                    if !s.is_ok() {
                        return self.failed(s);
                    }
                }
                let order_changed;
                let mut objects = vec![id.clone()];
                let tracked = if let Some((binding, _)) = keyform {
                    Some(self.track_binding_payload_write(binding))
                } else {
                    (self.get_mesh(id).unwrap().draw_order == *draw_order)
                        .then(|| self.track_mesh_write(vec![id.clone()]))
                };
                if let Some((binding, samples)) = keyform {
                    let form = self
                        .content
                        .bindings
                        .get_mut(binding)
                        .unwrap()
                        .keyforms
                        .iter_mut()
                        .find(|f| {
                            f.samples.len() == samples.len()
                                && f.samples.iter().all(|id| samples.contains(id))
                        })
                        .unwrap();
                    order_changed = form.draw_order != *draw_order;
                    form.appearance = *appearance;
                    form.draw_order = *draw_order;
                    objects.push(binding.clone());
                } else {
                    let mesh = self.content.meshes.get_mut(id).unwrap();
                    order_changed = mesh.draw_order != *draw_order;
                    mesh.appearance = *appearance;
                    mesh.draw_order = *draw_order;
                }
                let kind = if order_changed {
                    ChangeKind::Structure
                } else {
                    ChangeKind::Appearance
                };
                if let Some(write) = tracked {
                    self.changed_tracked(write, kind, vec![id.clone()], objects)
                } else {
                    self.changed(kind, vec![id.clone()], objects)
                }
            }
            PropertyPatch::Parameter {
                id,
                minimum,
                maximum,
                default_value,
                decimal_places,
            } => {
                let mut p = self.get_parameter(id).unwrap().clone();
                p.minimum = *minimum;
                p.maximum = *maximum;
                p.default_value = *default_value;
                p.decimal_places = *decimal_places;
                self.replace_parameter(p)
            }
            PropertyPatch::Part { id, draw_order } => {
                let mut p = self.get_part(id).unwrap().clone();
                p.draw_order = *draw_order;
                self.replace_part(p)
            }
            PropertyPatch::Glue { id, intensity } => {
                let mut g = self.get_glue(id).unwrap().clone();
                g.intensity = *intensity;
                self.replace_glue(g)
            }
            PropertyPatch::Motion {
                id,
                duration,
                fps,
                fade_in,
                fade_out,
            } => self.patch_motion_properties(id, *duration, *fps, *fade_in, *fade_out),
            PropertyPatch::Expression {
                id,
                fade_in,
                fade_out,
            } => {
                if self.editor_state().get(id).locked {
                    return self.failed(Status::error("OBJECT_LOCKED", id));
                }
                if [fade_in, fade_out]
                    .iter()
                    .any(|fade| fade.is_some_and(|v| !v.is_finite() || v < 0.))
                {
                    return self.failed(Status::error("INVALID_EXPRESSION_FADE", id));
                }
                let e = self.content.expressions.get_mut(id).unwrap();
                if e.fade_in == *fade_in && e.fade_out == *fade_out {
                    return self.failed(Status::ok());
                }
                e.fade_in = *fade_in;
                e.fade_out = *fade_out;
                self.changed(ChangeKind::Metadata, vec![], vec![id.clone()])
            }
            PropertyPatch::Export {
                padding_pixels,
                minimum_extent,
            } => {
                let mut e = self.export_settings();
                e.padding_pixels = *padding_pixels;
                e.minimum_extent = *minimum_extent;
                self.replace_export_settings(e)
            }
        }
    }

    /// Update and evaluate one scalar target. A rejection preserves persistent
    /// content, revisions, preview values and the last published preview frame.
    pub fn preview_property(
        &mut self,
        patch: &PropertyPatch,
        preview: &mut PreviewState,
        values: PreviewValues,
    ) -> EditResult {
        let original = match patch.current(self) {
            Ok(p) => p,
            Err(s) => return self.failed(s),
        };
        let snapshot_identity = self.snapshot_identity.clone();
        let tracking = self.tracking.for_rollback();
        // Structure changes can also prune stale imported authoring flags in
        // changed(). Keep that persistent state inside the same rollback as the
        // scalar and coverage. Appearance-only drags never clone this map.
        let may_prune_flags = match patch {
            PropertyPatch::Mesh { draw_order, .. } => {
                matches!(&original, PropertyPatch::Mesh { draw_order: old, .. } if old != draw_order)
            }
            PropertyPatch::Parameter { .. }
            | PropertyPatch::Part { .. }
            | PropertyPatch::Glue { .. } => true,
            PropertyPatch::Motion { .. }
            | PropertyPatch::Expression { .. }
            | PropertyPatch::Export { .. } => false,
        };
        let editor_state = may_prune_flags.then(|| self.content.editor_state.clone());
        let revision = self.revision;
        let evaluation_revision = self.evaluation_revision;
        let modified = self.modified_cache.get().copied();
        let result = self.patch_property(patch);
        if !result.status.is_ok() {
            return result;
        }
        if let Err(status) = preview.replace(self, 0, values) {
            let restored = self.patch_property(&original);
            assert!(
                restored.status.is_ok(),
                "Previously valid scalar property must be restorable"
            );
            if let Some(editor_state) = editor_state {
                self.content.editor_state = editor_state;
            }
            self.snapshot_identity = snapshot_identity;
            self.tracking = tracking;
            self.revision = revision;
            self.evaluation_revision = evaluation_revision;
            self.modified_cache.take();
            if let Some(modified) = modified {
                let _ = self.modified_cache.set(modified);
            }
            return self.failed(status);
        }
        result
    }
}
