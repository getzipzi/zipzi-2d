//! Frozen source tasks. Scheduling values contain no mutable preview handles.
use serde::{Deserialize, Serialize};
use zipzi_api_derive::ApiShape;

#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(deny_unknown_fields)]
pub struct ParameterActivation {
    pub id: String,
    pub time: f32,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SampleInput {
    Parameters {
        values: Vec<crate::evaluation::PreviewValues>,
    },
    /// Combined playback sampled by canonical 60 Hz replay, including scheduled activations,
    /// fades and optional physics. This is not stateless raw-clip curve sampling.
    Animation {
        /// Ordered requested times in seconds on the preview timeline. Seeks to times at least
        /// 1/60 second replay from that first update; sampling zero does not seed later replay.
        times: Vec<f32>,
        #[serde(default)]
        /// Motion clip ID and activation time in seconds; the second value is not a weight.
        /// A clip starts on the first replay update at or after activation. A clip activated at
        /// zero therefore has local time max(0, requested_time - 1/60) in its first cycle.
        /// Inspect each result's motion_positions.local_time for the evaluated curve coordinate.
        motions: Vec<(String, f32)>,
        #[serde(default)]
        /// Expression ID and activation time in seconds on the same preview timeline.
        expressions: Vec<(String, f32)>,
        #[serde(default)]
        inputs: Vec<ParameterActivation>,
        #[serde(default)]
        physics: bool,
        #[serde(default)]
        stabilized_start: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(deny_unknown_fields)]
pub struct SampleRequest {
    pub input: SampleInput,
    #[serde(default)]
    pub queries: Vec<crate::spatial::SpatialQuery>,
    #[serde(default)]
    pub quality: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ApiShape)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Runtime,
    Cubism,
}
