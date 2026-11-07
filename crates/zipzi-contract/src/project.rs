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
