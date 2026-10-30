#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct TriangleQuality {
    /// Zero-based triangle offset, in the sampled drawable's index order.
    pub triangle_index: usize,
    /// Unsigned evaluated area in model units squared.
    pub area: f64,
    /// Smallest interior angle in degrees; zero for a collapsed triangle.
    pub min_angle_degrees: f64,
    /// Longest edge divided by its altitude. None for a collapsed triangle.
    pub aspect_ratio: Option<f64>,
    /// Signed area / signed default-pose area. Negative means flipped; None if the reference is degenerate.
    pub relative_area: Option<f64>,
    /// Largest symmetric edge-length ratio max(current/reference, reference/current).
    /// None if any reference or sampled edge is collapsed. A rigid transform gives 1.
    pub edge_stretch: Option<f64>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct TextureCoverage {
    /// Alpha >= this value counts as painted. Coverage ignores filtering, masks and occlusion.
    pub alpha_threshold: u8,
    /// Top-down source-pixel grid spacing. Values above 1 mean an approximate report.
    pub pixel_step: u32,
    /// Number of source pixel centers tested on the regular grid.
    pub sampled_pixels: usize,
    /// Tested pixel centers covered by the union of UV triangles inside [0,1].
    pub covered_pixels: usize,
    /// Covered centers whose source alpha is below the threshold.
    pub transparent_covered_pixels: usize,
    /// Tested centers whose source alpha is at least the threshold.
    pub opaque_pixels: usize,
    /// Opaque centers outside the UV triangle union: a contour coverage error, not a distance.
    pub uncovered_opaque_pixels: usize,
    /// Transparent covered / covered; None if no tested center is covered.
    pub transparent_coverage_fraction: Option<f64>,
    /// Uncovered opaque / opaque; None for an entirely transparent sampled image.
    pub uncovered_opaque_fraction: Option<f64>,
    /// Number of UV triangles with vertices outside [0,1]; repeat-sampled coverage is not measured.
    pub out_of_bounds_triangles: usize,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct MeshQuality {
    /// Stable document mesh ID, in evaluated drawable order.
    pub mesh_id: String,
    /// Evaluated enabled/visible/positive-opacity state. Metrics are geometric and ignore occlusion.
    pub visible: bool,
    /// Number of evaluated vertices.
    pub vertex_count: usize,
    /// Per-triangle metrics in evaluated Y-up model coordinates, relative to default parameters
    /// of the same frozen authoring source. This does not certify unsampled parameter intervals.
    pub triangles: Vec<TriangleQuality>,
    /// Smallest angle over this mesh; None for an empty mesh.
    pub min_angle_degrees: Option<f64>,
    /// Largest finite aspect ratio; collapsed triangles are counted separately.
    pub max_aspect_ratio: Option<f64>,
    /// Largest finite symmetric edge-length ratio relative to the default pose.
    pub max_edge_stretch: Option<f64>,
    /// Number of exactly zero-area evaluated triangles.
    pub degenerate_triangles: usize,
    /// Number of negative relative-area triangles. Degenerate references are excluded.
    pub flipped_triangles: usize,
    /// Optional source-texture coverage. CPU-only sampling does not resolve image pixels.
    pub texture_coverage: Option<TextureCoverage>,
}

#[derive(Debug, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ModelingSamples {
    /// Exact SDK version captured for this value; not a best-effort latest-version marker.
    pub version: Version,
    /// Effective parameter maps in the same order as requested poses; each map is keyed by
    /// document parameter ID.
    pub parameters: Vec<crate::evaluation::PreviewValues>,
    /// CPU-evaluated frames in the same order as requested parameter maps.
    pub frames: Vec<crate::evaluation::DrawableFrame>,
    /// Structured diagnostics from this query or operation; an empty list means none were reported.
    pub diagnostics: Vec<GeometryDiagnostic>,
    /// Per-pose evaluated mesh quality in frame order when Checks.quality is true; empty otherwise.
    /// Uses default parameters of this same frozen source as the reference, without image coverage.
    pub quality: Vec<Vec<MeshQuality>>,
}

