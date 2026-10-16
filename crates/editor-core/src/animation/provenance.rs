//! Frame identities, monotonic request sequencing and result acceptance.
use super::*;

/// A barrier invalidates outstanding requests without reusing a sequence number.
#[derive(Default)]
pub(super) struct FrameSequence {
    requested: Option<AnimationIdentity>,
    floor: u64,
    shown: u64,
}
impl FrameSequence {
    pub(super) fn latest(&self) -> Option<AnimationIdentity> {
        self.requested
    }
    pub(super) fn acceptance_floor(&self) -> u64 {
        self.floor
    }
    pub(super) fn record(&mut self, identity: AnimationIdentity) {
        self.requested = Some(identity);
    }

    pub(super) fn next(&self) -> u64 {
        self.floor.max(
            self.requested
                .map_or(self.shown + 1, |previous| previous.request + 1),
        )
    }
    pub(super) fn barrier(&mut self) {
        self.floor = self.next();
        self.requested = None;
    }
    fn accepts(&self, request: u64) -> bool {
        request >= self.floor
            && request > self.shown
            && self
                .requested
                .is_some_and(|latest| request <= latest.request)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationIdentity {
    pub document: DocumentId,
    pub workspace: WorkspaceKind,
    pub workspace_revision: u64,
    pub model_generation: u64,
    pub preview_generation: u64,
    pub request: u64,
    pub acceptance_floor: u64,
    pub viewport_revision: u64,
    pub canvas_metrics: crate::CanvasMetrics,
    pub time: f32,
    pub preview_mode: AnimationPreviewMode,
    pub candidate: Option<TimelineCandidateIdentity>,
}

#[derive(Clone)]
pub struct AnimationRequest {
    pub identity: AnimationIdentity,
    pub source: Arc<AnimationSource>,
    pub motion: Option<MotionSelection>,
    /// Complete ordered schedule, including future events after a backward seek.
    pub expressions: Vec<ExpressionTrigger>,
}
impl AnimationRequest {
    pub(super) fn same_pose(&self, other: &Self) -> bool {
        // Navigation and acceptance barriers change frame provenance, not pose.
        // Compare the full recipe as well: detached callers may replace it
        // without going through the editor's preview-generation bookkeeping.
        AnimationIdentity {
            request: other.identity.request,
            acceptance_floor: other.identity.acceptance_floor,
            viewport_revision: other.identity.viewport_revision,
            canvas_metrics: other.identity.canvas_metrics,
            ..self.identity
        } == other.identity
            && Arc::ptr_eq(&self.source, &other.source)
            && self.motion == other.motion
            && self.expressions == other.expressions
    }
}

impl AnimationSession {
    pub(super) fn accepts(&self, id: AnimationIdentity) -> bool {
        id.model_generation == self.model_generation
            && (id.candidate == self.candidate
                || id
                    .candidate
                    .zip(self.candidate)
                    .is_some_and(|(frame, current)| {
                        frame.edit == current.edit && frame.revision <= current.revision
                    }))
            && id.preview_mode == self.preview_mode
            && id.preview_generation == self.preview_generation
            && self.frames.accepts(id.request)
            && ((self.preview_mode == AnimationPreviewMode::Editing && self.preview_loop)
                || self.scrubbing
                || self
                    .presented
                    .as_ref()
                    .is_none_or(|shown| id.time >= shown.time))
    }
}

impl Editor {
    /// Keep the last presented image visible while a new recipe is evaluated.
    /// Incoming CPU/GPU results must still pass `accepts_animation` / `present_animation`.
    pub fn can_display_animation_image(&self, identity: AnimationIdentity) -> bool {
        identity.workspace == WorkspaceKind::Animation
            && self.workspace() == WorkspaceKind::Animation
            && self.workspace_revision() == identity.workspace_revision
            && self.active_id() == Some(identity.document)
            && self.animation().is_some_and(|a| {
                a.model_generation == identity.model_generation
                    && a.frames.shown != 0
                    && a.frames.shown == identity.request
            })
    }
    pub fn accepts_animation(&self, identity: AnimationIdentity) -> bool {
        self.workspace() == identity.workspace
            && self.workspace() == WorkspaceKind::Animation
            && self.workspace_revision() == identity.workspace_revision
            && self.active_id() == Some(identity.document)
            && !self.is_busy()
            && !self.exiting()
            && self
                .animation()
                .is_some_and(|animation| animation.accepts(identity))
    }
    pub fn present_animation(
        &mut self,
        identity: AnimationIdentity,
        snapshot: Arc<MotionSnapshot>,
    ) -> bool {
        if !self.accepts_animation(identity) {
            return false;
        }
        // Navigation may advance while a frame is in flight. Its texture still
        // represents a valid intermediate view; request order prevents rollback.
        // A resized surface, however, must never adopt the old-sized texture.
        if self
            .animation_viewport_revision()
            .is_none_or(|current| identity.viewport_revision > current)
            || identity.canvas_metrics != self.canvas_metrics()
        {
            return false;
        }
        let animation = &mut self.active_session_mut().unwrap().animation;
        animation.frames.shown = identity.request;
        if animation.preview_mode == AnimationPreviewMode::Result {
            animation.result_position = animation
                .motion
                .as_ref()
                .and_then(|motion| {
                    snapshot
                        .motion_positions
                        .iter()
                        .find(|position| position.motion_id == motion.clip_id)
                })
                .cloned();
        }
        if animation.playing || !animation.head_located {
            animation.edit_time = animation
                .motion
                .as_ref()
                .and_then(|motion| {
                    snapshot
                        .motion_positions
                        .iter()
                        .find(|position| position.motion_id == motion.clip_id)
                })
                .map_or(snapshot.time, |position| position.local_time);
        }
        animation.presented = Some(snapshot);
        true
    }
    pub fn fail_animation(&mut self, identity: AnimationIdentity, error: AnimationFailure) {
        // Intermediate successful frames are useful, but an obsolete candidate's
        // failure must not poison a newer, potentially valid target.
        if self.accepts_animation(identity)
            && self
                .animation()
                .is_some_and(|a| a.candidate == identity.candidate)
        {
            let animation = &mut self.active_session_mut().unwrap().animation;
            animation.playing = false;
            animation.error = Some(error);
        }
    }
}
