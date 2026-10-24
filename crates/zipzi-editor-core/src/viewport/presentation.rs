//! Runtime presentation ownership. Document navigation is stored separately.
use crate::{AnimationIdentity, CanvasFrame, ViewportBinding, ViewportMetrics};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationViewportIdentity {
    pub binding: ViewportBinding,
    pub content: AnimationIdentity,
    pub viewport_revision: u64,
    pub metrics: ViewportMetrics,
    pub request: u64,
}

#[derive(Default)]
pub(crate) struct ViewportPresentation {
    pub isolation: super::isolation::ViewportIsolation,
    pub sequence: u64,
    pub floor: u64,
    pub requested_modeling: Option<(u64, u64)>,
    pub modeling: Option<CanvasFrame>,
    pub requested_animation: Option<(AnimationIdentity, u64)>,
    pub animation: Option<AnimationViewportIdentity>,
}
impl ViewportPresentation {
    pub fn invalidate(&mut self) {
        self.floor = self.sequence + 1;
        self.requested_modeling = None;
        self.modeling = None;
        self.requested_animation = None;
        self.animation = None;
    }
    pub fn next(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }
}
