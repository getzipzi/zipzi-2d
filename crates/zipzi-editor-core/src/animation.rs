//! Detached playback descriptions and frame provenance. No authoring edits.
mod evaluation;
mod playback;
mod provenance;
mod resources;
mod session;
use crate::{
    AnimationFailure, AnimationPreviewMode, Document, DocumentId, Editor, SceneSnapshot,
    SceneTexture, TimelineCandidateIdentity, WorkspaceKind,
};
use anyhow::{Result, ensure};
pub use evaluation::{AnimationEvaluated, AnimationEvaluator};
pub use provenance::{AnimationIdentity, AnimationRequest};
pub use resources::{ExpressionResource, ExpressionTrigger, MotionResource, MotionSelection};
use std::sync::Arc;
use zipzi_animation::ExpressionTransition;
use zipzi_animation::MotionPreview;
use zipzi_animation::MotionSnapshot;
use zipzi_sdk::{Snapshot, SourcedSnapshot};

pub struct AnimationSource {
    /// Captured model/resource pair with its exact published or inspector source.
    pub snapshot: Arc<SourcedSnapshot>,
    pub textures: Arc<[SceneTexture]>,
    /// A Timeline preview carries a complete, validated Draft snapshot.
    pub candidate: Option<Arc<SourcedSnapshot>>,
}

impl AnimationSource {
    pub fn document(&self) -> &zipzi_core::state::DocumentState {
        self.snapshot.snapshot.document()
    }

    pub(super) fn same_document(&self, other: &Self) -> bool {
        self.snapshot.source == other.snapshot.source
            && self.snapshot.snapshot.content_id() == other.snapshot.snapshot.content_id()
    }

    pub fn evaluation_snapshot(&self) -> &Snapshot {
        &self.candidate.as_ref().unwrap_or(&self.snapshot).snapshot
    }
}

pub struct AnimationSession {
    pub physics_preview: crate::PhysicsPreviewSettings,
    pub(crate) cursor_physics: Option<crate::CursorPhysicsPreview>,
    pub(crate) physics_return: Option<(f32, f32)>,
    pub(crate) inspected: Option<crate::ObjectRef>,
    pub motion: Option<MotionSelection>,
    pub expression: Option<String>,
    pub expressions: Vec<ExpressionTrigger>,
    pub time: f32,
    pub edit_time: f32,
    pub(crate) head_located: bool,
    pub preview_mode: AnimationPreviewMode,
    pub preview_loop: bool,
    /// Preview clock multiplier; never changes the authored motion timing.
    pub playback_speed: f32,
    pub candidate: Option<TimelineCandidateIdentity>,
    result_position: Option<zipzi_animation::MotionPlaybackPosition>,
    pub playing: bool,
    /// A paused loop scrub denotes the first cycle, including its right endpoint.
    pub loop_position: bool,
    pub presented: Option<Arc<MotionSnapshot>>,
    pub error: Option<AnimationFailure>,
    pub model_generation: u64,
    document_revision: u64,
    pub preview_generation: u64,
    pub(crate) source: Option<Arc<AnimationSource>>,
    frames: provenance::FrameSequence,
    scrubbing: bool,
}
