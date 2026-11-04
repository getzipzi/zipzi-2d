//! Canvas interaction state machine. Object selection, component selection and
//! write targets are independent; a click never opens a model transaction.
mod access;
mod controls;
mod gesture;
mod glue;
mod hand;
mod hints;
mod input;
mod lasso;
mod manual_mesh;
mod overlay;
mod region;
mod select;
mod transform_constraints;
pub use hints::{HintAction, InteractionHint};
pub use manual_mesh::{ManualMeshEdge, ManualMeshFeedback, ManualTool};

use crate::{
    CanvasScene, ComponentRef, Document, HitTarget, ObjectKind, ObjectRef, ViewportPoint,
    ViewportTransform,
};
use gesture::Gesture;
use std::sync::Arc;

pub use overlay::{
    GluePointState, OverlayDeformer, OverlayGlue, OverlayGluePoint, OverlayHandle,
    OverlayRotationBone, OverlaySnapshot, OverlayVertex, OverlayVertexKind,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolId {
    #[default]
    Select,
    Lasso,
    Glue,
    Hand,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeformerVisibility {
    pub ancestors: bool,
    pub all: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameVersion {
    pub binding: crate::ViewportBinding,
    pub scene: u64,
    pub viewport: u64,
    pub request: u64,
}
#[derive(Clone, Debug)]
pub struct CanvasFrame {
    pub version: FrameVersion,
    pub transform: ViewportTransform,
    pub geometry: Arc<CanvasScene>,
}
#[derive(Default)]
pub struct ToolController {
    pub active: ToolId,
    pub deformer_visibility: DeformerVisibility,
    pub deformer_grid: bool,
    pub warp_whole: bool,
    pub original_points: bool,
    pub(crate) components: Vec<ComponentRef>,
    pub(crate) warp_frames: std::collections::HashMap<
        (crate::ViewportId, ObjectRef),
        crate::warp_frame::WarpFrameState,
    >,
    hovered: Option<HitTarget>,
    hovered_binding: Option<crate::ViewportBinding>,
    alt: bool,
    gesture: Option<Gesture>,
    manual: Option<manual_mesh::ManualMeshSession>,
    access_cache: std::sync::Mutex<Option<access::AccessCache>>,
    #[cfg(test)]
    access_builds: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    overlay_builds: std::sync::atomic::AtomicUsize,
}
impl ToolController {
    fn rotation_length_target(&self) -> Option<&crate::ObjectRef> {
        match &self.gesture {
            Some(Gesture::Controls(g)) if g.adjusting_bone_length() => Some(g.target.object()),
            Some(_) => None,
            None if self.alt => self.hovered.as_ref().and_then(|hit| match hit {
                HitTarget::DeformerHandle(object, crate::DeformerHandle::RotationScale) => {
                    Some(object)
                }
                _ => None,
            }),
            None => None,
        }
    }
    pub(crate) fn set_gesture_modifiers(&mut self, shift: bool, alt: bool) -> bool {
        let changed = self.alt != alt;
        self.alt = alt;
        self.gesture
            .as_mut()
            .is_some_and(|g| g.set_modifiers(shift, alt))
            | changed
    }
    pub(crate) fn warp_selection_gizmo(
        &self,
        document: &Document,
        frame: &CanvasFrame,
    ) -> Option<crate::deformer::WarpGizmos> {
        if let Some(Gesture::Controls(gesture)) = &self.gesture {
            if let Some((object, info)) = gesture.frame_preview() {
                if gesture.press.version.binding.viewport == frame.version.binding.viewport {
                    let mut gizmos = gesture.press.warp_selection.clone().unwrap_or_default();
                    if let Some((_, gizmo)) = gizmos.items.iter_mut().find(|(o, _)| o == object) {
                        *gizmo = info.frame.gizmo(frame.transform);
                    }
                    return Some(gizmos);
                }
            }
        }
        if let Some(frame) = self.object_selection_frame(document, frame) {
            return Some(frame);
        }
        let components: Vec<_> = self
            .components
            .iter()
            .filter(|c| {
                document.context().selections.contains(&c.object)
                    && frame.geometry.in_isolation_scope(&c.object)
                    && frame.geometry.control(&c.object.id).is_some_and(|control| {
                        control.kind == crate::ControlKind::Warp
                            && frame.geometry.control_visible(
                                control,
                                &document.context().selections,
                                self.active,
                                self.deformer_visibility,
                            )
                    })
            })
            .cloned()
            .collect();
        let selection = (!self.warp_whole)
            .then(|| {
                frame
                    .geometry
                    .warp_selection_gizmo(&components, frame.transform)
            })
            .flatten();
        let local = selection.is_some();
        let items = if let Some(selection) = selection {
            vec![(selection.object, selection.gizmo)]
        } else {
            frame
                .geometry
                .controls
                .iter()
                .filter(|c| {
                    c.kind == crate::ControlKind::Warp
                        && document.context().selections.contains(&c.object)
                        && frame.geometry.control_visible(
                            c,
                            &document.context().selections,
                            self.active,
                            self.deformer_visibility,
                        )
                })
                .filter_map(|c| {
                    frame
                        .geometry
                        .deformer_gizmo(c, frame.transform)
                        .map(|g| (c.object.clone(), g))
                })
                .collect()
        };
        Some(crate::deformer::WarpGizmos {
            objects: vec![],
            local,
            adjusting: document
                .context()
                .selections
                .iter()
                .filter(|object| {
                    self.warp_frame_info(document, frame, object)
                        .is_some_and(|info| info.adjusting)
                })
                .cloned()
                .collect(),
            items: items
                .into_iter()
                .filter_map(|(object, gizmo)| {
                    if let Some(state) =
                        self.warp_frame_state(document, frame.version.binding.viewport, &object)
                    {
                        state
                            .info
                            .visible
                            .then(|| (object, state.info.frame.gizmo(frame.transform)))
                    } else {
                        Some((object, gizmo))
                    }
                })
                .collect(),
        })
    }
    pub(crate) fn hovered_binding(&self) -> Option<crate::ViewportBinding> {
        self.hovered_binding
    }
    pub(crate) fn interaction_viewport(&self) -> Option<crate::ViewportId> {
        if let Some(manual) = &self.manual {
            return manual.viewport();
        }
        self.gesture
            .as_ref()
            .map(|press| press.press().version.binding.viewport)
    }
    pub(crate) fn interaction_hints(&self) -> &'static [InteractionHint] {
        if self.gesture.is_none()
            && matches!(
                self.hovered,
                Some(HitTarget::DeformerHandle(
                    _,
                    crate::DeformerHandle::RotationScale
                ))
            )
        {
            return controls::BONE_TIP_HINTS;
        }
        self.gesture
            .as_ref()
            .map_or(controls::HINTS, Gesture::hints)
    }
    pub(crate) fn idle_hints(&self) -> &'static [InteractionHint] {
        if matches!(
            self.hovered,
            Some(HitTarget::DeformerHandle(
                _,
                crate::DeformerHandle::RotationScale
            ))
        ) {
            controls::BONE_TIP_HINTS
        } else {
            self.active.idle_hints()
        }
    }
    pub fn dragging(&self) -> bool {
        self.gesture.is_some() || self.manual.as_ref().is_some_and(|mode| mode.dragging())
    }
    /// Completed poses from this control edit can lag subsequent pointer moves.
    /// The press fixes the authoring target; new gestures still require a current frame.
    pub(crate) fn accepts_drag_frame(&self, version: FrameVersion) -> bool {
        if let Some(manual) = &self.manual {
            return manual.accepts_frame(version);
        }
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.accepts_frame(version))
    }

    /// Clear ordinary tool projections. Document modes have an explicit lifecycle.
    pub fn clear_selection(&mut self) {
        self.components.clear();
        self.warp_frames.clear();
        self.hovered = None;
        self.hovered_binding = None;
    }
    pub fn reconcile(&mut self, document: &mut Document) {
        if let Some(mode) = &mut self.manual {
            mode.reconcile(document);
        }
        let locks = document.object_state_index();
        let model = document.model();
        self.components.retain(|c| {
            !locks.is_locked(&c.object)
                && document.context().selections.contains(&c.object)
                && match c.object.kind {
                    ObjectKind::Mesh => model.get_mesh(&c.object.id).is_some(),
                    ObjectKind::Deformer => {
                        model.get_transform(&c.object.id).is_some()
                            && document.deformer_controls_visible(&c.object.id)
                    }
                    _ => false,
                }
        });
        self.hovered = None;
        self.hovered_binding = None;
        self.prune_warp_frames(document);
    }
    pub fn cancel(&mut self, document: &mut Document) -> bool {
        let gesture = self.gesture.take();
        let original = gesture.as_ref().and_then(|g| {
            if let Gesture::Controls(g) = g {
                g.original_frame()
                    .map(|(o, i)| (g.press.version.binding.viewport, o.clone(), i))
            } else {
                None
            }
        });
        let changed = gesture.is_some_and(|g| g.cancel(document))
            | self
                .manual
                .as_mut()
                .is_some_and(|mode| mode.cancel(document));
        if let Some((viewport, object, info)) = original {
            self.set_warp_frame(document, viewport, object, info);
        }
        self.hovered = None;
        self.hovered_binding = None;
        changed
    }
    pub fn set_tool(&mut self, tool: ToolId, document: &mut Document) -> bool {
        let changed = self.cancel(document);
        let preserve_components = self.active.preserves_components(tool);
        self.active = tool;
        if !preserve_components {
            self.clear_selection();
        }
        changed
    }
    pub fn manual_active(&self) -> bool {
        self.manual.is_some()
    }
    pub fn manual_editable(&self) -> bool {
        self.manual.as_ref().is_some_and(|mode| mode.editable())
    }
    pub fn manual_tool(&self) -> Option<ManualTool> {
        self.manual.as_ref().map(|mode| mode.tool)
    }
    pub fn manual_target(&self) -> Option<&crate::ManualMeshTarget> {
        self.manual.as_ref().map(|mode| &mode.target)
    }
    pub fn manual_targets(&self) -> Vec<crate::ManualMeshTarget> {
        self.manual
            .as_ref()
            .map_or_else(Vec::new, |mode| mode.targets())
    }
    pub fn manual_target_at(
        &self,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Option<&crate::ManualMeshTarget> {
        self.manual
            .as_ref()
            .map(|mode| mode.target_at(frame, point))
    }
    pub fn activate_manual_target(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> bool {
        self.manual
            .as_mut()
            .is_some_and(|mode| mode.activate_at(document, frame, point))
    }
    pub fn manual_edge_at(
        &self,
        frame: &CanvasFrame,
        point: crate::ViewportPoint,
    ) -> Option<crate::ManualMeshEdge> {
        self.manual.as_ref()?.edge_at(frame, point)
    }
    pub(crate) fn manual_viewport(&self) -> Option<crate::ViewportId> {
        self.manual.as_ref().and_then(|m| m.edit_viewport)
    }
    pub(crate) fn manual_uv_plane(&self, viewport: crate::ViewportId) -> bool {
        self.manual
            .as_ref()
            .is_some_and(|m| m.source_view && m.edit_viewport == Some(viewport))
    }
    pub(crate) fn bind_manual_viewport(&mut self, viewport: crate::ViewportId) {
        if let Some(m) = &mut self.manual {
            m.bind_viewport(viewport);
        }
    }
    pub fn manual_source_view(&self) -> bool {
        self.manual.as_ref().is_some_and(|mode| mode.source_view)
    }
    pub fn toggle_manual_source_view(&mut self) {
        if let Some(mode) = &mut self.manual {
            mode.source_view = !mode.source_view;
        }
    }
    pub fn refresh_manual_source(&mut self, document: &Document) -> anyhow::Result<()> {
        if let Some(mode) = &mut self.manual {
            mode.refresh_source(document)?;
        }
        Ok(())
    }
    pub fn after_manual_generation(&mut self, document: &Document) -> anyhow::Result<()> {
        if let Some(mode) = &mut self.manual {
            mode.refresh_source(document)?;
            mode.selected.clear();
            mode.selected_edge = None;
        }
        Ok(())
    }
    pub fn enter_manual(&mut self, document: &mut Document, targets: Vec<crate::ManualMeshTarget>) {
        self.cancel(document);
        self.components.clear();
        self.active = ToolId::Select;
        self.manual = Some(manual_mesh::ManualMeshSession::new(targets, document));
    }
    pub fn set_manual_tool(&mut self, tool: ManualTool, document: &mut Document) {
        self.cancel(document);
        if let Some(mode) = &mut self.manual {
            self.active = ToolId::Select;
            mode.set_tool(tool);
        }
    }
    pub fn exit_manual(&mut self, document: &mut Document) {
        self.cancel(document);
        self.manual = None;
    }
    pub fn settle_manual(&mut self, document: &mut Document) -> anyhow::Result<bool> {
        self.manual
            .as_mut()
            .map_or(Ok(false), |mode| mode.settle(document))
    }
    pub fn manual_select_all(&mut self, document: &Document) -> bool {
        if let Some(mode) = &mut self.manual {
            mode.select_all(document);
            return true;
        }
        false
    }
    pub fn manual_delete_selection(&mut self, document: &mut Document) -> anyhow::Result<bool> {
        self.manual
            .as_mut()
            .map_or(Ok(false), |mode| mode.delete_selection(document))
    }
}
#[derive(Clone, Copy, Debug)]
pub enum CanvasInput {
    Down {
        point: ViewportPoint,
        shift: bool,
        alt: bool,
        /// Platform primary modifier (Command on macOS), captured on press.
        primary: bool,
    },
    /// Move with the most recent transform modifiers. Native pointer callers
    /// supply current keys through `update_canvas_gesture_with_modifiers`.
    Move {
        point: ViewportPoint,
        left_down: bool,
    },
    Up {
        point: ViewportPoint,
    },
    /// Transform constraints change immediately, even with a stationary pointer.
    ModifiersChanged {
        shift: bool,
        alt: bool,
    },
    Cancel,
    Leave,
}

/// Guidance for an active viewport navigation capture.
pub fn navigation_hints() -> &'static [InteractionHint] {
    hand::CAPTURE_HINTS
}
