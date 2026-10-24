//! Selected typed API for zipzi-python2. Both local
//! and IPC adapters use these values and the same Runtime handlers.
use crate::{ImageId, PlanId, SourceRef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSnapshot {
    pub source: SourceRef,
    pub id: String,
    pub name: String,
    pub geometry: zipzi_sdk::MeshGeometry,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub source: SourceRef,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditOperation {
    Mesh {
        operation: zipzi_sdk::MeshOperation,
    },
    CreateImage {
        spec: zipzi_sdk::ImageCreateSpec,
        image: ImageId,
    },
    ReplaceImage {
        id: String,
        image: ImageId,
    },
    ApplyPlan {
        plan: PlanId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanIntent {
    MeshRename {
        id: String,
        expected_name: String,
        name: String,
    },
    ImageRename {
        id: String,
        expected_name: String,
        name: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanSnapshot {
    pub plan: PlanId,
    pub source: SourceRef,
    pub report: zipzi_sdk::ModelingPlanReport,
}

/// The initial control transport intentionally limits pixel payloads to 1 MiB.
/// Larger/binary payloads require the later resource transport stage.
pub const MAX_IMAGE_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Serialize)]
pub struct PreparedImageInfo {
    pub image: ImageId,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImagePixels {
    pub width: u32,
    pub height: u32,
    pub rgba: std::sync::Arc<[u8]>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImageSnapshot {
    pub source: SourceRef,
    pub id: String,
    pub name: String,
    pub pixels: ImagePixels,
}
