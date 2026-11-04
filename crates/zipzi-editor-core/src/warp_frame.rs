//! Viewport-local Warp operation frames. These values never enter model history.
use crate::{
    CanvasFrame, ControlKind, DeformerGizmo, Document, Editor, ModelPoint, ObjectRef,
    ViewportBinding, ViewportId, ViewportTransform, WarpTransformTarget,
};
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarpFrame {
    pub center: ModelPoint,
    pub half_size: [f32; 2],
    /// Counterclockwise angle in canvas coordinates.
    pub radians: f32,
}
impl WarpFrame {
    pub(crate) fn transformed(mut self, change: crate::control_edit::ControlChange) -> Self {
        self.center = change.apply_to(self.center);
        if let crate::control_edit::ControlChange::Transform { scale, radians, .. } = change {
            self.half_size[0] *= scale[0];
            self.half_size[1] *= scale[1];
            self.radians = (self.radians + radians + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            if self.radians.abs() < 1e-6 {
                self.radians = 0.;
            }
        }
        self
    }
    pub(crate) fn valid(self) -> bool {
        [
            self.center.x,
            self.center.y,
            self.half_size[0],
            self.half_size[1],
            self.radians,
        ]
        .iter()
        .all(|v| v.is_finite())
            && self.half_size.iter().all(|v| *v >= 0.)
    }
    pub(crate) fn fit(points: &[ModelPoint], radians: f32) -> Option<Self> {
        let (s, c) = radians.sin_cos();
        let b = crate::picking::bounds(points.iter().map(|p| ModelPoint {
            x: p.x * c + p.y * s,
            y: -p.x * s + p.y * c,
        }))?;
        let (x, y) = ((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
        Some(Self {
            center: ModelPoint {
                x: x * c - y * s,
                y: x * s + y * c,
            },
            half_size: [(b[2] - b[0]) / 2., (b[3] - b[1]) / 2.],
            radians,
        })
    }
    pub(crate) fn from_gizmo(g: &DeformerGizmo, t: ViewportTransform) -> Self {
        Self {
            center: t.to_model(g.center),
            half_size: [
                (g.bounds[2] - g.bounds[0]) / 2. / t.scale,
                (g.bounds[3] - g.bounds[1]) / 2. / t.scale,
            ],
            radians: -g.frame_angle,
        }
    }
    pub(crate) fn object_gizmo(self, t: ViewportTransform) -> DeformerGizmo {
        let p = t.to_viewport(self.center);
        let [w, h] = self.half_size.map(|v| v * t.scale);
        DeformerGizmo::object_oriented([p.x - w, p.y - h, p.x + w, p.y + h], -self.radians)
    }
    pub(crate) fn gizmo(self, t: ViewportTransform) -> DeformerGizmo {
        let p = t.to_viewport(self.center);
        let [w, h] = self.half_size.map(|v| v * t.scale);
        DeformerGizmo::warp_oriented([p.x - w, p.y - h, p.x + w, p.y + h], -self.radians)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarpFrameAction {
    ToggleVisible,
    ToggleAdjust,
    Align,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarpFrameInfo {
    pub frame: WarpFrame,
    pub visible: bool,
    pub adjusting: bool,
}

#[derive(Clone)]
pub(crate) struct WarpFrameState {
    pub target: WarpTransformTarget,
    pub selection: Vec<ObjectRef>,
    pub info: WarpFrameInfo,
}

impl crate::tools::ToolController {
    fn warp_frame_target(
        &self,
        document: &Document,
        object: &ObjectRef,
    ) -> Option<WarpTransformTarget> {
        let mut target = self.warp_transform_target(document, object)?;
        // One selected control keeps the owning Warp's whole operation frame;
        // the component itself remains selected for direct point edits.
        if target.components.len() == 1 {
            target.components.clear();
        }
        // A point selection spanning several Warps has one frame. Use the
        // same representative as the canvas gizmo, regardless of which object's
        // inspector/menu initiated the operation.
        if let Some(component) = target.components.first() {
            target.object = component.object.clone();
        }
        Some(target)
    }

    pub(crate) fn warp_frame_state(
        &self,
        document: &Document,
        viewport: ViewportId,
        object: &ObjectRef,
    ) -> Option<&WarpFrameState> {
        let target = self.warp_frame_target(document, object)?;
        let state = self.warp_frames.get(&(viewport, target.object.clone()))?;
        (target == state.target && state.selection == document.context().selections)
            .then_some(state)
    }
    pub(crate) fn prune_warp_frames(&mut self, document: &Document) {
        let obsolete: Vec<_> = self
            .warp_frames
            .keys()
            .filter(|(v, o)| self.warp_frame_state(document, *v, o).is_none())
            .cloned()
            .collect();
        for key in obsolete {
            self.warp_frames.remove(&key);
        }
    }
    pub(crate) fn warp_frame_points(
        &self,
        document: &Document,
        canvas: &CanvasFrame,
        object: &ObjectRef,
    ) -> Option<Vec<ModelPoint>> {
        let target = self.warp_frame_target(document, object)?;
        if target.components.is_empty() {
            Some(canvas.geometry.control(&object.id)?.points.clone())
        } else {
            Some(
                canvas
                    .geometry
                    .warp_selected_points(&target.components)
                    .into_iter()
                    .map(|(_, p)| p)
                    .collect(),
            )
        }
    }
    pub(crate) fn warp_frame_info(
        &self,
        document: &Document,
        canvas: &CanvasFrame,
        object: &ObjectRef,
    ) -> Option<WarpFrameInfo> {
        if let Some(state) =
            self.warp_frame_state(document, canvas.version.binding.viewport, object)
        {
            return Some(state.info);
        }
        Some(WarpFrameInfo {
            frame: WarpFrame::fit(&self.warp_frame_points(document, canvas, object)?, 0.)?,
            visible: true,
            adjusting: false,
        })
    }
    pub(crate) fn set_warp_frame(
        &mut self,
        document: &Document,
        viewport: ViewportId,
        object: ObjectRef,
        info: WarpFrameInfo,
    ) {
        if let Some(target) = self.warp_frame_target(document, &object) {
            self.warp_frames.insert(
                (viewport, target.object.clone()),
                WarpFrameState {
                    target,
                    selection: document.context().selections.clone(),
                    info,
                },
            );
        }
    }
    pub(crate) fn change_warp_frame(
        &mut self,
        document: &Document,
        canvas: &CanvasFrame,
        object: ObjectRef,
        action: WarpFrameAction,
    ) -> Result<()> {
        let control = canvas
            .geometry
            .control(&object.id)
            .filter(|c| c.kind == ControlKind::Warp && c.visible && c.enabled)
            .ok_or_else(|| anyhow::anyhow!("Select a visible Warp"))?;
        ensure!(
            document.context().selections.contains(&object)
                && canvas.geometry.in_isolation_scope(&object),
            "Warp is outside the current selection"
        );
        let mut info = self
            .warp_frame_info(document, canvas, &object)
            .ok_or_else(|| anyhow::anyhow!("Warp frame is unavailable"))?;
        match action {
            WarpFrameAction::ToggleVisible => {
                info.visible = !info.visible;
                if !info.visible {
                    info.adjusting = false;
                }
            }
            WarpFrameAction::ToggleAdjust => {
                info.visible = true;
                info.adjusting = !info.adjusting;
            }
            WarpFrameAction::Align | WarpFrameAction::Reset => {
                let radians = if action == WarpFrameAction::Reset {
                    0.
                } else {
                    let (_, columns, _) = canvas
                        .geometry
                        .topology
                        .controls
                        .get(&object.id)
                        .and_then(|t| t.warp_shape)
                        .ok_or_else(|| anyhow::anyhow!("Warp topology is unavailable"))?;
                    let mut dx = 0f64;
                    let mut dy = 0f64;
                    let mut length = 0f64;
                    for row in control.points.chunks(columns as usize + 1) {
                        for edge in row.windows(2) {
                            let x = f64::from(edge[1].x - edge[0].x);
                            let y = f64::from(edge[1].y - edge[0].y);
                            dx += x;
                            dy += y;
                            length += x.hypot(y);
                        }
                    }
                    ensure!(
                        length > 1e-6 && dx.hypot(dy) > length * 1e-5,
                        "Warp has no stable horizontal direction"
                    );
                    dy.atan2(dx) as f32
                };
                info.frame = WarpFrame::fit(
                    &self.warp_frame_points(document, canvas, &object).unwrap(),
                    radians,
                )
                .ok_or_else(|| anyhow::anyhow!("Warp frame is unavailable"))?;
            }
        }
        self.set_warp_frame(document, canvas.version.binding.viewport, object, info);
        Ok(())
    }
}

impl Editor {
    pub fn warp_frame_info(
        &self,
        binding: ViewportBinding,
        object: &ObjectRef,
    ) -> Option<WarpFrameInfo> {
        let canvas = self.displayed_modeling_frame(binding)?;
        let session = self.active_session()?;
        session
            .modeling
            .tools
            .warp_frame_info(&session.document, canvas, object)
    }
    pub fn change_warp_frame(
        &mut self,
        binding: ViewportBinding,
        object: ObjectRef,
        action: WarpFrameAction,
    ) -> Result<()> {
        self.sync_runtime()?;
        let canvas = self
            .displayed_modeling_frame(binding)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Wait for the current Warp frame"))?;
        ensure!(
            canvas.version.scene == self.active_session().unwrap().scene_revision,
            "Wait for the current Warp frame"
        );
        self.cancel_gesture();
        let session = self.active_session_mut().unwrap();
        ensure!(
            canvas.version.scene == session.scene_revision,
            "Wait for the current Warp frame"
        );
        session
            .modeling
            .tools
            .change_warp_frame(&session.document, &canvas, object, action)
    }
}
