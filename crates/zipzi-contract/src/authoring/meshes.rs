crate::api_record! {
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
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

crate::api_record! {
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
pub struct ImageCreateSpec {
    /// Optional new object UUID; None allocates a fresh UUID in the SDK when creation executes.
    pub id: Option<String>,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
}
}

#[derive(
    Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape,
)]
pub struct GeometryBounds {
    /// Minimum XY corner of the geometry bounds in the snapshot coordinate space.
    pub min: Vec2,
    /// Maximum XY corner of the geometry bounds in the snapshot coordinate space.
    pub max: Vec2,
}

#[derive(
    Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape,
)]
#[serde(default)]
pub struct GeometryChecks {
    /// Area in authored coordinates: model units squared at the root, or
    /// the appropriate rotation-local / Warp-parameter units under a parent.
    pub min_triangle_area: f64,
    /// Optional expected model bounds for root meshes only.
    pub model_bounds: Option<GeometryBounds>,
    /// Include per-pose mesh quality in evaluated Y-up model space, relative to this
    /// snapshot evaluated with default parameters. Only the requested poses are checked.
    pub quality: bool,
}

impl Default for GeometryChecks {
    fn default() -> Self {
        Self {
            min_triangle_area: 0.0,
            model_bounds: None,
            quality: false,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub enum GeometryDiagnosticKind {
    SmallTriangle,
    InconsistentWinding,
    OutsideModelBounds,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct GeometryDiagnostic {
    /// Typed geometry diagnostic category.
    pub kind: GeometryDiagnosticKind,
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Optional zero-based index of the offending triangle; None means the diagnostic is not
    /// triangle-specific.
    pub triangle_index: Option<usize>,
}

crate::api_union! {
    tag = "op";
    #[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub enum MeshOperation {
        Create {
            /// Creation specification; native admission supplies identities/defaults and
            /// rejects invalid input.
            spec: MeshCreateSpec,
        },
        Rename {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Required current display name; None disables this name precondition.
            expected_name: Option<String>,
        },
        SetPositions {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Stable vertex IDs selecting which positions to replace; values are paired with
            /// positions in the supplied order.
            vertex_ids: Vec<u32>,
            /// Replacement absolute mesh-local XY positions aligned with the supplied
            /// vertex_ids selection.
            positions: Vec<Vec2>,
        },
    }
}
