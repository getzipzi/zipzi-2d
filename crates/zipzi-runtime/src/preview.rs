//! Typed controls and owned results for client-owned frozen animation previews.
use serde::Serialize;

pub use zipzi_contract::runtime::{PreviewCommand, PreviewState};

pub use zipzi_contract::runtime::{PreviewBatch, PreviewFrame};

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
