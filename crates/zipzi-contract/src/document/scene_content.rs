#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select the base, an ordinary binding sample or a BlendShape sample for a content edit.
pub enum ShapeTarget {
    #[default]
    /// Edit the undeformed base shape.
    Base,
    /// Edit the ordinary shape selected by binding and sample identities.
    Ordinary {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable sample IDs selecting one ordinary-binding combination, in active axis order.
        samples: Vec<String>,
    },
    /// Edit the BlendShape displacement selected by binding and sample identities.
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
/// Optional updates to one stable mesh vertex in a selected shape.
pub struct VertexContentPatch {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Replacement XY value: absolute for Base/Ordinary, displacement for BlendShape.
    /// None preserves the current value in the selected shape.
    pub position: Option<Vec2>,
    /// Optional texture UV pair; None preserves the current UV. UV updates are permitted only
    /// for the Base shape, because UV coordinates are shared by all shapes.
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
/// Source vertex and interpolation weight used to initialize dependent shapes.
pub struct VertexWeight {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Finite nonnegative interpolation weight. The nonempty source weights must sum to 1
    /// within native tolerance.
    pub weight: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Policy for seeding a new vertex in dependent ordinary and BlendShape content.
pub enum VertexSeed {
    /// Use the new base position for ordinary shapes and zero displacement for BlendShapes.
    Base,
    /// Interpolate dependent content from existing vertices and supplied weights.
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
/// New stable mesh vertex, its base position and UV, and the policy for dependent shapes.
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
/// Select how deletion handles triangles and other content that reference a vertex.
pub enum VertexDeletePolicy {
    #[default]
    /// Reject deletion while dependent content refers to a selected vertex.
    RejectReferences,
    /// Remove dependent content as part of the same edit.
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
/// Replacement lattice point at a row and column in a selected Warp shape.
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
/// Choose how Part mappings are handled when Offscreen keyform slots are deleted.
pub enum OffscreenDeletePolicy {
    /// Reject removal of referenced keyform slots.
    RejectReferences,
    /// Replace references to removed slots with the neutral mapping.
    Neutral,
    /// Replace references according to the explicit old-to-new slot mapping.
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
/// Select the static Part slot or an ordinary binding sample combination for an Offscreen mapping.
pub enum PartKeyformSlot {
    /// Address the static Part slot.
    Static,
    /// Address one ordinary binding sample combination.
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
/// Assign an Offscreen keyform index to one static or sampled Part slot.
pub struct OffscreenMappingPatch {
    /// Static Part slot or stable Part-binding sample combination to remap.
    pub slot: PartKeyformSlot,
    /// Offscreen keyform-array index; -1 explicitly selects the neutral/unmapped state.
    pub keyform: i32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Typed edits to mesh, deformer, Glue and Offscreen shape content.
pub enum SceneContentEdit {
    /// Capture, restore or configure the stored original deformer shape.
    DeformerOriginal {
        /// Stable Warp or Rotation transform ID.
        transform_id: String,
        #[serde(default)]
        /// Current editable Base or stored Ordinary shape; BlendShape restore/update is unsupported.
        target: ShapeTarget,
        /// Explicit capture, restore, or first-key update policy.
        action: DeformerOriginalAction,
        #[serde(default)]
        /// Frozen parameter inputs used to evaluate current parent spaces in canvas coordinates.
        context: crate::evaluation::PreviewValues,
    },
    /// Patch selected stable mesh vertices in one shape.
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
    /// Insert mesh vertices and seed dependent shape content.
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
    /// Delete selected vertices with an explicit dependency policy.
    DeleteMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Stable vertex IDs in this mesh, not offsets into positions.
        vertices: Vec<VertexId>,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: VertexDeletePolicy,
    },
    /// Reorder mesh vertex storage and associated shape content.
    ReorderMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Complete new order of stable vertex IDs; preserves identities and associated per-
        /// vertex payload.
        order: Vec<VertexId>,
    },
    /// Insert a triangle of stable vertex IDs.
    InsertTriangle {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Triangle corners as three stable vertex IDs; not position-array indices.
        vertices: [VertexId; 3],
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    /// Replace the triangle at a numeric slot.
    PatchTriangle {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Zero-based position within the addressed content array; not a stable object ID.
        slot: usize,
        /// Triangle corners as three stable vertex IDs; not position-array indices.
        vertices: [VertexId; 3],
    },
    /// Delete triangles selected by numeric slots.
    DeleteTriangles {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Zero-based content-array positions selected by this operation.
        slots: Vec<usize>,
    },
    /// Replace the complete mesh triangle list.
    ReplaceTriangles {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Triangle triples of stable vertex IDs; preserve winding and do not substitute
        /// position-array offsets.
        triangles: Vec<[VertexId; 3]>,
    },
    /// Patch lattice points in one Warp shape.
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
        /// False preserves the controller and retains lattice detail for subsequent delta edits.
        clear_bezier: bool,
    },
    /// Replace or remove a Warp controller and update its baked lattice.
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
    /// Edit selected controls on a stored Warp controller.
    WarpBezierControls {
        /// Stable Warp transform ID.
        transform_id: String,
        #[serde(default)]
        /// Explicit base or ordinary stored shape; BlendShapes have no independent controller.
        target: ShapeTarget,
        /// Explicit controller destinations; the stored editing mode supplies neighborhood influence.
        updates: Vec<crate::warp_bezier::WarpBezierControlPatch>,
    },
    /// Update Warp editing settings and optionally refit controllers.
    WarpAuthoring {
        /// Stable Warp transform ID.
        transform_id: String,
        /// Object editing settings. An explicit grid refits all stored controllers without changing baked points.
        settings: crate::warp_bezier::WarpAuthoringSettings,
    },
    /// Reset or reform one stored Warp shape.
    WarpShape {
        /// Stable Warp transform ID.
        transform_id: String,
        #[serde(default)]
        /// Explicit base or ordinary stored shape; other stored shapes remain unchanged.
        target: ShapeTarget,
        /// Reset preserves baked points; Reform samples the controller and replaces baked detail.
        action: crate::warp_bezier::WarpShapeAction,
    },
    /// Replace a static Warp while compensating child shapes.
    WarpKeepChildren {
        /// Stable ID of a static Warp. Driven Warp maps are explicitly unsupported.
        transform_id: String,
        /// Complete replacement map/controller at the existing conversion density.
        /// The shared plan compensates all child shape channels atomically.
        warp: crate::model::WarpTransform,
    },
    /// Replace a static Rotation while compensating child shapes.
    RotationKeepChildren {
        /// Stable ID of a static Rotation. Driven maps and ancestors are unsupported.
        transform_id: String,
        /// Complete replacement rotation; the shared plan compensates every
        /// stored child shape channel atomically while retaining the hierarchy.
        rotation: crate::model::RotationTransform,
    },
    /// Change the Warp lattice conversion density.
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
    /// Insert a Glue pair with a stable identity.
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
    /// Replace an existing Glue pair.
    PatchGluePair {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Complete Glue pair to insert or replace; endpoint IDs belong to the two connected
        /// meshes.
        pair: GlueVertexPair,
    },
    /// Reorder selected Glue pairs.
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
    /// Move Glue pairs to another Glue with explicit replacement records.
    TransferGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Stable destination Glue object ID.
        destination: String,
        /// Ordered Glue vertex pairs with stable pair identities and endpoint vertex IDs.
        pairs: Vec<GlueVertexPair>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Duplicate selected Glue pairs.
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
    /// Delete selected Glue pairs.
    DeleteGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
    },
    /// Insert an Offscreen appearance form at a slot or append.
    InsertOffscreenKeyform {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Complete Offscreen opacity and color form.
        value: OffscreenKeyform,
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    /// Replace the Offscreen form at a slot.
    PatchOffscreenKeyform {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Zero-based slot in the Offscreen keyform array, not an object ID.
        slot: usize,
        /// Complete Offscreen opacity and color form.
        value: OffscreenKeyform,
    },
    /// Duplicate selected Offscreen keyform slots.
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
    /// Reorder Offscreen forms and remap their references.
    ReorderOffscreenKeyforms {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Permutation of zero-based Offscreen keyform slots; native execution remaps
        /// references consistently.
        order: Vec<usize>,
    },
    /// Delete Offscreen forms using an explicit mapping policy.
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
    /// Set static or sampled Part-to-Offscreen mappings.
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
    /// Maximum measured preservation deviation in canvas units; independent samples,
    /// not a global error bound. None for edits without preservation verification.
    pub preservation_error: Option<f64>,
    pub preservation_samples: usize,
}
