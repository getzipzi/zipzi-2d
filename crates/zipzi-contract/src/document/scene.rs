#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneCollection {
    Organization {
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
    },
    Deformation {
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
    },
    Glues,
    DrawingGroups,
    Drawing {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneCopyScope {
    #[default]
    Selected,
    DeformationSubtree,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneCreate {
    Mesh {
        #[serde(default)]
        /// Image-asset reference; resolved values use asset IDs and unresolved values retain
        /// external texture identifiers.
        texture: SceneReference,
    },
    Warp {
        #[serde(default)]
        /// Optional [left, bottom, right, top] warp bounds in parent coordinates; None requests
        /// native defaults.
        bounds: Option<[f32; 4]>,
    },
    Rotation,
    Offscreen,
    Glue {
        #[serde(default)]
        /// Mesh A endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        a: SceneReference,
        #[serde(default)]
        /// Mesh B endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        b: SceneReference,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape)]
pub enum SceneField {
    Label,
    RuntimeId,
    Enabled,
    Appearance,
    Texture,
    Masks,
    MeshDrawing,
    Warp,
    Rotation,
    GlueEndpoints,
    GlueIntensity,
    Offscreen,
    Context,
}
impl SceneField {
    pub fn is_reference(self) -> bool {
        matches!(
            self,
            Self::Texture | Self::Masks | Self::GlueEndpoints | Self::Context
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "field",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneFieldValue {
    Label(#[doc = "Replacement display label."] String),
    RuntimeId(
        /// Replacement external runtime identifier; an empty string explicitly clears it.
        String,
    ),
    Enabled(#[doc = "Replacement model evaluation enabled flag."] bool),
    Appearance(#[doc = "Replacement opacity and RGB color factors."] Appearance),
    Texture(
        /// Resolved image-asset reference or explicitly unresolved texture runtime identifier.
        SceneReference,
    ),
    Masks(#[doc = "Complete ordered clipping-mask mesh selection."] Vec<String>),
    MeshDrawing {
        /// Drawing-order coordinate; distinct from the final evaluated render_order. None
        /// leaves this channel unspecified for native evaluation.
        draw_order: Option<f32>,
        /// Compositing mode used when drawing this object.
        blend_mode: BlendMode,
        /// Original format-specific blend-mode code; None means no preserved raw code.
        raw_blend_mode: Option<u32>,
        /// Whether both triangle faces contribute to drawing.
        double_sided: bool,
        /// Whether to invert the combined clipping mask for this drawable.
        inverted_mask: bool,
    },
    Warp(#[doc = "Complete replacement warp lattice and controller definition."] WarpTransform),
    Rotation(#[doc = "Complete replacement rotation definition."] RotationTransform),
    GlueEndpoints {
        /// Mesh A endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        a: SceneReference,
        /// Mesh B endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        b: SceneReference,
    },
    GlueIntensity(#[doc = "Replacement Glue influence factor."] f32),
    Offscreen {
        /// Compositing mode used when drawing this object.
        blend_mode: u32,
        /// Preserved format-specific Offscreen compositing flags.
        flags: u8,
        /// Shape samples for this binding; coordinate/sample identities must match its axes or
        /// key table.
        keyforms: Vec<OffscreenKeyform>,
        /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
        part_keyform_indices: Vec<i32>,
    },
    Context {
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
    },
}
impl SceneFieldValue {
    /// Native reference payload used by publication summaries. Context exposes
    /// organization/deformation targets; topology changes are reported separately.
    pub fn into_references(self) -> Option<Vec<SceneReference>> {
        match self {
            Self::Texture(target) => Some(vec![target]),
            Self::Masks(targets) => Some(targets.into_iter().map(Into::into).collect()),
            Self::GlueEndpoints { a, b } => Some(vec![a, b]),
            Self::Context { part_id, parent_id } => Some(vec![
                part_id.unwrap_or_default().into(),
                parent_id.unwrap_or_default().into(),
            ]),
            _ => None,
        }
    }

    pub fn field(&self) -> SceneField {
        match self {
            Self::Label(_) => SceneField::Label,
            Self::RuntimeId(_) => SceneField::RuntimeId,
            Self::Enabled(_) => SceneField::Enabled,
            Self::Appearance(_) => SceneField::Appearance,
            Self::Texture(_) => SceneField::Texture,
            Self::Masks(_) => SceneField::Masks,
            Self::MeshDrawing { .. } => SceneField::MeshDrawing,
            Self::Warp(_) => SceneField::Warp,
            Self::Rotation(_) => SceneField::Rotation,
            Self::GlueEndpoints { .. } => SceneField::GlueEndpoints,
            Self::GlueIntensity(_) => SceneField::GlueIntensity,
            Self::Offscreen { .. } => SceneField::Offscreen,
            Self::Context { .. } => SceneField::Context,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneEdit {
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Native scene object kind and its creation options; not a persisted full object record.
        object: SceneCreate,
        #[serde(default)]
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
        #[serde(default)]
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
        /// Stable anchor in the organization collection (`part_id`), or the
        /// connection collection for Glue. None appends to that collection.
        #[serde(default)]
        before: Option<String>,
        /// Independent stable anchor in the deformation collection (`parent_id`).
        /// Only Mesh/Warp/Rotation support this position; None appends.
        #[serde(default)]
        deformation_before: Option<String>,
    },
    Patch {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: SceneFieldValue,
        /// Scalar value in the referenced parameter domain.
        value: SceneFieldValue,
        #[serde(default)]
        /// Explicitly clear existing Glue vertex pairs when changing Glue endpoints.
        clear_glue_pairs: bool,
    },
    Move {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: SceneCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Reparent {
        /// Edit candidate handle; None selects formal document state.
        edit: ReparentEdit,
    },
    Duplicate {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        #[serde(default)]
        /// Native copy/delete scope specifying whether dependent deformation descendants are
        /// included.
        scope: SceneCopyScope,
        #[serde(default)]
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: Option<OrganizationCollection>,
        #[serde(default)]
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        #[serde(default)]
        /// Native copy/delete scope specifying whether dependent deformation descendants are
        /// included.
        scope: SceneCopyScope,
    },
    TransferOffscreen {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Destination Part object ID; it must name an existing Part.
        part_id: String,
        /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
        part_keyform_indices: Vec<i32>,
    },
    EnsureDrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Drawing-group parent owner; an empty string selects the root drawing group.
        parent: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    SetDrawingRange {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: [i32; 2],
        /// Inclusive lower and upper drawing-order bounds, in that order.
        range: [i32; 2],
    },
    DissolveDrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
    DuplicateDrawingGroup {
        /// Exact formal or candidate source captured for this result; later host changes do not
        /// rewrite it.
        source: String,
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: String,
    },
}
