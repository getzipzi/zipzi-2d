#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ShapeTarget {
    #[default]
    Base,
    Ordinary {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable sample IDs selecting one ordinary-binding combination, in active axis order.
        samples: Vec<String>,
    },
    Blend {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
    },
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct VertexContentPatch {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Replacement XY value: absolute for Base/Ordinary, displacement for BlendShape.
    /// None preserves the current value in the selected shape.
    pub position: Option<Vec2>,
    /// Optional texture UV pair; None preserves the current UV.
    pub uv: Option<Vec2>,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct VertexWeight {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Dimensionless interpolation weight applied to the referenced source vertex.
    pub weight: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum VertexSeed {
    Base,
    Interpolate {
        /// Source vertex IDs and interpolation weights used to seed dependent shape content.
        weights: Vec<VertexWeight>,
    },
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct NewMeshVertex {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Absolute XY authoring position for this vertex, not a displacement.
    pub position: Vec2,
    /// Texture UV pair for this vertex; not a model-space position.
    pub uv: Vec2,
    /// Explicit policy for initializing this new vertex in dependent ordinary/BlendShape keyforms.
    pub seed: VertexSeed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum VertexDeletePolicy {
    #[default]
    RejectReferences,
    RemoveDependentContent,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct WarpPointPatch {
    /// Zero-based lattice point row, in the inclusive range 0..rows.
    pub row: u32,
    /// Zero-based lattice point column, in the inclusive range 0..columns.
    pub column: u32,
    /// Replacement lattice XY value: absolute for Base/Ordinary, displacement for BlendShape.
    pub point: Vec2,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum OffscreenDeletePolicy {
    RejectReferences,
    Neutral,
    Remap {
        #[serde(deserialize_with = "crate::serde_maps::usize_map")]
        /// Deleted-keyform-slot to replacement-slot map; None selects neutral/unmapped for that
        /// slot.
        replacements: BTreeMap<usize, Option<usize>>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PartKeyformSlot {
    Static,
    Samples {
        /// Stable sample IDs selecting one ordinary-binding combination, in active axis order.
        samples: Vec<String>,
    },
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct OffscreenMappingPatch {
    /// Static Part slot or stable Part-binding sample combination to remap.
    pub slot: PartKeyformSlot,
    /// Offscreen keyform-array index; -1 explicitly selects the neutral/unmapped state.
    pub keyform: i32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneContentEdit {
    MeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        #[serde(default)]
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: ShapeTarget,
        /// Ordered content patches applied together by the native edit.
        updates: Vec<VertexContentPatch>,
    },
    InsertMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// New vertex records in insertion order; each has a stable vertex ID and explicit
        /// dependent-shape seed.
        vertices: Vec<NewMeshVertex>,
        /// Optional stable vertex ID before which to insert new vertices; None appends. This is
        /// not a position-array index.
        before: Option<VertexId>,
    },
    DeleteMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Stable vertex IDs in this mesh, not offsets into positions.
        vertices: Vec<VertexId>,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: VertexDeletePolicy,
    },
    ReorderMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Complete new order of stable vertex IDs; preserves identities and associated per-
        /// vertex payload.
        order: Vec<VertexId>,
    },
    InsertTriangle {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Triangle corners as three stable vertex IDs; not position-array indices.
        vertices: [VertexId; 3],
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    PatchTriangle {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Zero-based position within the addressed content array; not a stable object ID.
        slot: usize,
        /// Triangle corners as three stable vertex IDs; not position-array indices.
        vertices: [VertexId; 3],
    },
    DeleteTriangles {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Zero-based content-array positions selected by this operation.
        slots: Vec<usize>,
    },
    ReplaceTriangles {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Triangle triples of stable vertex IDs; preserve winding and do not substitute
        /// position-array offsets.
        triangles: Vec<[VertexId; 3]>,
    },
    WarpPoints {
        /// Stable deformation-transform ID in this document.
        transform_id: String,
        #[serde(default)]
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: ShapeTarget,
        /// Ordered content patches applied together by the native edit.
        updates: Vec<WarpPointPatch>,
        #[serde(default)]
        /// Explicitly remove the authoring Bezier controller when editing baked lattice points.
        clear_bezier: bool,
    },
    WarpBezier {
        /// Stable deformation-transform ID in this document.
        transform_id: String,
        #[serde(default)]
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: ShapeTarget,
        /// Complete authoring Bezier controller; its baked lattice is updated by the native edit.
        controller: Option<WarpBezier>,
    },
    ResizeWarp {
        /// Stable deformation-transform ID in this document.
        transform_id: String,
        /// Number of warp lattice cells vertically; a lattice has rows + 1 point rows.
        rows: u32,
        /// Number of warp lattice cells horizontally; a lattice has columns + 1 point columns.
        columns: u32,
        /// Whether to use the quad warp interpolation mode for the lattice.
        quad: bool,
    },
    InsertGluePair {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Complete Glue pair to insert or replace; endpoint IDs belong to the two connected
        /// meshes.
        pair: GlueVertexPair,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    PatchGluePair {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Complete Glue pair to insert or replace; endpoint IDs belong to the two connected
        /// meshes.
        pair: GlueVertexPair,
    },
    MoveGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    TransferGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: String,
        /// Ordered Glue vertex pairs with stable pair identities and endpoint vertex IDs.
        pairs: Vec<GlueVertexPair>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    DuplicateGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    DeleteGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
    },
    InsertOffscreenKeyform {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Scalar value in the referenced parameter domain.
        value: OffscreenKeyform,
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    PatchOffscreenKeyform {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Zero-based slot in the addressed triangle or Offscreen keyform array; never an
        /// object ID.
        slot: usize,
        /// Scalar value in the referenced parameter domain.
        value: OffscreenKeyform,
    },
    DuplicateOffscreenKeyforms {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Zero-based Offscreen keyform slots; native execution checks/remaps dependent Part
        /// mappings.
        slots: Vec<usize>,
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    ReorderOffscreenKeyforms {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Permutation of zero-based Offscreen keyform slots; native execution remaps
        /// references consistently.
        order: Vec<usize>,
    },
    DeleteOffscreenKeyforms {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Zero-based Offscreen keyform slots; native execution checks/remaps dependent Part
        /// mappings.
        slots: Vec<usize>,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: OffscreenDeletePolicy,
    },
    SetOffscreenMapping {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Ordered content patches applied together by the native edit.
        updates: Vec<OffscreenMappingPatch>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneContentDomain {
    MeshTopology,
    WarpTopology,
    GluePairs,
    OffscreenKeyforms,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct SceneContentReport {
    pub generated_pair_ids: BTreeMap<String, String>,
    pub removed_pair_ids: Vec<String>,
    pub removed_vertices: Vec<VertexId>,
    pub removed_triangle_slots: Vec<usize>,
    /// Original Offscreen keyform index -> new index, None for deleted forms.
    pub keyform_remap: Vec<Option<usize>>,
    pub resampled_shapes: usize,
    pub vertex_sources: Vec<NewMeshVertex>,
    pub losses: Vec<String>,
}
