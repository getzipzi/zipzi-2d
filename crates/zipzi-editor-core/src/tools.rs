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
mod overlay;
mod region;
mod select;
pub use hints::{HintAction, InteractionHint};

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
        self.gesture.is_some()
    }
    /// Completed poses from this control edit can lag subsequent pointer moves.
    /// The press fixes the authoring target; new gestures still require a current frame.
    pub(crate) fn accepts_drag_frame(&self, version: FrameVersion) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.accepts_frame(version))
    }

    pub fn clear(&mut self) {
        self.components.clear();
        self.hovered = None;
        self.hovered_binding = None;
    }
    pub fn reconcile(&mut self, document: &Document) {
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
        let changed = self.gesture.take().is_some_and(|g| g.cancel(document));
        self.hovered = None;
        self.hovered_binding = None;
        changed
    }
    pub fn set_tool(&mut self, tool: ToolId, document: &mut Document) -> bool {
        let changed = self.cancel(document);
        let preserve_components = self.active.preserves_components(tool);
        self.active = tool;
        if !preserve_components {
            self.clear();
        }
        changed
    }
}
#[derive(Clone, Copy, Debug)]
pub enum CanvasInput {
    Down {
        point: ViewportPoint,
        shift: bool,
        alt: bool,
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
