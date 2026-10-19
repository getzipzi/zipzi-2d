//! Drag preparation and candidate updates against the captured press frame.
use super::ToolController;
use crate::control_edit::{ControlChange, ControlEdit};
use crate::{Document, HitTarget, ViewportPoint};
use anyhow::{Result, ensure};

impl ToolController {
    pub(super) fn update(&mut self, document: &mut Document, point: ViewportPoint) -> Result<bool> {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid pointer position"
        );
        let Some(press) = self.press.as_mut() else {
            return Ok(false);
        };
        press.current = point;
        press.dragged |= crate::picking::distance2(press.point, point) >= 9.;
        let Some(target) = press
            .target
            .as_ref()
            .filter(|h| !matches!(h, HitTarget::Object(_)))
        else {
            return Ok(false);
        };
        if press.edit.is_none() && crate::picking::distance2(press.point, point) < 9. {
            return Ok(false);
        }
        if press.edit.is_none() {
            document.ensure_editable(&document.context().selections)?;
            let mut targets = vec![target.object().clone()];
            if matches!(target, HitTarget::Component(_)) {
                for c in &self.components {
                    if press.scene.in_isolation_scope(&c.object) && !targets.contains(&c.object) {
                        targets.push(c.object.clone());
                    }
                }
            }
            ensure!(
                !targets.iter().any(|r| press
                    .scene
                    .ancestors(r)
                    .iter()
                    .any(|parent| targets.contains(parent))),
                "Select controls from one deformation level at a time"
            );
            let edits = targets
                .iter()
                .map(|r| ControlEdit::prepare(document, &press.scene, r))
                .collect::<Result<Vec<_>>>()?;
            document.begin_control_edit(&edits)?;
            press.edit = Some(edits);
        }
        let edits = press.edit.as_ref().unwrap();
        let start = press.transform.to_model(press.point);
        let end = press.transform.to_model(point);
        let delta = crate::ModelVector {
            x: end.x - start.x,
            y: end.y - start.y,
        };
        let change = match target {
            HitTarget::RotationHandle(_) => ControlChange::Rotation {
                translation: crate::ModelVector::default(),
                angle: edits[0].rotation_delta(start, end)?,
                scale: 1.,
            },
            HitTarget::DeformerHandle(object, handle) => {
                use crate::DeformerHandle as H;
                let control = press
                    .scene
                    .controls
                    .iter()
                    .find(|c| &c.object == object)
                    .unwrap();
                let gizmo = press
                    .scene
                    .deformer_gizmo(control, press.transform)
                    .unwrap();
                match handle {
                    H::Move => ControlChange::MoveAll(delta),
                    H::Rotate => ControlChange::Rotation {
                        translation: crate::ModelVector::default(),
                        angle: edits[0].rotation_delta(start, end)?,
                        scale: 1.,
                    },
                    H::RotationScale => {
                        let center = press.transform.to_model(gizmo.center);
                        let before = (start.x - center.x).hypot(start.y - center.y);
                        let after = (end.x - center.x).hypot(end.y - center.y);
                        ensure!(before > 1e-6, "Rotation scale handle is unavailable");
                        ControlChange::Rotation {
                            translation: crate::ModelVector::default(),
                            angle: 0.,
                            scale: (after / before).max(0.0001),
                        }
                    }
                    H::Scale(index) => {
                        let (corner, anchor) = gizmo.scale_anchor(*index);
                        let ratio = |d: f32, span: f32| {
                            if span.abs() < 1e-5 {
                                1.
                            } else {
                                (1. + d / span).max(0.001)
                            }
                        };
                        let sx = ratio(point.x - press.point.x, corner.x - anchor.x);
                        let sy = ratio(point.y - press.point.y, corner.y - anchor.y);
                        let scale = if press.shift && index % 2 == 0 {
                            let s = if (sx - 1.).abs() > (sy - 1.).abs() {
                                sx
                            } else {
                                sy
                            };
                            [s, s]
                        } else {
                            [sx, sy]
                        };
                        ControlChange::Transform {
                            translation: crate::ModelVector::default(),
                            pivot: press.transform.to_model(anchor),
                            scale,
                            radians: 0.,
                        }
                    }
                }
            }
            _ => ControlChange::Translate(delta),
        };
        if press.last == Some(change) {
            return Ok(false);
        }
        document.preview_control_edit(edits, &self.components, change)?;
        press.last = Some(change);
        Ok(true)
    }
}
