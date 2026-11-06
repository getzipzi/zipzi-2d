crate::api_record! {
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
/// Vertex IDs, authoring-space positions, texture UVs and triangle connectivity for a mesh.
/// Positions and UVs follow vertex_ids order; triangles contain vertex IDs.
/// Positions use model space without a deformation parent, otherwise the parent's
/// local/parameter space.
pub struct MeshGeometry {
    /// Stable vertex IDs in the same order as positions and UVs; IDs need not be contiguous.
    pub vertex_ids: Vec<VertexId>,
    /// Absolute XY coordinates in the mesh authoring space (model space without a deformation
    /// parent, otherwise the parent's local/parameter space), ordered by vertex_ids.
    pub positions: Vec<Vec2>,
    /// Texture UV coordinates ordered by vertex_ids.
    pub uvs: Vec<Vec2>,
    /// Triangle entries reference vertex_ids values, never offsets into positions.
    pub triangles: Vec<[VertexId; 3]>,
    /// Optional canonical UV boundary and retained internal structure edges.
    #[serde(default)]
    pub meshing: Option<MeshMeshing>,
}
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    /// Specification for creating a mesh. An omitted ID allocates a fresh UUID when
    /// creation executes; an empty texture_asset_id leaves the texture unassigned.
    pub struct MeshCreateSpec {
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// Owned vertex IDs, positions, UVs and triangles; triangles contain vertex IDs, not
        /// position offsets.
        pub geometry: MeshGeometry,
        #[serde(default)]
        /// Image-asset ID used by this mesh; the default empty string leaves its texture
        /// reference unassigned.
        pub texture_asset_id: String,
        /// Optional new mesh UUID; None allocates a fresh UUID in the SDK when the create
        /// operation executes.
        pub id: Option<String>,
    }
}
