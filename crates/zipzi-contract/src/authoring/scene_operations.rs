crate::api_union! {
    tag = "op";
    #[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub enum SceneOperation {
        CreatePart {
            /// Creation specification; native admission supplies identities/defaults and
            /// rejects invalid input.
            spec: PartCreateSpec,
        },
        CreateParameterGroup {
            /// Creation specification; native admission supplies identities/defaults and
            /// rejects invalid input.
            spec: ParameterGroupCreateSpec,
        },
        CreateWarp {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Explicit organizational and deformation placement, with independent ordering
            /// anchors.
            placement: ScenePlacement,
            /// Optional [left, bottom, right, top] warp bounds in parent coordinates; None
            /// requests native defaults.
            bounds: Option<[f32; 4]>,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateRotation {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Explicit organizational and deformation placement, with independent ordering
            /// anchors.
            placement: ScenePlacement,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateGlue {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Mesh A endpoint of the Glue; resolved references use stable mesh IDs and
            /// unresolved references retain runtime identifiers.
            a: String,
            /// Mesh B endpoint of the Glue; resolved references use stable mesh IDs and
            /// unresolved references retain runtime identifiers.
            b: String,
            /// Organizational Part owner ID; independent of the deformation parent. None
            /// selects the root/unassigned container.
            part_id: Option<String>,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateOffscreen {
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Owning Part object ID; it must name an existing Part.
            part_id: String,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        MoveToPart {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Organizational Part owner ID; independent of the deformation parent. None
            /// selects the root/unassigned container.
            part_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        MoveDeformation {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parent object ID in this hierarchy; independent of storage and drawing order.
            /// None selects the root/unassigned container.
            parent_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        MoveParameters {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parameter-group ID in the document metadata hierarchy. None selects the
            /// root/unassigned container.
            group_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        MoveParameterGroups {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parent object ID in this hierarchy; independent of storage and drawing order.
            /// None selects the root/unassigned container.
            parent_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        DuplicateScene {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Include the deformation subtree when true; false affects only the selected objects.
            subtree: bool,
            /// Organizational Part owner ID; independent of the deformation parent. None
            /// selects the root/unassigned container.
            part_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        DeleteScene {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Include the deformation subtree when true; false affects only the selected objects.
            subtree: bool,
        },
        DeleteOrganization {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Preserve contained objects by dissolving their organization container instead of
            /// deleting its contents.
            dissolve: bool,
        },
        SetSceneLabel {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Exact current field value used as an optimistic precondition; stale values
            /// reject the edit.
            expected: String,
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
        },
        SetSceneEnabled {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Exact current field value used as an optimistic precondition; stale values
            /// reject the edit.
            expected: bool,
            /// Whether this object participates in evaluation; editor hiding is a separate state.
            enabled: bool,
        },
        ReparentLocal {
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
            /// Parent object ID in this hierarchy; independent of storage and drawing order.
            /// None selects the root/unassigned container.
            parent_id: Option<String>,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        ReplaceTriangles {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Triangle triples of stable vertex IDs; preserve winding and do not substitute
            /// position-array offsets.
            triangles: Vec<[u32; 3]>,
        },
        ReorderMeshVertices {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Complete new order of stable vertex IDs; preserves identities and associated
            /// per-vertex payload.
            order: Vec<u32>,
        },
        InsertMeshVertex {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable u32 vertex ID within the mesh, not an offset into its position array.
            vertex_id: u32,
            /// Absolute XY authoring position for this vertex, not a displacement.
            position: [f32; 2],
            /// Texture UV pair for this vertex; not a model-space position.
            uv: [f32; 2],
            /// Source vertex IDs and interpolation weights used to seed dependent shape content.
            weights: Vec<VertexWeightSpec>,
            /// Optional stable vertex ID before which to insert new vertices; None appends.
            /// This is not a position-array index.
            before: Option<u32>,
        },
        DeleteMeshVertices {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable vertex IDs in this mesh, not offsets into positions.
            vertices: Vec<u32>,
            /// Explicitly remove dependent topology/keyform/Glue content; false rejects
            /// referenced-vertex deletion.
            remove_dependents: bool,
        },
        CreateBlendTable {
            /// Stable document parameter ID, not its exported runtime identifier.
            parameter_id: String,
            /// Strictly increasing finite parameter samples with stable IDs; base_sample_id
            /// must name one of them.
            samples: Vec<ScalarSample>,
            /// Sample ID defining the neutral/base shape in this key table.
            base_sample_id: String,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        CreateMeshBlend {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable BlendShape key-table ID in this document.
            table_id: String,
            /// Optional new object UUID; None allocates a fresh identity in the SDK.
            id: Option<String>,
        },
        SetMeshBlendVertices {
            /// Stable ID of the target mesh in this document.
            mesh_id: String,
            /// Stable binding ID owning the addressed axes or keyforms.
            binding_id: String,
            /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
            /// parameter value.
            sample_id: String,
            /// Ordered content patches applied together by the native edit.
            updates: Vec<VertexPatchSpec>,
        },
        InsertGluePair {
            /// Stable Glue object ID owning the affected vertex pairs.
            glue_id: String,
            /// Complete Glue pair to insert or replace; endpoint IDs belong to the two
            /// connected meshes.
            pair: GluePairSpec,
            /// Optional sibling ID before which to insert; None appends. The anchor must belong
            /// to the selected destination.
            before: Option<String>,
        },
        DeleteGluePairs {
            /// Stable Glue object ID owning the affected vertex pairs.
            glue_id: String,
            /// Ordered stable object IDs selected for this operation; visible names and row
            /// numbers are not identities.
            ids: Vec<String>,
        },
        PatchOffscreenKeyform {
            /// Stable Offscreen object ID; not a texture asset ID.
            offscreen_id: String,
            /// Zero-based slot in the addressed triangle or Offscreen keyform array; never an
            /// object ID.
            slot: u32,
            /// Scalar value in the referenced parameter domain.
            value: OffscreenFormSpec,
        },
        InsertOffscreenKeyform {
            /// Stable Offscreen object ID; not a texture asset ID.
            offscreen_id: String,
            /// Scalar value in the referenced parameter domain.
            value: OffscreenFormSpec,
            /// Optional stable vertex ID before which to insert new vertices; None appends.
            /// This is not a position-array index.
            before: Option<u32>,
        },
        SetOffscreenStaticKeyform {
            /// Stable Offscreen object ID; not a texture asset ID.
            offscreen_id: String,
            /// Complete shape sample to insert or replace; unrelated samples are retained.
            keyform: i32,
        },
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct VertexWeightSpec {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        pub vertex_id: u32,
        /// Dimensionless interpolation weight applied to the referenced source vertex.
        pub weight: f32,
    }
}
