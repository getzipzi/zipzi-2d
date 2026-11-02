#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select an organization, deformation, Glue or drawing collection.
pub enum SceneCollection {
    /// Select members of a Part or the root organization.
    Organization {
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
    },
    /// Select children of a deformation parent or the root.
    Deformation {
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
    },
    /// Select the Glue collection.
    Glues,
    /// Select drawing groups.
    DrawingGroups,
    /// Select members of one drawing group.
    Drawing {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select whether scene duplication or deletion includes deformation descendants.
pub enum SceneCopyScope {
    #[default]
    /// Operate on selected objects only.
    Selected,
    /// Include descendants in the deformation hierarchy.
    DeformationSubtree,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select the kind and options for creating a scene object.
pub enum SceneCreate {
    /// Create a mesh with an explicit texture reference.
    Mesh {
        #[serde(default)]
        /// Image-asset reference; resolved values use asset IDs and unresolved values retain
        /// external texture identifiers.
        texture: SceneReference,
    },
    /// Create a warp transform with optional parent-space bounds.
    Warp {
        #[serde(default)]
        /// Optional [left, bottom, right, top] warp bounds in parent coordinates; None requests
        /// native defaults.
        bounds: Option<[f32; 4]>,
    },
    /// Create a rotation transform with optional parent-space origin.
    Rotation {
        #[serde(default)]
        /// Optional rotation origin in parent coordinates. None uses (0, 0) for an empty
        /// deformer, or the current content center when grouping a selection.
        origin: Option<PreciseVec2>,
    },
    /// Create an Offscreen group.
    Offscreen,
    /// Create a Glue connection with explicit mesh references.
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
/// Identifies a typed scene property for reads or field-address operations.
pub enum SceneField {
    /// Display label.
    Label,
    /// External runtime identifier.
    RuntimeId,
    /// Evaluation enabled flag.
    Enabled,
    /// Opacity and color factors.
    Appearance,
    /// Image-asset reference.
    Texture,
    /// Clipping-mask mesh references.
    Masks,
    /// Mesh drawing configuration.
    MeshDrawing,
    /// Warp transform definition.
    Warp,
    /// Rotation transform definition.
    Rotation,
    /// Glue endpoint mesh references.
    GlueEndpoints,
    /// Glue influence factor.
    GlueIntensity,
    /// Offscreen compositing and keyform mapping.
    Offscreen,
    /// Organization and deformation parent identities.
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
/// Typed scene property value used for exact-precondition patches.
pub enum SceneFieldValue {
    /// Select a display-label value.
    Label(#[doc = "Replacement display label."] String),
    /// Select an external runtime identifier.
    RuntimeId(
        /// Replacement external runtime identifier; an empty string explicitly clears it.
        String,
    ),
    /// Select an evaluation enabled value.
    Enabled(#[doc = "Replacement model evaluation enabled flag."] bool),
    /// Select opacity and color factors.
    Appearance(#[doc = "Replacement opacity and RGB color factors."] Appearance),
    /// Select an image-asset reference.
    Texture(
        /// Resolved image-asset reference or explicitly unresolved texture runtime identifier.
        SceneReference,
    ),
    /// Select the complete clipping-mask mesh list.
    Masks(#[doc = "Complete ordered clipping-mask mesh selection."] Vec<String>),
    /// Select mesh drawing configuration.
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
    /// Select a complete Warp definition.
    Warp(#[doc = "Complete replacement warp lattice and controller definition."] WarpTransform),
    /// Select a complete Rotation definition.
    Rotation(#[doc = "Complete replacement rotation definition."] RotationTransform),
    /// Select both Glue endpoint references.
    GlueEndpoints {
        /// Mesh A endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        a: SceneReference,
        /// Mesh B endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        b: SceneReference,
    },
    /// Select a Glue influence factor.
    GlueIntensity(#[doc = "Replacement Glue influence factor."] f32),
    /// Select Offscreen compositing and mapping data.
    Offscreen {
        /// Compositing mode used when drawing this object.
        blend_mode: u32,
        /// Preserved format-specific Offscreen compositing flags.
        flags: u8,
        /// Ordered Offscreen appearance forms; Part mapping entries select their array indices.
        keyforms: Vec<OffscreenKeyform>,
        /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
        part_keyform_indices: Vec<i32>,
    },
    /// Select organization and deformation context.
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
/// Typed scene creation, hierarchy, property and drawing-group edits.
pub enum SceneEdit {
    /// Create a typed scene object at explicit organization and deformation positions.
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
    /// Replace one typed scene field if its current value matches.
    Patch {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: SceneFieldValue,
        /// Replacement field value; its variant must match expected.
        value: SceneFieldValue,
        #[serde(default)]
        /// Explicitly clear existing Glue vertex pairs when changing Glue endpoints.
        clear_glue_pairs: bool,
    },
    /// Move objects to the selected collection.
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
    /// Reparent deformation objects under an explicit preservation strategy.
    Reparent {
        /// Reparent request with its source selection and preservation strategy.
        edit: ReparentEdit,
    },
    /// Duplicate selected scene objects under an explicit scope.
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
    /// Delete scene objects under an explicit scope.
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        #[serde(default)]
        /// Native copy/delete scope specifying whether dependent deformation descendants are
        /// included.
        scope: SceneCopyScope,
    },
    /// Transfer an Offscreen group to another Part with its mapping.
    TransferOffscreen {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Destination Part object ID; it must name an existing Part.
        part_id: String,
        /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
        part_keyform_indices: Vec<i32>,
    },
    /// Create or retain a drawing group under the specified parent.
    EnsureDrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Drawing-group parent owner; an empty string selects the root drawing group.
        parent: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Replace a drawing group range under an exact precondition.
    SetDrawingRange {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: [i32; 2],
        /// Inclusive lower and upper drawing-order bounds, in that order.
        range: [i32; 2],
    },
    /// Remove a drawing group while retaining its members.
    DissolveDrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
    /// Copy drawing-group structure from a source owner to a target owner.
    DuplicateDrawingGroup {
        /// Source drawing-group owner ID; an empty string selects the root group.
        source: String,
        /// Existing target Part owner ID; it must differ from the source.
        target: String,
    },
}
