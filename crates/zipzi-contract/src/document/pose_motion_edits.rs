#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PoseCollection {
    Groups,
    Entries {
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: String,
    },
    Links {
        /// Stable ID of the owning entry; links remain owned by that entry.
        entry_id: String,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PoseFields {
    Asset {
        /// Optional fade-in duration in seconds; None retains the format/runtime default.
        fade_in: Option<f32>,
    },
    Group {
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    Entry {
        /// Typed resolved/unresolved Part reference retained by the Pose entry or link.
        part: PosePartRef,
    },
    Link {
        /// Typed target reference; unresolved imported runtime labels remain explicit.
        target: PosePartRef,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PoseEdit {
    Create {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    /// Also creates the singleton when absent, in the same transaction.
    CreateGroup {
        /// Stable Pose singleton ID; the operation may create it when absent.
        pose_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    CreateEntry {
        /// Stable ID of the group owning the affected rows or node chain.
        group_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    CreateLink {
        /// Stable ID of the owning entry; links remain owned by that entry.
        entry_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    Patch {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: PoseFields,
        /// Typed replacement fields for the addressed object, applied atomically.
        fields: PoseFields,
    },
    Move {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: PoseCollection,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    Duplicate {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: PoseCollection,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    Delete {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// A Pose group has no root member container; a receiving group is required.
    DissolveGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionContainer {
    Root,
    Group(
        /// Destination motion-group identity; Root selects ungrouped registrations.
        String,
    ),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionEdit {
    Create {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Stable ID allocated for the new motion library registration.
        registration_id: String,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
    },
    CreateGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    RenameGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    ReorderGroups {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    Register {
        /// Complete replacement motion registration; its owner is selected separately.
        entry: MotionRegistration,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    PatchRegistration {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: MotionRegistration,
        /// Complete replacement motion registration; its owner is selected separately.
        entry: MotionRegistration,
    },
    MoveRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DuplicateRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DeleteRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    DissolveGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    DeleteGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    DuplicateGroup {
        /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    DuplicateClip {
        /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Stable ID allocated for the new motion library registration.
        registration_id: String,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
    },
    DeleteClip {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    ReorderTracks {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    CreateTrack {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Stable identity allocated for the initial key of a new track.
        key_id: String,
        /// Absolute time in seconds within the owning motion clip.
        time: f32,
    },
    SetTrackTarget {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Typed target reference; unresolved imported runtime labels remain explicit.
        target: MotionTrackTarget,
    },
    MoveTracks {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
        /// True allocates copied identities; false transfers the existing identities.
        copy: bool,
    },
    DeleteTracks {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    TransferKeys {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable receiving track ID for transferred keys.
        destination_track: String,
        /// Offset in seconds added when moving or copying keys/events.
        time_offset: f32,
        /// True allocates copied identities; false transfers the existing identities.
        copy: bool,
    },
    SetEvent {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Owned motion event with a stable ID, time in seconds and string payload.
        event: MotionEvent,
    },
    DeleteEvents {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    MoveEvents {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Offset in seconds added when moving or copying keys/events.
        time_offset: f32,
        /// True allocates copied identities; false transfers the existing identities.
        copy: bool,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct ExpressionEntryFields {
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: ExpressionTarget,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: f32,
    /// Optional expression blend mode; None uses the native Add default.
    pub blend: Option<ExpressionBlend>,
}
impl From<&ExpressionEntry> for ExpressionEntryFields {
    fn from(e: &ExpressionEntry) -> Self {
        Self {
            target: e.target.clone(),
            value: e.value,
            blend: e.blend,
        }
    }
}

/// All positions use stable entry IDs; None appends to the destination.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ExpressionEdit {
    Create {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    CreateEntry {
        /// Stable expression asset ID owning the selected entries.
        owner: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    PatchEntry {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: ExpressionEntryFields,
        /// Typed replacement fields for the addressed object, applied atomically.
        fields: ExpressionEntryFields,
    },
    MoveEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DuplicateEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// New stable identities paired with the selected source objects.
        new_ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DeleteEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    Duplicate {
        /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// New stable entry IDs paired with the source expression entries.
        entry_ids: Vec<String>,
    },
    Delete {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    Reorder {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
}
