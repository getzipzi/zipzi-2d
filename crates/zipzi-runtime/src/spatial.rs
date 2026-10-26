//! Frozen, bounded CPU queries. SDK owns geometric selection and hit semantics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpatialSampleBatch {
    pub source: crate::SourceRef,
    pub samples: zipzi_sdk::SpatialSamples,
}
