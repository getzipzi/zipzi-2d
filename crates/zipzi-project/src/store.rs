//! Resource preparation and value-only format imports.
mod assets;
mod import;
mod manifest;
pub(crate) use assets::rgba_size;
pub use assets::{
    prepare_image_content, prepare_image_content_with_budget, prepare_png_content,
    prepare_project_image, read_project_asset, read_project_asset_if_changed,
    read_project_texture_bounded, PreparedImageContent, VerifiedTexture,
};
pub use manifest::project_identity;
use zipzi_core::Status;
#[derive(Debug, Clone, Default)]
pub struct ProjectImporter {
    pub resources: crate::ResourceStore,
}
#[derive(Debug, Clone)]
pub struct ImportedProject {
    pub document: zipzi_core::state::DocumentState,
    pub resources: crate::ResourceView,
    pub protected_paths: Vec<std::path::PathBuf>,
}
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