/// Explicit diagnostic drawing options; overlays ignore masks and occlusion.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(default, deny_unknown_fields)]
pub struct TopologyOverlay {
    /// Diagnostic selection only; the textured context image is unchanged. None selects all
    /// meshes, Some([]) selects none. Mesh/Part targets are unioned and deduplicated;
    /// Parts include nested Parts. Missing targets fail explicitly. Maximum 256 selectors.
    pub targets: Option<Vec<crate::spatial::ObjectTarget>>,
    /// Draw triangle edges in cyan, in final evaluated model coordinates.
    #[serde(default = "default_wireframe")]
    pub wireframe: bool,
    /// Draw vertex positions as yellow dots.
    pub vertices: bool,
    /// Draw stable authoring vertex IDs in yellow, rather than transient array offsets.
    pub vertex_ids: bool,
    /// Draw meshing boundary edges in green and internal constraints in magenta.
    pub constraints: bool,
    /// Fill triangles by symmetric edge stretch relative to the default pose: blue=1,
    /// orange=2, red>=4; flipped or collapsed triangles are magenta.
    pub strain: bool,
    /// Include disabled, hidden and zero-opacity meshes. Default draws visible meshes only.
    pub include_hidden: bool,
    /// Clear the textured image before diagnostic drawing, retaining the exact same view.
    pub topology_only: bool,
    /// Heat fill opacity in [0,1]. Applied once per pixel using the most severe overlapping
    /// triangle, so overlapping layers cannot accumulate opacity. Lines and labels stay opaque.
    #[serde(default = "default_heat_opacity")]
    pub heat_opacity: f32,
    /// Restrict every diagnostic layer to anomalous triangles and their incident vertices/edges.
    /// Degenerate, flipped and undefined-reference triangles always count as anomalies.
    pub anomalies_only: bool,
    /// Thresholds used by anomalies_only; reports themselves are never threshold-filtered.
    pub thresholds: TopologyThresholds,
    /// Decimal glyph scale in output pixels, 1..4. Labels avoid other labels globally;
    /// an ID is omitted when no nearby in-view position has enough space.
    #[serde(default = "default_label_scale")]
    pub label_scale: u32,
    /// Minimum clear gap between label rectangles in pixels, 0..32.
    #[serde(default = "default_label_spacing")]
    pub label_spacing: u32,
    /// Maximum labels per image, 0..4096. Vertex dots remain independent of this limit.
    #[serde(default = "default_max_labels")]
    pub max_labels: usize,
}
fn default_heat_opacity() -> f32 {
    0.25
}
fn default_label_scale() -> u32 {
    2
}
fn default_label_spacing() -> u32 {
    3
}
fn default_max_labels() -> usize {
    256
}
fn default_wireframe() -> bool {
    true
}
impl Default for TopologyOverlay {
    fn default() -> Self {
        Self {
            targets: None,
            wireframe: true,
            vertices: false,
            vertex_ids: false,
            constraints: false,
            strain: false,
            include_hidden: false,
            topology_only: false,
            heat_opacity: default_heat_opacity(),
            anomalies_only: false,
            thresholds: TopologyThresholds::default(),
            label_scale: default_label_scale(),
            label_spacing: default_label_spacing(),
            max_labels: default_max_labels(),
        }
    }
}

/// Advisory anomaly thresholds in evaluated model space, relative to the default pose.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(default, deny_unknown_fields)]
pub struct TopologyThresholds {
    /// An angle below this value is anomalous; finite degrees in [0,60]. Default 10.
    #[serde(default = "default_min_angle")]
    pub min_angle_degrees: f64,
    /// A longest-edge/altitude ratio above this is anomalous; finite and >= 1. Default 20.
    #[serde(default = "default_max_aspect")]
    pub max_aspect_ratio: f64,
    /// A symmetric edge-length ratio above this is anomalous; finite and >= 1. Default 2.
    #[serde(default = "default_max_stretch")]
    pub max_edge_stretch: f64,
}
fn default_min_angle() -> f64 {
    10.
}
fn default_max_aspect() -> f64 {
    20.
}
fn default_max_stretch() -> f64 {
    2.
}
impl Default for TopologyThresholds {
    fn default() -> Self {
        Self {
            min_angle_degrees: default_min_angle(),
            max_aspect_ratio: default_max_aspect(),
            max_edge_stretch: default_max_stretch(),
        }
    }
}
