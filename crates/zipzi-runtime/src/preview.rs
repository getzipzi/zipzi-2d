//! Typed controls and owned results for client-owned frozen animation previews.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewCommand {
    SetParameter {
        /// Document parameter UUID; the SDK validates existence and domain.
        id: String,
        /// Finite base value in the parameter's authored domain.
        value: f32,
    },
    ScheduleMotion {
        /// Clip UUID; uses the clip's own fades.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
    },
    ScheduleRegistration {
        /// Registration UUID; uses the registration's SDK fade overrides.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
    },
    ScheduleExpression {
        /// Native expression asset UUID.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
    },
    ScheduleParameter {
        /// Document parameter UUID for an SDK input activation.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
        /// Finite parameter input in the authored parameter domain.
        value: f32,
    },
    SetPhysics {
        /// Enable the SDK Physics stage in this Preview only.
        enabled: bool,
        /// Use the SDK stabilized-start policy on canonical replay.
        stabilized_start: bool,
    },
    Reset,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
pub struct PreviewState {
    /// Monotonic accepted operation ordinal, including cancelled sample requests.
    pub sequence: u64,
    /// Actual SDK clock after the last completed operation, in seconds.
    pub time: f32,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct PreviewFrame {
    /// Optional GPU image from this exact animation state and once-resolved batch textures.
    pub observation: Option<crate::observation::ObservationImage>,
    /// Geometric query results in the corresponding request order, from this exact SDK state.
    pub spatial: Vec<zipzi_sdk::SpatialResult>,
    /// Runtime-wide Preview serial; pair with the outer Source.runtime for view identity.
    pub preview_id: u64,
    /// Accepted sample request ordinal; pair with the outer Source and Preview identity.
    pub sequence: u64,
    /// Zero-based target slot in the ordered sample request.
    pub index: usize,
    /// SDK Motion/Expression/Physics/Pose result and actual sample coordinates.
    pub motion: zipzi_sdk::MotionSnapshot,
    /// Evaluated CPU geometry when requested; model_opacity remains a separate SDK channel.
    pub geometry: Option<zipzi_core::DrawableFrame>,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct PreviewBatch {
    /// Explicit image view when GPU observation was requested; None keeps CPU-only behavior.
    pub view: Option<crate::observation::ObservationView>,
    /// Owned geometric query descriptors aligned with every frame.spatial slot.
    pub queries: Vec<zipzi_sdk::SpatialQuery>,
    /// Accepted sample request ordinal shared by these ordered frames.
    pub sequence: u64,
    /// Owned frames in requested target order, retaining duplicate and backward targets.
    pub frames: Vec<PreviewFrame>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewSampleBatch {
    pub source: crate::SourceRef,
    pub samples: PreviewBatch,
}

pub(crate) fn animation_error(error: zipzi_sdk::AnimationError) -> crate::RuntimeError {
    if matches!(error, zipzi_sdk::AnimationError::SeekCancelled) {
        crate::RuntimeError::Cancelled
    } else {
        crate::RuntimeError::Sdk(zipzi_sdk::SdkError {
            code: "ANIMATION_PREVIEW".into(),
            message: error.to_string().into(),
            operation: "preview",
            object_ids: vec![],
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        })
    }
}
