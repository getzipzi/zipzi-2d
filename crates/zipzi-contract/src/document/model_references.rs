#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select a model3 or CDI reference metadata collection.
pub enum ReferenceCollection {
    /// Select model3 parameter groups.
    ModelParameterGroups,
    /// Select model3 hit areas.
    ModelHitAreas,
    /// Select CDI combined-parameter sets.
    CdiCombinedSets,
    /// Select members of one model3 parameter group.
    ModelParameterMembers {
        /// Existing model3 parameter-group object ID; this collection has no root/unassigned group.
        group_id: String,
    },
    /// Select members of one CDI combined-parameter set.
    CdiCombinedMembers {
        /// Stable CDI combined-parameter set ID owning the selected members.
        set_id: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select an ordered object-reference list for replacement.
pub enum ReferenceSelectionField {
    /// Select a mesh clipping-mask list.
    MeshMasks,
    /// Select an Offscreen clipping-mask list.
    OffscreenMasks,
    /// Select a BlendShape binding constraint list.
    BlendConstraints,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Typed metadata fields used to patch a matching reference entry.
pub enum ReferenceFields {
    /// Supply model3 parameter-group fields.
    ModelParameterGroup {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    /// Supply model3 hit-area fields and mesh target.
    ModelHitArea {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Resolved mesh target or preserved unresolved mesh runtime identifier.
        mesh: ModelTargetRef,
    },
    /// Supply a model3 parameter member target.
    ModelParameterMember {
        /// Resolved document target or preserved unresolved runtime identifier; entry identity
        /// remains independent of the target.
        target: ModelTargetRef,
    },
    /// Supply a CDI combined-parameter member target.
    CdiCombinedMember {
        /// Resolved document target or preserved unresolved runtime identifier; entry identity
        /// remains independent of the target.
        target: CdiParameterRef,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Create, patch, move or remove model3 and CDI reference metadata.
pub enum ReferenceEdit {
    /// Create a model3 parameter group.
    CreateModelParameterGroup {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Create a model3 hit area.
    CreateModelHitArea {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Create a CDI combined-parameter set.
    CreateCdiCombinedSet {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Create a reference member in the selected collection.
    CreateMember {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: ReferenceCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Replace matching metadata fields under an exact precondition.
    Patch {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: ReferenceFields,
        /// Typed replacement metadata fields; their variant must match the addressed reference
        /// kind.
        fields: ReferenceFields,
    },
    /// Move selected entries to a compatible collection.
    Move {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: ReferenceCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Duplicate selected entries into a compatible collection.
    Duplicate {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: ReferenceCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Delete selected reference entries.
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
    },
    /// Remove a reference container and move its members to a compatible destination.
    Dissolve {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Stable ID of a compatible destination group or combined set; it must differ from id.
        destination: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Replace an ordered target list under an exact precondition.
    ReplaceSelection {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Explicit field discriminator; must match the addressed object/domain.
        field: ReferenceSelectionField,
        /// Exact current ordered target-ID selection; a mismatch rejects the edit.
        expected: Vec<String>,
        /// Complete replacement ordered target-ID selection; an empty list explicitly clears it.
        targets: Vec<String>,
    },
}
