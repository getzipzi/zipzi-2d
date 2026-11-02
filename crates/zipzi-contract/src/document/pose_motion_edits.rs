#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select a Pose group, entry or link collection.
pub enum PoseCollection {
    /// Select Pose groups.
    Groups,
    /// Select entries in one group.
    Entries {
        /// Stable ID of the Pose group whose entries are selected.
        group_id: String,
    },
    /// Select links in one entry.
    Links {
        /// Stable ID of the owning entry; links remain owned by that entry.
        entry_id: String,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Typed replacement fields for a Pose asset or owned row.
pub enum PoseFields {
    /// Replace Pose asset fade settings.
    Asset {
        /// Optional fade-in duration in seconds; None retains the format/runtime default.
        fade_in: Option<f32>,
    },
    /// Replace a Pose group label.
    Group {
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    /// Replace an entry Part reference.
    Entry {
        /// Typed resolved/unresolved Part reference retained by the Pose entry or link.
        part: PosePartRef,
    },
    /// Replace a link Part target.
    Link {
        /// Typed target reference; unresolved imported runtime labels remain explicit.
        target: PosePartRef,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Create, move, patch or delete Pose assets and owned rows.
pub enum PoseEdit {
    /// Create the Pose singleton.
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
    /// Create an entry in a group.
    CreateEntry {
        /// Stable ID of the Pose group that will own the new entry.
        group_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Create a link owned by an entry.
    CreateLink {
        /// Stable ID of the owning entry; links remain owned by that entry.
        entry_id: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Replace matching fields under an exact precondition.
    Patch {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: PoseFields,
        /// Typed replacement fields for the addressed object, applied atomically.
        fields: PoseFields,
    },
    /// Move selected owned identities to a compatible collection.
    Move {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: PoseCollection,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Duplicate selected owned identities into a compatible collection.
    Duplicate {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: PoseCollection,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Delete selected Pose identities.
    Delete {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// A Pose group has no root member container; a receiving group is required.
    DissolveGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Stable ID of the different Pose group receiving the entries.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select the ungrouped root or a motion registration group.
pub enum MotionContainer {
    /// Select ungrouped motion registrations.
    Root,
    /// Select registrations in one motion group.
    Group(
        /// Destination motion-group identity; Root selects ungrouped registrations.
        String,
    ),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Edit motion clips, registrations, tracks, keys and events independently of playback.
pub enum MotionEdit {
    /// Create a motion clip and its initial registration.
    Create {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Stable ID allocated for the new motion library registration.
        registration_id: String,
        /// Container for the initial registration.
        destination: MotionContainer,
    },
    /// Create a motion registration group.
    CreateGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    /// Rename a motion group.
    RenameGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    /// Reorder motion groups.
    ReorderGroups {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// Register an existing clip in the selected container.
    Register {
        /// Complete replacement motion registration; its owner is selected separately.
        entry: MotionRegistration,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Replace a registration under an exact precondition.
    PatchRegistration {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: MotionRegistration,
        /// Complete replacement motion registration; its owner is selected separately.
        entry: MotionRegistration,
    },
    /// Move registrations to another container.
    MoveRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Duplicate registrations into a container.
    DuplicateRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Delete selected registrations subject to clip retention rules.
    DeleteRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// Move group registrations to the ungrouped root and remove the group.
    DissolveGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    /// Remove a group and its registrations while retaining required last clip registrations.
    DeleteGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    /// Duplicate a group and its registrations under new identities.
    DuplicateGroup {
        /// Stable ID of the motion group to duplicate.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    /// Duplicate a clip and create its registration.
    DuplicateClip {
        /// Stable ID of the motion clip to duplicate.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Stable ID allocated for the new motion library registration.
        registration_id: String,
        /// Container for the new clip registration.
        destination: MotionContainer,
    },
    /// Delete a clip and its registrations.
    DeleteClip {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    /// Change display order of tracks in a clip.
    ReorderTracks {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// Create a track with a persistent initial key.
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
    /// Set the target channel of an existing track.
    SetTrackTarget {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Typed target reference; unresolved imported runtime labels remain explicit.
        target: MotionTrackTarget,
    },
    /// Move or copy tracks to another clip.
    MoveTracks {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable ID of the clip receiving the tracks.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
        /// True allocates copied identities; false transfers the existing identities.
        copy: bool,
    },
    /// Delete selected motion tracks.
    DeleteTracks {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// Move or copy stable keys to a track with a time offset.
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
    /// Insert or replace a timed event in a clip.
    SetEvent {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Owned motion event with a stable ID, time in seconds and string payload.
        event: MotionEvent,
    },
    /// Delete selected motion events.
    DeleteEvents {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// Move or copy timed events with a time offset.
    MoveEvents {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable ID of the clip receiving the selected events.
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
/// Typed target, value and blend replacement for an expression entry.
pub struct ExpressionEntryFields {
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: ExpressionTarget,
    /// Parameter contribution applied according to the selected blend mode.
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
    /// Create an expression asset.
    Create {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    /// Create an entry in an expression.
    CreateEntry {
        /// Stable expression asset ID owning the selected entries.
        owner: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Replace an entry under an exact precondition.
    PatchEntry {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: ExpressionEntryFields,
        /// Typed replacement fields for the addressed object, applied atomically.
        fields: ExpressionEntryFields,
    },
    /// Move entries to another expression.
    MoveEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable ID of the expression receiving the entries.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Copy entries using explicitly paired new identities.
    DuplicateEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// New stable identities paired with the selected source objects.
        new_ids: Vec<String>,
        /// Stable ID of the expression receiving the copied entries.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    /// Delete selected expression entries.
    DeleteEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    /// Duplicate an expression and its entries under new identities.
    Duplicate {
        /// Stable ID of the expression to duplicate.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// New stable entry IDs paired with the source expression entries.
        entry_ids: Vec<String>,
    },
    /// Delete an expression asset.
    Delete {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    /// Reorder expression assets.
    Reorder {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
}
