//! Control-point and gizmo gestures: candidate ownership, transforms and feedback.
use super::{OverlaySnapshot, gesture::Press, hints::*};
use crate::ComponentRef;
use crate::control_edit::{ControlChange, ControlEdit, ControlScope};
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
    axis: Option<Axis>,
    rotation: Option<RotationDrag>,
}
struct RotationDrag {
    previous: f64,
    radians: f64,
}
impl RotationDrag {
    fn new(center: crate::ModelPoint, point: crate::ModelPoint) -> Self {
        Self {
            previous: f64::from(point.y - center.y).atan2(f64::from(point.x - center.x)),
            radians: 0.,
        }
    }
    fn update(&mut self, center: crate::ModelPoint, point: crate::ModelPoint) -> f32 {
        let (x, y) = (f64::from(point.x - center.x), f64::from(point.y - center.y));
        if x.hypot(y) > 1e-6 {
            let angle = y.atan2(x);
            let delta = angle - self.previous;
            self.radians += delta.sin().atan2(delta.cos());
            self.previous = angle;
        }
        self.radians as f32
    }
}
#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
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
        self.edit.is_some() && !self.press.adjust_frame
    }
    pub(super) fn original_frame(&self) -> Option<(&crate::ObjectRef, crate::WarpFrameInfo)> {
        let HitTarget::DeformerHandle(object, _) = &self.target else {
            return None;
        };
        let (_, gizmo) = self
            .press
            .warp_selection
            .as_ref()?
            .items
            .iter()
            .find(|(o, _)| o == object)?;
        Some((
            object,
            crate::WarpFrameInfo {
                frame: crate::WarpFrame::from_gizmo(gizmo, self.press.transform),
                visible: true,
                adjusting: self.press.adjust_frame,
            },
        ))
    }
    pub(super) fn frame_preview(&self) -> Option<(&crate::ObjectRef, crate::WarpFrameInfo)> {
        let (object, mut info) = self.original_frame()?;
        let change = self.edit.as_ref()?.last?;
        info.frame = info.frame.transformed(change);
        Some((object, info))
    }
    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        let origin = self.press.version;
        self.editing()
            && version.binding.document == origin.binding.document
            && version.scene > origin.scene
            && (version.binding.viewport != origin.binding.viewport
                || (version.viewport == origin.viewport && version.request > origin.request))
    }
    pub fn update(
        &mut self,
        document: &mut Document,
        _components: &[ComponentRef],
    ) -> Result<bool> {
        let point = self.press.current;
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid pointer position"
        );
        let press = &self.press;
        let components = &press.components;
        let target = &self.target;
        let local_frame = matches!(target, HitTarget::DeformerHandle(object, _)
            if press.warp_selection.as_ref().is_some_and(|s| s.local && s.items.iter().any(|(o,_)|o==object)));
        if self.edit.is_none() && crate::picking::distance2(press.point, point) < 9. {
            return Ok(false);
        }
        if self.edit.is_none() {
            let edits = if press.adjust_frame {
                Vec::new()
            } else {
                document.ensure_editable(&document.context().selections)?;
                let generic = press.warp_selection.as_ref().filter(|s| {
                    !s.objects.is_empty() && s.items.iter().any(|(o, _)| o == target.object())
                });
                let mut targets = if matches!(target, HitTarget::DeformerHandle(..)) {
                    generic
                        .map(|s| s.objects.clone())
                        .unwrap_or_else(|| vec![target.object().clone()])
                } else {
                    vec![target.object().clone()]
                };
                if local_frame {
                    targets.retain(|o| components.iter().any(|c| c.object == *o));
                }
                if matches!(target, HitTarget::Component(_)) || local_frame {
                    for c in components {
                        if press.scene.in_isolation_scope(&c.object) && !targets.contains(&c.object)
                        {
                            targets.push(c.object.clone());
                        }
                    }
                }
                let mut edits = ControlEdit::prepare_targets(
                    document,
                    &press.scene,
                    &targets,
                    press.original_points,
                )?;
                ensure!(
                    !press.keep_children
                        || !edits.iter().any(|edit| edit.warp.is_some())
                        || edits.iter().all(|edit| edit.warp.is_some()),
                    "Keep children requires a selection of Warp controls"
                );
                for target in &mut edits {
                    if press.keep_children && target.warp.is_some() {
                        target.keep_warp_children(document)?;
                    }
                }
                document.begin_control_edit(&edits)?;
                edits
            };
            self.edit = Some(ActiveEdit {
                edits,
                last: None,
                constrained: None,
                rotation: match target {
                    HitTarget::RotationHandle(object) => Some(RotationDrag::new(
                        press.scene.control(&object.id).unwrap().points[0],
                        press.transform.to_model(press.point),
                    )),
                    HitTarget::DeformerHandle(object, crate::DeformerHandle::Rotate) => {
                        let gizmo = press
                            .scene
                            .object_gizmo(object, press.transform, press.warp_selection.as_ref())
                            .unwrap();
                        Some(RotationDrag::new(
                            press.transform.to_model(gizmo.center),
                            press.transform.to_model(press.point),
                        ))
                    }
                    _ => None,
                },
                axis: (press.shift
                    && (matches!(target, HitTarget::Component(_))
                        || matches!(
                            target,
                            HitTarget::DeformerHandle(_, crate::DeformerHandle::Move)
                        )))
                .then(|| {
                    if (point.x - press.point.x).abs() >= (point.y - press.point.y).abs() {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    }
                }),
            });
        }
        let edit = self.edit.as_mut().unwrap();
        let edits = &edit.edits;
        let start = press.transform.to_model(press.point);
        let constrained_point = match edit.axis {
            Some(Axis::Horizontal) => crate::ViewportPoint {
                x: point.x,
                y: press.point.y,
            },
            Some(Axis::Vertical) => crate::ViewportPoint {
                x: press.point.x,
                y: point.y,
            },
            None => point,
        };
        let end = press.transform.to_model(constrained_point);
        let mut delta = crate::ModelVector {
            x: end.x - start.x,
            y: end.y - start.y,
        };
        if press.shift && press.alt {
            delta.x = delta.x.round();
            delta.y = delta.y.round();
        }
        let pointer_angle = |radians: f32| {
            let radians = if press.shift {
                let step = if press.alt { 1f32 } else { 15f32 }.to_radians();
                (radians / step).round() * step
            } else {
                radians
            };
            edits[0].rotation_pointer_angle(radians)
        };
        let change = match target {
            HitTarget::RotationHandle(_) => ControlChange::Rotation {
                translation: crate::ModelVector::default(),
                angle: pointer_angle(edit.rotation.as_mut().unwrap().update(
                    press.scene.control(&target.object().id).unwrap().points[0],
                    end,
                ))?,
                scale: 1.,
            },
            HitTarget::DeformerHandle(object, handle) => {
                use crate::DeformerHandle as H;
                let gizmo = if *handle == H::RotationScale {
                    press
                        .scene
                        .deformer_gizmo(press.scene.control(&object.id).unwrap(), press.transform)
                        .unwrap()
                } else {
                    press
                        .scene
                        .object_gizmo(object, press.transform, press.warp_selection.as_ref())
                        .unwrap()
                };
                let scope = if local_frame {
                    ControlScope::Selected
                } else {
                    ControlScope::Whole
                };
                match handle {
                    H::Move if local_frame => ControlChange::Transform {
                        scope,
                        pivot: press.transform.to_model(gizmo.center),
                        scale: [1.; 2],
                        radians: 0.,
                        axes: -gizmo.frame_angle,
                        translation: delta,
                    },
                    H::Move => ControlChange::MoveAll(delta),
                    H::Rotate
                        if edits.len() == 1
                            && edits[0].rotation
                            && press
                                .scene
                                .control(&object.id)
                                .is_some_and(|c| c.kind == crate::ControlKind::Rotation) =>
                    {
                        ControlChange::Rotation {
                            translation: crate::ModelVector::default(),
                            angle: pointer_angle(
                                edit.rotation
                                    .as_mut()
                                    .unwrap()
                                    .update(press.transform.to_model(gizmo.center), end),
                            )?,
                            scale: 1.,
                        }
                    }
                    H::Rotate if edit.rotation.is_some() => ControlChange::Transform {
                        scope,
                        pivot: press.transform.to_model(gizmo.center),
                        scale: [1.; 2],
                        axes: -gizmo.frame_angle,
                        radians: edit
                            .rotation
                            .as_mut()
                            .unwrap()
                            .update(press.transform.to_model(gizmo.center), end),
                        translation: crate::ModelVector::default(),
                    },
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
                        let (_, opposite) = gizmo.scale_anchor(*index);
                        let anchor = if press.alt { gizmo.center } else { opposite };
                        let ratio = |d: f32, span: f32| {
                            if span.abs() < 1e-5 {
                                1.
                            } else {
                                (1. + d / span).max(0.001)
                            }
                        };
                        let (sin, cos) = gizmo.frame_angle.sin_cos();
                        let (dx, dy) = (point.x - press.point.x, point.y - press.point.y);
                        let [span_x, span_y] = gizmo.scale_span(*index, press.alt);
                        let sx = ratio(dx * cos + dy * sin, span_x);
                        let sy = ratio(-dx * sin + dy * cos, span_y);
                        let scale = if (press.shift && index % 2 == 0)
                            || edits.iter().any(|e| e.rotation)
                        {
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
                            scope,
                            translation: crate::ModelVector::default(),
                            pivot: press.transform.to_model(anchor),
                            scale,
                            radians: 0.,
                            axes: -gizmo.frame_angle,
                        }
                    }
                }
            }
            _ => ControlChange::Translate(delta),
        };
        let change = if press.shift {
            let angle_step: f32 = if press.alt { 1. } else { 15. };
            match change {
                ControlChange::Transform {
                    scope,
                    pivot,
                    scale,
                    radians,
                    translation,
                    axes,
                } => ControlChange::Transform {
                    scope,
                    pivot,
                    scale: if press.alt {
                        scale.map(snap_scale)
                    } else {
                        scale
                    },
                    radians: (radians / angle_step.to_radians()).round() * angle_step.to_radians(),
                    translation,
                    axes,
                },
                ControlChange::Rotation {
                    translation,
                    angle,
                    scale,
                } => ControlChange::Rotation {
                    translation,
                    angle: (angle / angle_step).round() * angle_step,
                    scale: if press.alt { snap_scale(scale) } else { scale },
                },
                other => other,
            }
        } else {
            change
        };
        if edit.last == Some(change) {
            edit.constrained = None;
            return Ok(false);
        }
        if press.adjust_frame {
            let gizmo = press
                .warp_selection
                .as_ref()
                .and_then(|g| g.items.iter().find(|(o, _)| o == self.target.object()))
                .map(|(_, g)| g)
                .unwrap();
            ensure!(
                crate::WarpFrame::from_gizmo(gizmo, press.transform)
                    .transformed(change)
                    .valid(),
                "Warp frame exceeds the finite canvas range"
            );
            edit.last = Some(change);
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
    pub fn finish(
        &mut self,
        document: &mut Document,
        components: &mut Vec<ComponentRef>,
    ) -> Result<()> {
        if self.editing() {
            document.finish_edit()?;
        } else if self.edit.is_none()
            && let Some(component) = &self.press.deferred_toggle
        {
            components.retain(|c| c != component);
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
                    ControlChange::Transform {
                        scale,
                        radians,
                        translation,
                        ..
                    } if radians == 0.
                        && scale == [1.; 2]
                        && translation != crate::ModelVector::default() =>
                    {
                        format!("ΔX {:.2}  ΔY {:.2}", translation.x, translation.y)
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
/// Ratios are relative to the frozen press shape, in percentage-point steps.
fn snap_scale(scale: f32) -> f32 {
    (1. + ((scale - 1.) * 100.).round() / 100.).max(0.01)
}
pub(super) const HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::Transform),
    InteractionHint::Command(crate::CommandId::CancelGesture),
];
