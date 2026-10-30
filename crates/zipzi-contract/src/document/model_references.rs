#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceCollection {
    ModelParameterGroups,
    ModelHitAreas,
    CdiCombinedSets,
    ModelParameterMembers {
        /// Existing model3 parameter-group object ID; this collection has no root/unassigned group.
        group_id: String,
    },
    CdiCombinedMembers {
        /// Stable CDI combined-parameter set ID owning the selected members.
        set_id: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceSelectionField {
    MeshMasks,
    OffscreenMasks,
    BlendConstraints,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceFields {
    ModelParameterGroup {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    ModelHitArea {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Resolved mesh target or preserved unresolved mesh runtime identifier.
        mesh: ModelTargetRef,
    },
    ModelParameterMember {
        /// Resolved document target or preserved unresolved runtime identifier; entry identity
        /// remains independent of the target.
        target: ModelTargetRef,
    },
    CdiCombinedMember {
        /// Resolved document target or preserved unresolved runtime identifier; entry identity
        /// remains independent of the target.
        target: CdiParameterRef,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceEdit {
    CreateModelParameterGroup {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreateModelHitArea {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreateCdiCombinedSet {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreateMember {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: ReferenceCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
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
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
    },
    Dissolve {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
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
