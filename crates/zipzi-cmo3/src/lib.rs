//! Export editable Cubism CMO3 models and linked CAN3 animation projects.
//!
//! The codec has no filesystem, GUI, JVM or Cubism SDK dependency. A future CLI
//! can use `Document` as its conversion hub and supply assets from any importer.
mod caff;
mod import;
#[cfg(feature = "test-support")]
pub use import::test_support;
pub use import::{
    animation_model_paths, animation_model_paths_with_budget, import_animation,
    import_animation_with_budget, import_model, import_model_with_budget, ImportedModel,
};
mod can3;
mod canvas;
mod expression_semantics;
pub use can3::{
    encode_can3, encode_cubism, inspect_can3, inspect_cubism, Can3Artifact, Can3Options,
    CubismArtifact, CubismOptions,
};
mod export;
mod schema;
mod xml;

use serde::{Deserialize, Serialize};
use zipzi_core::{Document, ImageAsset};

/// How to handle document features that this exporter cannot represent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LossPolicy {
    /// Return all loss diagnostics and no artifact.
    #[default]
    Reject,
    /// Produce an artifact and retain the loss diagnostics in its report.
    Allow,
}

#[derive(Debug, Clone)]
pub struct Cmo3Options {
    pub model_name: String,
    pub loss_policy: LossPolicy,
    /// CAFF XOR key. Fixed by default for reproducible output; not encryption.
    pub obfuscation_key: i32,
}
impl Default for Cmo3Options {
    fn default() -> Self {
        Self {
            model_name: "Zipzi Model".into(),
            loss_policy: LossPolicy::Reject,
            obfuscation_key: 0x4b617361,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Loss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cmo3Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub object_id: String,
    pub message: String,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cmo3Report {
    pub diagnostics: Vec<Cmo3Diagnostic>,
}
impl Cmo3Report {
    pub fn has_loss(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Loss)
    }
}

#[derive(Debug, Clone)]
pub struct Cmo3Artifact {
    pub bytes: Vec<u8>,
    pub report: Cmo3Report,
}

#[derive(Debug, thiserror::Error)]
pub enum Cmo3Error {
    #[error("Invalid document: {0}")]
    InvalidDocument(String),
    #[error("Cannot export without loss ({})", .0.diagnostics.len())]
    Unsupported(Cmo3Report),
    #[error("Asset {asset_id}: {message}")]
    Asset { asset_id: String, message: String },
    #[error("Cubism archive exceeds the CAFF 32-bit size limit")]
    TooLarge,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}

/// Resource boundary shared by in-memory, project and future CLI adapters.
/// Return encoded PNG bytes; the codec verifies image dimensions before export.
pub trait AssetResolver {
    fn read_png(&self, asset: &ImageAsset) -> Result<Vec<u8>, Cmo3Error>;
}
impl<F> AssetResolver for F
where
    F: Fn(&ImageAsset) -> Result<Vec<u8>, Cmo3Error>,
{
    fn read_png(&self, asset: &ImageAsset) -> Result<Vec<u8>, Cmo3Error> {
        self(asset)
    }
}

/// Inspect conversion losses without reading resources or writing output.
pub fn inspect_export(document: &Document) -> Result<Cmo3Report, Cmo3Error> {
    export::inspect(document)
}

/// Encode a document and its resources entirely in memory. Never mutates input.
pub fn encode_cmo3(
    document: &Document,
    assets: &dyn AssetResolver,
    options: &Cmo3Options,
) -> Result<Cmo3Artifact, Cmo3Error> {
    export::encode(document, assets, options)
}
