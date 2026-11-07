//! Observation inputs, texture plans and verified resource reports.

use serde::{Deserialize, Serialize};

/// Default host policy bound for observation base levels, distinct from GPU limits.
pub const DEFAULT_OBSERVATION_TEXTURE_RGBA_BYTES: u64 = 256 << 20;

/// Explicit base-level texture policy. Budgets count logical straight RGBA8
/// bytes per referenced asset ID, independently of physical buffer sharing.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservationTexturePolicy {
    /// Preserve source dimensions and decoded RGBA8 pixels without downsampling.
    Original {},
    /// Deterministically downsample referenced base textures to fit a logical RGBA8 budget.
    FitBudget {
        /// Positive integer no greater than the host's base RGBA limit.
        max_rgba_bytes: u64,
    },
}

impl Default for ObservationTexturePolicy {
    fn default() -> Self {
        Self::Original {}
    }
}

/// Pixel dimensions; zero axes are invalid resource metadata.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(deny_unknown_fields)]
pub struct ObservationTextureSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// A source content identity includes its representation. A PNG byte digest
/// and a domain-separated straight RGBA8 pixel digest are not interchangeable.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservationContentIdentity {
    /// Digest of the encoded source PNG bytes.
    PngSha256 {
        /// SHA256 of encoded source PNG bytes; verification is a separate state.
        hash: String,
    },
    /// Domain-separated digest of source dimensions and straight RGBA8 pixels.
    Rgba8Sha256 {
        /// Domain-separated hash including dimensions and pixel conventions.
        hash: String,
    },
}

/// Metadata-only source description. A declared identity has not been verified
/// by estimation; Source freezes the document, not external file bytes.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(deny_unknown_fields)]
pub struct ObservationTextureSource {
    /// Stable image asset ID, counted once across all batch frames.
    pub asset_id: String,
    /// Declared source dimensions, without opening or decoding the image.
    pub source_size: ObservationTextureSize,
    /// Declared representation-specific identity, if available in metadata.
    pub source_content_identity: Option<ObservationContentIdentity>,
}

/// Source metadata and planned rendering dimensions for one referenced texture.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(deny_unknown_fields)]
pub struct ObservationTextureTarget {
    /// Declared source metadata; the executor must validate it again.
    pub source: ObservationTextureSource,
    /// Planned base-level rendering dimensions.
    pub render_size: ObservationTextureSize,
}

/// Feasible deterministic CPU plan. Clients cannot submit this as a trusted
/// execution plan. Mipmap bytes exclude the base level and are logical bytes,
/// not GPU allocation measurements.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(deny_unknown_fields)]
pub struct ObservationTexturePlan {
    /// Requested policy; equivalent actual pixels may have different policies.
    pub policy: ObservationTexturePolicy,
    /// Requested FitBudget bound or the host base RGBA limit for Original.
    pub effective_max_rgba_bytes: u64,
    /// Maximum permitted longest edge; None only for an empty reference set.
    pub longest_edge_limit: Option<u32>,
    /// Logical RGBA bytes of referenced source images, excluding unused assets.
    pub source_rgba_bytes: u64,
    /// Logical RGBA bytes of planned base levels.
    pub render_rgba_bytes: u64,
    /// Sum of every non-base mip level, halving each axis down to 1x1.
    pub estimated_mipmap_bytes: u64,
    /// Number of assets with changed base dimensions.
    pub resized_count: usize,
    /// Version of reference collection, integer scaling and search semantics.
    pub planning_version: u32,
    /// Version of mip dimensions and byte-count semantics.
    pub mipmap_version: u32,
    /// Complete reference set and targets sorted by asset ID.
    pub textures: Vec<ObservationTextureTarget>,
}

