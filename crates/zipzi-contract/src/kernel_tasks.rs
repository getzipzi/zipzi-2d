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
    Animation {
        times: Vec<f32>,
        #[serde(default)]
        motions: Vec<(String, f32)>,
        #[serde(default)]
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
