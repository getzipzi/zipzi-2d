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
use kasane_sdk::{AuthoringSnapshot, ExpressionTransition, MotionPreview, MotionSnapshot};
pub use provenance::{AnimationIdentity, AnimationRequest};
pub use resources::{ExpressionResource, ExpressionTrigger, MotionResource, MotionSelection};
use std::{
    collections::HashSet,
    sync::{Arc, OnceLock},
};

pub struct AnimationSource {
    /// Immutable committed model shared by all revisions of a gesture.
    pub snapshot: Arc<AuthoringSnapshot>,
    pub textures: Arc<[SceneTexture]>,
    /// Validated motion overlay; the base document remains unchanged.
    pub candidate: Option<Arc<kasane_sdk::MotionClipCandidate>>,
    pub(crate) materialized: OnceLock<Arc<AuthoringSnapshot>>,
}

impl AnimationSource {
    /// Result playback needs a document containing the candidate. Raw editing
    /// samples its curve overlay directly and shares the immutable model.
    pub fn materialized_snapshot(&self) -> Result<Arc<AuthoringSnapshot>> {
        let Some(candidate) = &self.candidate else {
            return Ok(self.snapshot.clone());
        };
        if let Some(snapshot) = self.materialized.get() {
            return Ok(snapshot.clone());
        }
        let snapshot = Arc::new(self.snapshot.with_motion_candidate(candidate)?);
        let _ = self.materialized.set(snapshot);
        Ok(self.materialized.get().unwrap().clone())
    }
}

pub struct AnimationSession {
    pub motion: Option<MotionSelection>,
    pub expression: Option<String>,
    pub expressions: Vec<ExpressionTrigger>,
    pub time: f32,
    pub edit_time: f32,
    pub(crate) head_located: bool,
    pub preview_mode: AnimationPreviewMode,
    pub preview_loop: bool,
    pub candidate: Option<TimelineCandidateIdentity>,
    result_position: Option<kasane_sdk::MotionPlaybackPosition>,
    pub playing: bool,
    /// A paused loop scrub denotes the first cycle, including its right endpoint.
    pub loop_position: bool,
    pub presented: Option<Arc<MotionSnapshot>>,
    pub error: Option<AnimationFailure>,
    pub model_generation: u64,
    pub preview_generation: u64,
    pub(crate) source: Option<Arc<AnimationSource>>,
    frames: provenance::FrameSequence,
    scrubbing: bool,
}
