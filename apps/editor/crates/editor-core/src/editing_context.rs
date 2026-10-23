//! Authoring contexts are document state; workspace layouts only choose which to activate.
use crate::{AnimationSession, ToolController, WorkspaceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditingContext {
    Modeling,
    Animation,
}
impl From<WorkspaceKind> for EditingContext {
    fn from(workspace: WorkspaceKind) -> Self {
        match workspace {
            WorkspaceKind::Modeling => Self::Modeling,
            WorkspaceKind::Animation => Self::Animation,
        }
    }
}

#[derive(Default)]
pub(crate) struct ModelingContext {
    pub(crate) tools: ToolController,
}
pub(crate) struct AnimationContext {
    pub(crate) playback: AnimationSession,
    pub(crate) timeline: crate::timeline::TimelineSession,
}
