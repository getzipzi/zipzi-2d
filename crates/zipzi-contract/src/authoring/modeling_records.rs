#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct TopologyReplacement {
    /// Complete replacement mesh geometry and metadata. Setting meshing to None explicitly
    /// discards previous modeling topology; ordinary mesh replacements must preserve it.
    pub mesh: Mesh,
    /// Optional replacement ordinary mesh binding; None supplies no ordinary binding.
    pub binding: Option<MeshBinding>,
    /// Complete replacement BlendShape bindings for the remeshed object.
    pub blend_bindings: Vec<BlendShapeBinding>,
    /// Complete replacement Glue records affected by the remesh.
    pub glues: Vec<Glue>,
    #[serde(deserialize_with = "crate::serde_maps::u32_map")]
    /// Old vertex ID to replacement ID map; Some(new_id) remaps an identity and None explicitly
    /// removes it.
    pub vertex_mapping: HashMap<VertexId, Option<VertexId>>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PhysicsCollection {
    Groups,
    Inputs {
        /// Stable Physics rig ID owning the selected rows.
        group_id: String,
    },
    Outputs {
        /// Stable Physics rig ID owning the selected rows.
        group_id: String,
    },
    Nodes {
        /// Stable Physics rig ID owning the selected rows.
        group_id: String,
    },
}

/// Owned selected query results. No mutation or query behavior lives in this DTO.
#[derive(Debug, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct OwnedModelingRecords {
    /// Stable document UUID; not a Runtime Session handle.
    pub document_id: String,
    /// Model source-canvas and export-frame configuration.
    pub setup: crate::model::ModelSetup,
    /// Independent organization/deformation palette ranks; does not alter model relationships.
    pub hierarchy_order: crate::document::HierarchyOrder,
    /// Owned authoring visibility/lock metadata; separate from evaluated model appearance.
    pub editor_state: crate::document::EditorState,
    /// Owned CDI display metadata, including explicit imported unresolved references.
    pub display_info: crate::document::DisplayInfo,
    /// Owned model3 export metadata and reference registrations.
    pub model3_settings: crate::document::Model3Settings,
    /// Optional drawing-order groups; None leaves grouping absent instead of supplying an empty
    /// group list.
    pub draw_order_groups: Option<Vec<crate::document::DrawOrderGroup>>,
    /// Domain-name-to-ID directory of document collections; identities remain available even
    /// when no record payloads were requested.
    pub ids: std::collections::BTreeMap<String, Vec<String>>,
    /// Owned image assets aligned with the corresponding query ID list; None marks a missing
    /// requested ID and an empty query returns no payloads.
    pub assets: Vec<Option<crate::model::ImageAsset>>,
    /// Owned meshes aligned with the corresponding query ID list; None marks a missing
    /// requested ID and an empty query returns no payloads.
    pub meshes: Vec<Option<crate::model::Mesh>>,
    /// Owned parameters aligned with the corresponding query ID list; None marks a missing
    /// requested ID and an empty query returns no payloads.
    pub parameters: Vec<Option<crate::model::Parameter>>,
    /// Owned Parts aligned with the corresponding query ID list; None marks a missing requested
    /// ID and an empty query returns no payloads.
    pub parts: Vec<Option<crate::model::Part>>,
    /// Owned deformation transforms aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub transforms: Vec<Option<crate::model::Transform>>,
    /// Owned ordinary mesh bindings aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub bindings: Vec<Option<crate::model::MeshBinding>>,
    /// Owned scene bindings aligned with the corresponding query ID list; None marks a missing
    /// requested ID and an empty query returns no payloads.
    pub scene_bindings: Vec<Option<crate::model::SceneBinding>>,
    /// Owned BlendShape key tables aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub blend_key_tables: Vec<Option<crate::model::BlendShapeKeyTable>>,
    /// Owned BlendShape constraints aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub blend_constraints: Vec<Option<crate::model::BlendShapeConstraint>>,
    /// Owned BlendShape bindings aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub blend_bindings: Vec<Option<crate::model::BlendShapeBinding>>,
    /// Owned Glue objects aligned with the corresponding query ID list; None marks a missing
    /// requested ID and an empty query returns no payloads.
    pub glues: Vec<Option<crate::model::Glue>>,
    /// Owned Offscreen objects aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub offscreens: Vec<Option<crate::model::Offscreen>>,
    /// Owned CDI parameter groups aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub parameter_groups: Vec<Option<crate::document::CdiParameterGroup>>,
    /// Owned package attachments aligned with the corresponding query ID list; None marks a
    /// missing requested ID and an empty query returns no payloads.
    pub attachments: Vec<Option<crate::document::PackageAttachment>>,
    /// Owned selected motions aligned with query IDs; None marks a missing ID.
    pub motions: Vec<Option<crate::document::MotionClip>>,
    /// Owned selected motion_groups aligned with query IDs; None marks a missing ID.
    pub motion_groups: Vec<Option<crate::document::MotionGroup>>,
    /// Owned selected motion_registrations aligned with query IDs; None marks a missing ID.
    pub motion_registrations: Vec<Option<crate::document::MotionRegistration>>,
    /// Owned selected expressions aligned with query IDs; None marks a missing ID.
    pub expressions: Vec<Option<crate::document::ExpressionAsset>>,
    /// Owned Physics singleton when requested and present; None otherwise.
    pub physics: Option<crate::physics_document::PhysicsAsset>,
    /// Owned Pose singleton when requested and present; None otherwise.
    pub pose: Option<crate::document::PoseAsset>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct ManualMeshTransferReport {
    /// Number of newly allocated vertices.
    pub added: usize,
    /// Retained identities whose UV sampling location changed.
    pub moved: usize,
    /// Retired identities, including isolated vertices removed by edge deletion.
    pub removed: usize,
    /// Stored base, ordinary and BlendShape channels processed; no Cartesian expansion.
    pub shape_count: usize,
    /// Whether triangle connectivity changed.
    pub reconnected: bool,
    /// Whether a referenced Glue endpoint was resampled.
    pub glue_endpoints: bool,
    /// A Warp occurs anywhere in the actual parent chain.
    pub non_affine_parent: bool,
    /// Dependency-free affine extension outside the previous UV coverage.
    pub affine_extension: bool,
    /// Reasons the plan does not promise identical appearance in every pose.
    pub appearance_limits: Vec<String>,
    /// Conservative result, workspace and history accounting.
    pub cost: crate::document::ConversionCost,
}
