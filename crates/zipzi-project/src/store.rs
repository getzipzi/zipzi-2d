//! Project storage entry points. Asset handling, persistence, import assembly,
//! and document session state live in separate implementation modules.
mod assets;
mod import;
mod manifest;
mod session;

use crate::filesystem::{FileSystem, NativeFileSystem};
use assets::MemoryAsset;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use zipzi_core::{Document, Status};

pub(crate) use assets::asset_path;
pub use assets::{
    PreparedImageContent, prepare_image_content, read_project_asset, read_project_asset_if_changed,
};
pub use manifest::{project_identity, project_manifest};
pub use session::DocumentSession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceDiagnostic {
    pub asset_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectResult {
    pub status: Status,
    pub diagnostics: Vec<ResourceDiagnostic>,
    pub warnings: Vec<String>,
    pub published: bool,
    pub durable: bool,
}

impl ProjectResult {
    pub fn ok() -> Self {
        Self {
            status: Status::ok(),
            diagnostics: Vec::new(),
            warnings: Vec::new(),
            published: true,
            durable: true,
        }
    }

    pub fn failed(code: &str, message: &str) -> Self {
        Self {
            status: Status::error(code, message),
            diagnostics: Vec::new(),
            warnings: Vec::new(),
            published: false,
            durable: false,
        }
    }

    pub fn from_status(status: Status) -> Self {
        Self {
            status,
            ..Default::default()
        }
    }

    pub fn resources_complete(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct DocumentSnapshot {
    pub document: Document,
    pub manifest: PathBuf,
    pub manifest_sha256: String,
}

pub struct DocumentStore {
    filesystem: Arc<dyn FileSystem>,
    verified_pngs: Mutex<HashMap<String, (u32, u32)>>,
    memory_assets: Mutex<HashMap<String, Arc<MemoryAsset>>>,
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentStore {
    pub fn new() -> Self {
        Self::with_filesystem(Arc::new(NativeFileSystem))
    }

    pub fn with_filesystem(filesystem: Arc<dyn FileSystem>) -> Self {
        Self {
            filesystem,
            verified_pngs: Mutex::new(HashMap::new()),
            memory_assets: Mutex::new(HashMap::new()),
        }
    }
}
