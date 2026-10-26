//! Control-point and gizmo gestures: candidate ownership, transforms and feedback.
use super::{OverlaySnapshot, gesture::Press, hints::*};
use crate::ComponentRef;
use crate::control_edit::{ControlChange, ControlEdit};
use crate::{Document, FrameVersion, HitTarget};
use anyhow::{Result, ensure};

pub(super) struct ControlGesture {
    pub press: Press,
    pub target: HitTarget,
    edit: Option<ActiveEdit>,
}
struct ActiveEdit {
    edits: Vec<ControlEdit>,
    last: Option<ControlChange>,
    constrained: Option<String>,
}
impl ControlGesture {
    pub fn new(press: Press, target: HitTarget) -> Self {
        Self {
            press,
            target,
            edit: None,
        }
    }
    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }
    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        let origin = self.press.version;
        self.editing()
            && version.binding.document == origin.binding.document
            && version.scene > origin.scene
            && (version.binding.viewport != origin.binding.viewport
                || (version.viewport == origin.viewport && version.request > origin.request))
    }
    pub fn update(&mut self, document: &mut Document, components: &[ComponentRef]) -> Result<bool> {
        let point = self.press.current;
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid pointer position"
        );
        let press = &self.press;
        let target = &self.target;
        if self.edit.is_none() && crate::picking::distance2(press.point, point) < 9. {
            return Ok(false);
        }
        if self.edit.is_none() {
            document.ensure_editable(&document.context().selections)?;
            let mut targets = vec![target.object().clone()];
            if matches!(target, HitTarget::Component(_)) {
                for c in components {
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
            let mut edits = targets
                .iter()
                .map(|r| ControlEdit::prepare(document, &press.scene, r))
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                !targets
                    .iter()
                    .any(|r| r.kind == crate::ObjectKind::Deformer)
                    || !targets.iter().any(|r| r.kind == crate::ObjectKind::Mesh
                        && !document.model().glues_for_mesh(&r.id).is_empty()),
                "Edit Glue-connected meshes separately from deformer controls"
            );
            for target in &mut edits {
                if let Some(mesh) = &mut target.mesh {
                    mesh.original_points = press.original_points;
                }
            }
            document.begin_control_edit(&edits)?;
            self.edit = Some(ActiveEdit {
                edits,
                last: None,
                constrained: None,
            });
        }
        let edit = self.edit.as_mut().unwrap();
        let edits = &edit.edits;
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
        if edit.last == Some(change) {
            edit.constrained = None;
            return Ok(false);
        }
        if let Err(error) = document.preview_control_edit(edits, components, change) {
            if edit.last.is_none() || !error.is::<crate::mesh_edit::PreviewConstraint>() {
                return Err(error);
            }
            edit.constrained = Some(error.to_string());
            return Ok(false);
        }
        edit.constrained = None;
        edit.last = Some(change);
        Ok(true)
    }
    pub fn finish(&mut self, document: &mut Document) -> Result<()> {
        if self.editing() {
            document.finish_edit()?;
        }
        self.edit = None;
        Ok(())
    }
    pub fn cancel(&self, document: &mut Document) -> bool {
        if !self.editing() {
            return false;
        }
        document.cancel_edit();
        true
    }
    pub fn overlay(&self, out: &mut OverlaySnapshot) {
        if let Some(reason) = self.edit.as_ref().and_then(|e| e.constrained.as_ref()) {
            out.interaction_label = Some(reason.clone());
            return;
        }
        out.interaction_label =
            self.edit
                .as_ref()
                .and_then(|edit| edit.last)
                .map(|change| match change {
                    ControlChange::Translate(d) | ControlChange::MoveAll(d) => {
                        format!("ΔX {:.2}  ΔY {:.2}", d.x, d.y)
                    }
                    ControlChange::Transform { scale, radians, .. } if radians == 0. => {
                        format!("X {:.1}%  Y {:.1}%", scale[0] * 100., scale[1] * 100.)
                    }
                    ControlChange::Transform { radians, .. } => {
                        format!("{:.1}°", radians.to_degrees())
                    }
                    ControlChange::Rotation { angle, scale, .. } => {
                        format!("Δ {:.1}°  × {:.3}", angle, scale)
                    }
                });
        if let Some(label) = &mut out.interaction_label {
            if self.press.original_points {
                *label = format!("Original points · {label}");
            }
            if let Some(target) = out
                .target_label
                .as_ref()
                .filter(|t| t.starts_with("BlendShape delta"))
            {
                *label = format!("{target} · {label}");
            }
        }
    }
}
pub(super) const HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::Transform),
    InteractionHint::Command(crate::CommandId::CancelGesture),
];
