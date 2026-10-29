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
pub use hints::{HintAction, InteractionHint};
pub use manual_mesh::{ManualMeshEdge, ManualMeshFeedback, ManualTool};

use crate::{
    CanvasScene, ComponentRef, Document, HitTarget, ObjectKind, ObjectRef, ViewportPoint,
    ViewportTransform,
};
use gesture::Gesture;
use std::sync::Arc;

pub use overlay::{
    GluePointState, OverlayDeformer, OverlayGlue, OverlayGluePoint, OverlayHandle, OverlaySnapshot,
    OverlayVertex, OverlayVertexKind,
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
    pub warp_fine: bool,
    pub original_points: bool,
    components: Vec<ComponentRef>,
    hovered: Option<HitTarget>,
    hovered_binding: Option<crate::ViewportBinding>,
    gesture: Option<Gesture>,
    manual: Option<manual_mesh::ManualMeshMode>,
    access_cache: std::sync::Mutex<Option<access::AccessCache>>,
    #[cfg(test)]
    access_builds: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    overlay_builds: std::sync::atomic::AtomicUsize,
}
impl ToolController {
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
        self.gesture
            .as_ref()
            .map_or(controls::HINTS, Gesture::hints)
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
    }
    pub fn cancel(&mut self, document: &mut Document) -> bool {
        let changed = self.gesture.take().is_some_and(|g| g.cancel(document))
            | self
                .manual
                .as_mut()
                .is_some_and(|mode| mode.cancel(document));
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
        self.manual
            .as_ref()
            .is_some_and(|mode| mode.blocked.is_none())
    }
    pub fn manual_tool(&self) -> Option<ManualTool> {
        self.manual.as_ref().map(|mode| mode.tool)
    }
    pub fn manual_target(&self) -> Option<&crate::ManualMeshTarget> {
        self.manual.as_ref().map(|mode| &mode.target)
    }
    pub fn manual_edge_at(
        &self,
        frame: &CanvasFrame,
        point: crate::ViewportPoint,
    ) -> Option<crate::ManualMeshEdge> {
        self.manual.as_ref()?.edge_at(frame, point)
    }
    pub(crate) fn manual_viewport(&self) -> Option<crate::ViewportId> {
        self.manual.as_ref().and_then(|m| m.uv_viewport)
    }
    pub(crate) fn manual_uv_plane(&self, viewport: crate::ViewportId) -> bool {
        self.manual
            .as_ref()
            .is_some_and(|m| !m.preview_result && m.uv_viewport == Some(viewport))
    }
    pub(crate) fn bind_manual_viewport(&mut self, viewport: crate::ViewportId) {
        if let Some(m) = &mut self.manual {
            m.uv_viewport.get_or_insert(viewport);
        }
    }
    pub fn manual_preview_result(&self) -> bool {
        self.manual.as_ref().is_some_and(|mode| mode.preview_result)
    }
    pub fn toggle_manual_preview_result(&mut self) {
        if let Some(mode) = &mut self.manual {
            mode.preview_result = !mode.preview_result;
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
    pub fn enter_manual(&mut self, document: &mut Document, target: crate::ManualMeshTarget) {
        self.cancel(document);
        self.components.clear();
        self.active = ToolId::Select;
        self.manual = Some(manual_mesh::ManualMeshMode::new(target, document));
    }
    pub fn set_manual_tool(&mut self, tool: ManualTool, document: &mut Document) {
        self.cancel(document);
        if let Some(mode) = &mut self.manual {
            self.active = ToolId::Select;
            mode.tool = tool;
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
    Move {
        point: ViewportPoint,
        left_down: bool,
    },
    Up {
        point: ViewportPoint,
    },
    Cancel,
    Leave,
}

/// Guidance for an active viewport navigation capture.
pub fn navigation_hints() -> &'static [InteractionHint] {
    hand::CAPTURE_HINTS
}
