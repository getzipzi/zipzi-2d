/// Select explicit record IDs; empty lists return collection identities only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape)]
pub struct ModelingQuery {
    /// Explicit IDs of image assets to read, in result-slot order; an empty list requests no
    /// payloads, while collection IDs remain discoverable.
    pub assets: Vec<String>,
    /// Explicit IDs of meshes to read, in result-slot order; an empty list requests no
    /// payloads, while collection IDs remain discoverable.
    pub meshes: Vec<String>,
    /// Explicit IDs of parameters to read, in result-slot order; an empty list requests no
    /// payloads, while collection IDs remain discoverable.
    pub parameters: Vec<String>,
    /// Explicit IDs of Parts to read, in result-slot order; an empty list requests no payloads,
    /// while collection IDs remain discoverable.
    pub parts: Vec<String>,
    /// Explicit IDs of deformation transforms to read, in result-slot order; an empty list
    /// requests no payloads, while collection IDs remain discoverable.
    pub transforms: Vec<String>,
    /// Explicit IDs of ordinary mesh bindings to read, in result-slot order; an empty list
    /// requests no payloads, while collection IDs remain discoverable.
    pub bindings: Vec<String>,
    /// Explicit IDs of scene bindings to read, in result-slot order; an empty list requests no
    /// payloads, while collection IDs remain discoverable.
    pub scene_bindings: Vec<String>,
    /// Explicit IDs of BlendShape key tables to read, in result-slot order; an empty list
    /// requests no payloads, while collection IDs remain discoverable.
    pub blend_key_tables: Vec<String>,
    /// Explicit IDs of BlendShape constraints to read, in result-slot order; an empty list
    /// requests no payloads, while collection IDs remain discoverable.
    pub blend_constraints: Vec<String>,
    /// Explicit IDs of BlendShape bindings to read, in result-slot order; an empty list
    /// requests no payloads, while collection IDs remain discoverable.
    pub blend_bindings: Vec<String>,
    /// Explicit IDs of Glue objects to read, in result-slot order; an empty list requests no
    /// payloads, while collection IDs remain discoverable.
    pub glues: Vec<String>,
    /// Explicit IDs of Offscreen objects to read, in result-slot order; an empty list requests
    /// no payloads, while collection IDs remain discoverable.
    pub offscreens: Vec<String>,
    /// Explicit IDs of CDI parameter groups to read, in result-slot order; an empty list
    /// requests no payloads, while collection IDs remain discoverable.
    pub parameter_groups: Vec<String>,
    /// Explicit IDs of package attachments to read, in result-slot order; an empty list
    /// requests no payloads, while collection IDs remain discoverable.
    pub attachments: Vec<String>,
    /// Explicit motion clip IDs; payload slots follow order and retain missing IDs.
    pub motions: Vec<String>,
    /// Explicit motion registration-group IDs; group payloads include their ordered entries.
    pub motion_groups: Vec<String>,
    /// Explicit registration IDs, including root and grouped registrations.
    pub motion_registrations: Vec<String>,
    /// Explicit expression asset IDs; entries are owned by the selected asset.
    pub expressions: Vec<String>,
    /// Request the Physics singleton payload; false requests no Physics content.
    pub physics: bool,
    /// Request the Pose singleton payload; false requests no Pose content.
    pub pose: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum GeometrySpace {
    Model,
    RotationLocal(
        /// Stable rotation-transform ID defining the local position space.
        String,
    ),
    WarpParameter(
        /// Stable warp-transform ID defining the parameter/lattice position space.
        String,
    ),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct GeometrySnapshot {
    /// Exact SDK version captured for this value; not a best-effort latest-version marker.
    pub version: Version,
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Stable u32 vertex IDs in the same order as positions and UVs; IDs need not be contiguous.
    pub vertex_ids: Vec<VertexId>,
    /// Absolute XY vertices in the explicitly recorded GeometrySpace, aligned with vertex_ids.
    pub positions: Vec<Vec2>,
    /// Texture UV pairs ordered by vertex_ids; keep one UV for each position.
    pub uvs: Vec<Vec2>,
    /// Triangle triples of stable vertex IDs; preserve winding and do not substitute position-
    /// array offsets.
    pub triangles: Vec<[VertexId; 3]>,
    /// Optional canonical UV boundary and retained internal structure edges.
    #[serde(default)]
    pub meshing: Option<crate::model::MeshMeshing>,
    /// Coordinate space of positions; local-space variants carry the controlling transform ID.
    pub space: GeometrySpace,
}
