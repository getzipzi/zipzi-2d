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
            ..self.identity
        } == other.identity
            && Arc::ptr_eq(&self.source, &other.source)
            && self.motion == other.motion
            && self.expressions == other.expressions
    }
}

impl AnimationSession {
    pub(super) fn accepts_content(&self, id: AnimationIdentity) -> bool {
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
            && id.request >= self.frames.floor
            && self
                .frames
                .latest()
                .is_some_and(|latest| id.request <= latest.request)
    }
    pub(super) fn accepts(&self, id: AnimationIdentity) -> bool {
        self.accepts_content(id)
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
            && self.editing_context() == Some(crate::EditingContext::Animation)
            && self.workspace_revision() == identity.workspace_revision
            && self.active_id() == Some(identity.document)
            && self.animation().is_some_and(|a| {
                a.model_generation == identity.model_generation
                    && a.frames.shown != 0
                    && a.frames.shown == identity.request
            })
    }
    pub fn accepts_animation(&self, identity: AnimationIdentity) -> bool {
        identity.workspace == WorkspaceKind::Animation
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
        self.adopt_animation_content(identity, snapshot);
        true
    }
    fn adopt_animation_content(
        &mut self,
        identity: AnimationIdentity,
        snapshot: Arc<MotionSnapshot>,
    ) {
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
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
    }
    pub fn fail_animation(&mut self, identity: AnimationIdentity, error: AnimationFailure) {
        // Intermediate successful frames are useful, but an obsolete candidate's
        // failure must not poison a newer, potentially valid target.
        if self.accepts_animation(identity)
            && self
                .animation()
                .is_some_and(|a| a.candidate == identity.candidate)
        {
            let animation = &mut self.active_session_mut().unwrap().animation.playback;
            animation.playing = false;
            animation.error = Some(error);
        }
    }
}

impl Editor {
    /// Content may have been presented by another viewport already. Its recipe
    /// and candidate epoch still have to be valid for a new projection.
    pub fn animation_content_valid(&self, identity: AnimationIdentity) -> bool {
        identity.workspace == WorkspaceKind::Animation
            && self.active_id() == Some(identity.document)
            && self.workspace_revision() == identity.workspace_revision
            && !self.is_busy()
            && !self.exiting()
            && self
                .animation()
                .is_some_and(|animation| animation.accepts_content(identity))
    }
    pub fn animation_viewport_request(
        &mut self,
        binding: crate::ViewportBinding,
        content: AnimationIdentity,
    ) -> Option<(crate::AnimationViewportIdentity, crate::ViewportTransform)> {
        if !self.viewport_binding_valid(binding)
            || binding.kind != crate::ViewportKind::Animation
            || binding.document != content.document
            || !self.animation_content_valid(content)
        {
            return None;
        }
        let metrics = self.viewport_metrics(binding.viewport)?;
        let session = self.active_session_mut()?;
        session.fit_default_view(binding.viewport, metrics).ok()?;
        let state = session.view_state(binding.viewport);
        let transform = crate::ViewportTransform::new(metrics, state.view)?;
        let presentation = self.viewport_presentation_mut(binding.viewport)?;
        if presentation.requested_animation == Some((content, state.revision)) {
            return None;
        }
        presentation.requested_animation = Some((content, state.revision));
        let identity = crate::AnimationViewportIdentity {
            binding,
            content,
            viewport_revision: state.revision,
            metrics,
            request: presentation.next(),
        };
        Some((identity, transform))
    }
    pub fn present_animation_viewport(
        &mut self,
        identity: crate::AnimationViewportIdentity,
        snapshot: Arc<MotionSnapshot>,
    ) -> bool {
        let binding = identity.binding;
        if !self.viewport_binding_valid(binding)
            || binding.kind != crate::ViewportKind::Animation
            || binding.document != identity.content.document
            || !self.animation_content_valid(identity.content)
            || self.viewport_metrics(binding.viewport) != Some(identity.metrics)
            || self
                .viewport_state(binding.document, binding.viewport)
                .is_none_or(|state| identity.viewport_revision > state.revision)
        {
            return false;
        }
        let presentation = self.viewport_presentation(binding.viewport).unwrap();
        if identity.request < presentation.floor
            || identity.request > presentation.sequence
            || presentation.animation.is_some_and(|shown| {
                shown.request >= identity.request
                    || shown.content.request > identity.content.request
            })
        {
            return false;
        }
        self.viewport_presentation_mut(binding.viewport)
            .unwrap()
            .animation = Some(identity);
        // Inspectors follow the newest presented content, while each viewport
        // can continue consuming its own intermediate frame without starvation.
        if self
            .animation()
            .is_some_and(|animation| identity.content.request > animation.frames.shown)
        {
            self.adopt_animation_content(identity.content, snapshot);
        }
        true
    }
    pub fn can_display_animation_viewport(
        &self,
        identity: crate::AnimationViewportIdentity,
    ) -> bool {
        self.viewport_binding_valid(identity.binding)
            && self
                .viewport_presentation(identity.binding.viewport)
                .is_some_and(|state| state.animation == Some(identity))
            && self.animation().is_some_and(|animation| {
                animation.model_generation == identity.content.model_generation
            })
    }
    pub fn accepts_animation_viewport_error(
        &self,
        identity: crate::AnimationViewportIdentity,
    ) -> bool {
        self.viewport_binding_valid(identity.binding)
            && self.animation_content_valid(identity.content)
            && self
                .animation()
                .is_some_and(|animation| animation.candidate == identity.content.candidate)
            && self
                .viewport_presentation(identity.binding.viewport)
                .is_some_and(|state| {
                    state.sequence == identity.request
                        && state.requested_animation
                            == Some((identity.content, identity.viewport_revision))
                })
    }
}
