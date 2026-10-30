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
pub enum OpaqueExportProblem {
    NamespaceChanged,
    ContentChanged,
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
pub enum OpaqueExportIssue {
    Model3NamespaceChanged,
    Cdi {
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
    Motion {
        /// Persistent retained resource UUID in the frozen source.
        id: String,
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
    Expression {
        /// Persistent retained resource UUID in the frozen source.
        id: String,
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
    Pose {
        /// Persistent retained resource UUID in the frozen source.
        id: String,
        /// Exact native opaque-field guard violated by this retained resource.
        problem: OpaqueExportProblem,
    },
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
pub enum ImportFormat {
    Model3,
    Psd,
    Cubism,
    Moc3,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Package,
    Cmo3,
    Can3,
    Cubism,
    Motion3,
    Expression3,
    Physics3,
    Pose3,
    Cdi3,
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectRequest {
    Diagnose,
    Import {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
        /// Explicit import/export format; does not change based on the installed backend.
        format: ImportFormat,
        /// Explicit external texture-slot to host-file mappings used by imports requiring
        /// separate images.
        textures: Vec<TextureSlot>,
    },
    Open {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
    },
    New {
        /// Stable document UUID; not a Runtime Session handle.
        document_id: String,
        /// Model source-canvas and export-frame configuration.
        setup: crate::model::ModelSetup,
    },
    Export {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
        /// Explicit import/export format; does not change based on the installed backend.
        format: ExportFormat,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Stable document ID of the object described or selected by this value.
        object_id: Option<String>,
        /// Explicit permission to export despite format-conversion losses; false rejects
        /// unsupported loss.
        allow_loss: bool,
        /// Whether export may bake animation into the destination representation.
        bake_animation: bool,
        /// Whether export may choose a frame rate automatically instead of requiring an
        /// explicit one.
        auto_frame_rate: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
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
    /// Whether the operation published its result; inspect durable separately for persistence
    /// guarantees.
    pub published: bool,
    /// Whether the reported persistence step achieved its durability guarantee; job completion
    /// alone does not imply true.
    pub durable: bool,
    /// Host-side output paths actually reported by the operation; no client-side path
    /// translation is implied.
    pub paths: Vec<String>,
}
