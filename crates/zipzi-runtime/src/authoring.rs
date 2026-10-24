//! Selected typed API for zipzi-python2. Both local
//! and IPC adapters use these values and the same Runtime handlers.
use crate::SourceRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSnapshot {
    pub source: SourceRef,
    pub id: String,
    pub name: String,
    pub geometry: zipzi_sdk::MeshGeometry,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshWriteResult {
    pub source: SourceRef,
    pub ids: Vec<String>,
}
