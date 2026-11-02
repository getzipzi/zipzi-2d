//! Project format observation values.
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Reason retained opaque data cannot be exported safely under current source identities or content.
pub enum OpaqueExportProblem {
    /// Referenced runtime identity namespace no longer matches the imported data.
    NamespaceChanged,
    /// Retained source content changed since opaque data was captured.
    ContentChanged,
    /// Required source provenance is unavailable.
    ProvenanceMissing,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Opaque data or namespace guard that prevents a safe export.
pub enum OpaqueExportIssue {
    /// Model3 opaque metadata no longer matches its imported namespace.
    Model3NamespaceChanged,
    /// CDI opaque metadata failed its source guard.
    Cdi {
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
    /// A retained motion asset failed its opaque-data guard.
    Motion {
        /// Persistent retained resource UUID in the frozen source.
        id: String,
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
    /// A retained expression asset failed its opaque-data guard.
    Expression {
        /// Persistent retained resource UUID in the frozen source.
        id: String,
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
    /// Pose opaque data failed its source guard.
    Pose {
        /// Persistent retained resource UUID in the frozen source.
        id: String,
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
    /// Physics opaque data failed its source guard.
    Physics {
        /// Persistent retained resource UUID in the frozen source.
        id: String,
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
}

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(rename_all = "snake_case")]
/// Explicit source format used by host-side project import.
pub enum ImportFormat {
    /// Import a model3 descriptor and referenced resources.
    Model3,
    /// Import a layered PSD source.
    Psd,
    /// Import a Cubism project source.
    Cubism,
    /// Import a bare MOC3 with explicitly selected texture slots.
    Moc3,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(rename_all = "snake_case")]
/// Explicit host-side export representation.
pub enum ExportFormat {
    /// Export a project package.
    Package,
    /// Export a CMO3 model.
    Cmo3,
    /// Export a CAN3 animation.
    Can3,
    /// Export a Cubism project representation.
    Cubism,
    /// Export one selected Motion3 asset.
    Motion3,
    /// Export one selected Expression3 asset.
    Expression3,
    /// Export the Physics3 sidecar when present.
    Physics3,
    /// Export the Pose3 sidecar when present.
    Pose3,
    /// Export CDI3 display metadata.
    Cdi3,
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
/// Host-side project diagnosis, replacement, import or export request.
pub enum ProjectRequest {
    /// Inspect resource diagnostics without publishing model changes.
    Diagnose,
    /// Import an explicitly selected format into the selected Session.
    Import {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
        /// Explicit import/export format; does not change based on the installed backend.
        format: ImportFormat,
        /// External texture-slot to host-file mappings for bare MOC3 import; other import
        /// formats do not consume this list.
        textures: Vec<TextureSlot>,
    },
    /// Replace the selected Session from a host project path.
    Open {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
    },
    /// Replace the selected Session with a new document and model setup.
    New {
        /// Stable document UUID; not a Runtime Session handle.
        document_id: String,
        /// Model source-canvas and export-frame configuration.
        setup: crate::model::ModelSetup,
    },
    /// Export a selected format to a host path under an explicit loss policy.
    Export {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
        /// Explicit import/export format; does not change based on the installed backend.
        format: ExportFormat,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Stable Motion or Expression ID for the corresponding sidecar export; None supplies
        /// no object selection. Other formats do not use this selection.
        object_id: Option<String>,
        /// For CMO3, CAN3 and Cubism conversion, permit reported losses; false rejects
        /// unsupported loss. Other export formats do not consume this policy.
        allow_loss: bool,
        /// Whether CAN3/Cubism export may bake animation into the destination representation.
        bake_animation: bool,
        /// Whether CAN3/Cubism export may choose a frame rate automatically instead of requiring
        /// an explicit one.
        auto_frame_rate: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
/// Explicit external texture slot and host-side image path for bare MOC3 import.
pub struct TextureSlot {
    /// Zero-based external texture slot number used by the import format.
    pub slot: usize,
    /// Host-side image file path supplying this external texture slot.
    pub path: PathBuf,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
/// Machine-readable project warning or informational diagnostic.
pub struct ProjectDiagnostic {
    /// Diagnostic severity label; inspect code for stable programmatic handling.
    pub severity: String,
    /// Machine-readable reason or diagnostic code; use it instead of matching human-readable text.
    pub code: String,
    /// Stable document object ID related to this diagnostic; empty if no object was attributed.
    pub object_id: String,
    /// Human-readable diagnostic detail; code is the stable programmatic discriminator.
    pub message: String,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
/// Owned outcome of a project operation, including diagnostics, publication and durability.
pub struct ProjectReport {
    /// Exact SDK version before the operation.
    pub before: crate::authoring::Version,
    /// Exact SDK version after the operation; may equal before for a no-op or nonpublishing
    /// operation.
    pub after: crate::authoring::Version,
    /// Nonfatal operation warnings retained even when an output was produced.
    pub warnings: Vec<String>,
    /// Structured diagnostics from this query or operation; an empty list means none were reported.
    pub diagnostics: Vec<ProjectDiagnostic>,
    /// Whether the operation published its result: a replacement for import/open/new, or an
    /// output for export. Diagnose does not publish.
    pub published: bool,
    /// Operation-reported durability. Diagnose and New may report true without writing a file;
    /// inspect the SaveReceipt separately when asserting project persistence.
    pub durable: bool,
    /// Host-side output paths actually reported by the operation; no client-side path
    /// translation is implied.
    pub paths: Vec<String>,
}