/// A valid policy can be statically infeasible. Retain source dimensions for
/// diagnosis instead of treating a budget shortfall as malformed input.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservationTexturePlanning {
    /// A feasible metadata plan; GPU device limits have not been checked.
    Ready {
        /// Feasible CPU dimensions; device limits remain unverified.
        plan: ObservationTexturePlan,
    },
    /// The requested budget cannot accommodate the referenced textures.
    InsufficientBudget {
        /// Requested policy.
        policy: ObservationTexturePolicy,
        /// Effective base RGBA bound.
        effective_max_rgba_bytes: u64,
        /// Total logical source bytes, without decoding.
        source_rgba_bytes: u64,
        /// Source bytes for Original, or four bytes per texture for FitBudget.
        minimum_required_rgba_bytes: u64,
        /// Source metadata in asset ID order; no target plan exists.
        textures: Vec<ObservationTextureSource>,
    },
}

/// Verified source and actual rendering content for one referenced texture.
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservationTextureReportEntry {
    /// Stable image asset ID; distinct IDs count separately even for equal pixels.
    pub asset_id: String,
    /// Verified source pixel dimensions before any observation downsampling.
    pub source_size: ObservationTextureSize,
    /// Actual straight RGBA8 rendering base dimensions.
    pub render_size: ObservationTextureSize,
    /// Verified PNG-byte or domain-separated source-pixel identity.
    pub source_content_identity: ObservationContentIdentity,
    /// Domain-separated hash of actual top-left row-major straight RGBA8 base pixels.
    pub render_rgba_hash: String,
}

/// Texture policy and verified content used across an observation batch.
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservationTextureReport {
    /// Requested texture policy; not part of rendering-content digests.
    pub policy: ObservationTexturePolicy,
    /// Effective host or caller base RGBA bound, in logical bytes.
    pub effective_max_rgba_bytes: u64,
    /// Common planned integer longest-edge bound; None for an empty set.
    pub longest_edge_limit: Option<u32>,
    /// Logical RGBA source bytes of the referenced asset set.
    pub source_rgba_bytes: u64,
    /// Actual logical base RGBA bytes; excludes mipmaps.
    pub render_rgba_bytes: u64,
    /// Exact logical non-base mip bytes, not driver allocation measurements.
    pub estimated_mipmap_bytes: u64,
    /// Number of assets whose base dimensions changed.
    pub resized_count: usize,
    /// Fixed straight RGBA alpha-weighted area-average filter name.
    pub filter: String,
    /// Area filter and rounding version.
    pub algorithm_version: u32,
    /// Reference collection and integer planning version.
    pub planning_version: u32,
    /// Mipmap dimension and filtering version.
    pub mipmap_version: u32,
    /// Verified source and derived content, in asset ID order, once per batch.
    pub textures: Vec<ObservationTextureReportEntry>,
}

/// Output dimensions and Y-up model-space region for GPU observation.
#[derive(
    Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire,
)]
#[serde(deny_unknown_fields)]
pub struct ObservationView {
    /// Explicit texture policy; Original preserves source pixels and dimensions.
    #[serde(default)]
    pub texture_policy: ObservationTexturePolicy,
    /// Output width in pixels, positive and subject to host/device limits (host default 4096).
    pub width: u32,
    /// Output height in pixels, positive and subject to host/device limits (host default 4096).
    pub height: u32,
    /// Nonempty finite [min_x,min_y,max_x,max_y] in Y-up model coordinates.
    pub roi: [f32; 4],
    /// Nonnegative padding around the ROI in model units; aspect ratio is preserved.
    #[serde(default)]
    pub padding_model: f32,
    /// Optional topology overlay in the same frozen Source and ROI mapping. It ignores
    /// masks and occlusion; None preserves the ordinary GPU image.
    #[serde(default)]
    pub topology: Option<crate::authoring::TopologyOverlay>,
    /// Include per-mesh evaluated quality and approximate source-alpha coverage.
    /// Uses default parameters of the same source as reference; checks requested frames only.
    #[serde(default)]
    pub quality: bool,
}

impl ObservationView {
    pub fn needs_quality(&self) -> bool {
        self.quality
            || self
                .topology
                .as_ref()
                .is_some_and(|o| o.strain || o.anomalies_only)
    }
}
