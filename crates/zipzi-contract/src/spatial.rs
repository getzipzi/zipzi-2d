//! Data for bounded geometric queries.
use serde::{Deserialize, Serialize};

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
/// A mesh or Part selected by its stable authoring ID.
///
/// A Part selects every descendant mesh, including meshes in nested Parts.
pub enum ObjectTarget {
    /// Select exactly one mesh.
    Mesh(
        /// Persistent authoring mesh UUID.
        String,
    ),
    /// Select all descendant meshes of a Part.
    Part(
        /// Persistent Part UUID selecting descendant meshes.
        String,
    ),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Bounds of evaluated target vertices in model coordinates.
///
/// These geometric bounds can extend outside the canvas and do not account for
/// textures, masks, or occlusion.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObjectBounds {
    /// Resolved mesh IDs in authoring order, including meshes filtered from the bounds.
    pub mesh_ids: Vec<String>,
    /// `[min_x, min_y, max_x, max_y]`, or `None` when no vertices contribute.
    /// Evaluated Y-up model units, independent of source canvas or export padding.
    /// Geometric bounds only: textures, masks and occlusion are not applied.
    pub model_bounds: Option<[f64; 4]>,
    /// Why `model_bounds` is absent: `no_descendant_mesh`, `filtered_out`, or
    /// `no_evaluated_positions`.
    pub empty_reason: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// One evaluated triangle containing the query point.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct TriangleHit {
    /// The triangle's zero-based position in the drawable's index buffer.
    pub triangle_index: usize,
    /// Offsets into the evaluated drawable's vertex arrays.
    pub vertex_indices: [u32; 3],
    /// Stable authoring vertex IDs, when present.
    pub vertex_ids: Option<[u32; 3]>,
    /// Interpolation weights corresponding to `vertex_indices`.
    pub barycentric: [f64; 3],
    /// UV coordinate interpolated from the triangle's evaluated UVs.
    pub uv: Option<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// One mesh whose evaluated geometry contains the query point.
///
/// A hit is a geometric candidate, not proof that the mesh painted a visible
/// pixel. `part_path` and `part_names` run from root to leaf.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct GeometryHit {
    /// Stable authoring mesh ID.
    pub mesh_id: String,
    /// Authoring mesh display name.
    pub name: String,
    /// Ancestor Part IDs, from root to leaf.
    pub part_path: Vec<String>,
    /// Ancestor Part display names, parallel to `part_path`.
    pub part_names: Vec<String>,
    /// Evaluated drawable enabled state.
    pub enabled: bool,
    /// Evaluated drawable visibility state.
    pub visible: bool,
    /// Evaluated drawable opacity.
    pub opacity: f32,
    /// Evaluated draw order, used for stable descending candidate ordering.
    /// This is not a visibility ranking across offscreen composition groups.
    pub render_order: i32,
    /// Containing triangles; populated only when `details` was requested.
    pub triangles: Vec<TriangleHit>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Mesh candidates for a model-space point, ordered by descending draw order.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct GeometryQuery {
    /// Candidates retained after applying `max_candidates`.
    pub hits: Vec<GeometryHit>,
    /// Number of candidates before applying the limit.
    pub total: usize,
    /// Whether any candidates were omitted by the limit.
    pub truncated: bool,
}

/// CPU geometric queries; masks, textures and occlusion are not pixel-tested.
#[derive(
    Clone, Debug, Serialize, Deserialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpatialQuery {
    Bounds {
        /// Mesh/Part UUID selectors; an empty selection selects no meshes.
        targets: Vec<ObjectTarget>,
        /// Include disabled, hidden and zero-opacity geometry in the union.
        #[serde(default)]
        include_hidden: bool,
    },
    Hits {
        /// Finite XY point in evaluated Y-up model coordinates, not screen pixels.
        point: [f64; 2],
        /// Include disabled, hidden and zero-opacity geometric candidates.
        #[serde(default)]
        include_hidden: bool,
        /// Include every containing triangle, stable vertex IDs, barycentric weights and UV.
        #[serde(default)]
        details: bool,
        /// Maximum mesh candidates retained in descending draw order; total remains untruncated.
        #[serde(default = "default_candidates")]
        max_candidates: usize,
    },
}
fn default_candidates() -> usize {
    32
}

#[derive(Clone, Debug, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SpatialResult {
    Bounds(
        /// Bounds and resolved mesh IDs for the corresponding selector request.
        ObjectBounds,
    ),
    Hits(
        /// Geometric candidates in SDK order, including total and truncation metadata.
        GeometryQuery,
    ),
}

#[derive(Clone, Debug, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct SpatialFrame {
    /// Effective evaluated parameter values keyed by document UUID, including defaults/clamps.
    pub parameters: crate::evaluation::PreviewValues,
    /// Query results in input order, including repeated queries and empty results.
    pub results: Vec<SpatialResult>,
}

#[derive(Clone, Debug, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct SpatialSamples {
    /// Exact SDK version of the immutable formal or candidate snapshot.
    pub version: crate::authoring::Version,
    /// Owned requested parameter maps, preserving pose order and duplicates.
    pub requested: Vec<crate::evaluation::PreviewValues>,
    /// Owned geometric queries shared by every returned frame.
    pub queries: Vec<SpatialQuery>,
    /// Effective poses and query results, aligned with requested parameter maps.
    pub frames: Vec<SpatialFrame>,
}
