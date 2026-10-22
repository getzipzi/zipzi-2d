//! Canvas interaction state machine. Object selection, component selection and
//! write targets are independent; a click never opens a model transaction.
mod drag;
mod input;
mod overlay;
mod selection;

use crate::control_edit::{ControlChange, ControlEdit};
use crate::{
    CanvasScene, ComponentRef, Document, HitTarget, ObjectKind, ObjectRef, ViewportPoint,
    ViewportTransform,
};
use std::sync::Arc;

pub use overlay::{
    OverlayDeformer, OverlayGlue, OverlayHandle, OverlaySnapshot, OverlayVertex, OverlayVertexKind,
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
struct Press {
    version: FrameVersion,
    point: ViewportPoint,
    transform: ViewportTransform,
    scene: Arc<CanvasScene>,
    target: Option<HitTarget>,
    component_surface: bool,
    shift: bool,
    lasso: Option<selection::LassoGesture>,
    warp_fine: bool,
    selection: Vec<ObjectRef>,
    current: ViewportPoint,
    dragged: bool,
    edit: Option<Vec<ControlEdit>>,
    last: Option<ControlChange>,
}
#[derive(Default)]
pub struct ToolController {
    pub active: ToolId,
    pub deformer_visibility: DeformerVisibility,
    pub deformer_grid: bool,
    pub warp_fine: bool,
    components: Vec<ComponentRef>,
    hovered: Option<HitTarget>,
    hovered_binding: Option<crate::ViewportBinding>,
    press: Option<Press>,
}
impl ToolController {
    pub(crate) fn interaction_viewport(&self) -> Option<crate::ViewportId> {
        self.press
            .as_ref()
            .map(|press| press.version.binding.viewport)
    }
    pub(crate) fn lasso_active(&self) -> bool {
        self.press
            .as_ref()
            .is_some_and(|press| press.lasso.is_some())
    }
    pub fn dragging(&self) -> bool {
        self.press.is_some()
    }
    /// Completed poses from this control edit can lag subsequent pointer moves.
    /// The press fixes the authoring target; new gestures still require a current frame.
    pub(crate) fn accepts_drag_frame(&self, version: FrameVersion) -> bool {
        self.press.as_ref().is_some_and(|press| {
            press.edit.is_some()
                && version.binding.document == press.version.binding.document
                && version.scene > press.version.scene
                && (version.binding.viewport != press.version.binding.viewport
                    || (version.viewport == press.version.viewport
                        && version.request > press.version.request))
        })
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
        let changed = self.press.take().is_some_and(|p| p.edit.is_some());
        if changed {
            document.cancel_edit();
        }
        self.hovered = None;
        self.hovered_binding = None;
        changed
    }
    pub fn set_tool(&mut self, tool: ToolId, document: &mut Document) -> bool {
        let changed = self.cancel(document);
        let preserve_components = matches!(self.active, ToolId::Select | ToolId::Lasso)
            && matches!(tool, ToolId::Select | ToolId::Lasso);
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
