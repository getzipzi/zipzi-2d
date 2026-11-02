#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Editor visibility and lock flags for one document object.
pub struct ObjectEditorState {
    #[serde(default, skip_serializing_if = "is_false")]
    /// Editor-only hiding flag; does not replace the model enabled/visibility state.
    pub hidden_in_editor: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    /// Authoring lock state; native edit admission and capabilities enforce the lock.
    pub locked: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Explicit per-object Editor visibility and lock metadata.
pub struct EditorState {
    /// Explicit flags only, keyed by globally unique object ID. Defaults are omitted.
    #[serde(default)]
    pub objects: BTreeMap<String, ObjectEditorState>,
}
impl EditorState {
    pub fn hidden_objects(&self) -> impl Iterator<Item = &str> {
        self.objects
            .iter()
            .filter(|(_, state)| state.hidden_in_editor)
            .map(|(id, _)| id.as_str())
    }
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
    pub fn get(&self, id: &str) -> ObjectEditorState {
        self.objects.get(id).copied().unwrap_or_default()
    }
}

crate::api_record! {
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryOutput)]
/// Estimated work and storage for a planned conversion; estimates are not an admission guarantee.
pub struct ConversionCost {
    /// Number of shape-sample combinations the conversion must process.
    pub sample_combinations: usize,
    /// Estimated owned result payload in bytes; not total process RSS.
    pub payload_bytes: usize,
    /// Estimated temporary working storage in bytes; not total process RSS.
    pub temporary_bytes: usize,
    /// Includes the SDK's complete previous checkpoint and new candidate.
    pub history_bytes: usize,
}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// How an expression entry contributes to its target parameter.
pub enum ExpressionBlend {
    /// Add the entry value as a parameter contribution.
    Add,
    /// Multiply using the entry value as a factor.
    Multiply,
    /// Contribute the entry value as an overwrite target.
    Overwrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Optional resolved or unresolved expression parameter target.
pub enum ExpressionTarget {
    /// No parameter target is specified.
    Unspecified,
    /// Target a stable document parameter.
    Resolved {
        /// Stable document parameter UUID; evaluation uses its declared range and default.
        parameter_id: String,
    },
    /// Preserve an unresolved external parameter runtime ID.
    Unresolved {
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// One stable expression parameter contribution with an optional blend mode.
pub struct ExpressionEntry {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: ExpressionTarget,
    /// Parameter contribution applied according to the selected blend mode.
    pub value: f32,
    /// Missing means Framework's default Add blend.
    pub blend: Option<ExpressionBlend>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Optional resolved or unresolved Part target in Pose data.
pub enum PosePartRef {
    /// No Part target is specified.
    Unspecified,
    /// Target a stable document Part.
    Resolved {
        /// Stable document Part UUID selected by this Pose reference.
        part_id: String,
    },
    /// Preserve an unresolved external Part runtime ID.
    Unresolved {
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Stable Pose group entry with a Part target and optional links.
pub struct PoseEntry {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed resolved/unresolved Part reference retained by the Pose entry or link.
    pub part: PosePartRef,
    /// Optional ordered Pose links; None retains absence rather than an explicit empty list.
    pub links: Option<crate::SharedVec<PoseLink>>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Stable link from a Pose entry to another Part target.
pub struct PoseLink {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: PosePartRef,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Ordered Pose entries sharing a named group.
pub struct PoseGroup {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Human-readable display or registration name; distinct from persistent object identity.
    pub name: String,
    /// Ordered Pose entries owned by this group.
    pub entries: crate::SharedVec<PoseEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Authored Pose singleton with groups and optional fade settings.
pub struct PoseAsset {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Optional format type label retained from the imported Pose asset.
    pub file_type: Option<String>,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Ordered owned Pose groups; each owns its entries and link rows.
    pub groups: crate::SharedVec<PoseGroup>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
    /// Original runtime-ID namespace used to validate preservation of opaque imported fields.
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Original imported-content hash used to detect unsupported changes to opaque fields.
    pub opaque_source_content_hash: Option<String>,
}

impl PoseAsset {
    pub fn entity_ids(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str()).chain(self.groups.iter().flat_map(|g| {
            std::iter::once(g.id.as_str()).chain(g.entries.iter().flat_map(|e| {
                std::iter::once(e.id.as_str())
                    .chain(e.links.iter().flatten().map(|l| l.id.as_str()))
            }))
        }))
    }

    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self
                .groups
                .iter()
                .flat_map(|group| &group.entries)
                .any(|entry| !entry.extensions.is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Independent palette ranks for the organization and deformation projections.
/// Partial lists are allowed: consumers retain source order for unlisted objects.
/// Entries use globally unique scene object IDs, never visible row indices.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct HierarchyOrder {
    #[serde(default)]
    /// Partial organization-palette order expressed as object IDs; unlisted IDs keep their
    /// source order.
    pub organization: Vec<String>,
    #[serde(default)]
    /// Partial deformation-palette order expressed as object IDs; does not reparent objects.
    pub deformation: Vec<String>,
    /// Management order only. Glue evaluation retains its native execution order.
    #[serde(default)]
    pub glues: Vec<String>,
}

impl HierarchyOrder {
    pub fn is_empty(&self) -> bool {
        self.organization.is_empty() && self.deformation.is_empty() && self.glues.is_empty()
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
/// Drawing-order range and ordered members owned by a Part or the root.
pub struct DrawOrderGroup {
    /// Empty for the root; otherwise the stable ID of the Part driving this group.
    pub owner: String,
    /// Stable mesh/Part IDs, in tie-breaking order. A Part references its group.
    pub items: Vec<String>,
    /// Inclusive lower drawing-order bound for this group.
    pub min_order: i32,
    /// Inclusive upper drawing-order bound for this group.
    pub max_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Stable package attachment path and optional owned bytes.
pub struct PackageAttachment {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Normalized package-relative path using forward slashes; absolute paths, empty components
    /// and .. are invalid.
    pub path: String,
    /// Optional owned file contents; None preserves an unavailable attachment, not an empty file.
    pub bytes: Option<std::sync::Arc<Vec<u8>>>,
}

/// Imported paths remain explicitly unresolved until a matching attachment is loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum AttachmentRef {
    /// Reference a loaded package attachment by stable ID.
    Resolved {
        /// Stable package-attachment ID, distinct from its relative package path.
        attachment_id: String,
    },
    /// Preserve a package-relative path that has not resolved to an attachment.
    Unresolved {
        /// Normalized package-relative attachment path using forward slashes; absolute paths
        /// and .. components are invalid.
        path: String,
    },
}
impl From<String> for AttachmentRef {
    fn from(path: String) -> Self {
        Self::Unresolved { path }
    }
}
impl From<&str> for AttachmentRef {
    fn from(path: &str) -> Self {
        path.to_owned().into()
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Create, edit, delete or assign package attachments and their references.
pub enum AttachmentEdit {
    /// Create an attachment at a package-relative path.
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Normalized package-relative attachment path using forward slashes; absolute paths
        /// and .. components are invalid.
        path: String,
    },
    /// Replace an attachment under an exact current-value precondition.
    Replace {
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: PackageAttachment,
        /// Complete package attachment record to replace; bytes may be explicitly unavailable.
        attachment: PackageAttachment,
    },
    /// Duplicate an attachment under a fresh ID and path.
    Duplicate {
        /// Stable ID of the existing source object to duplicate, not a filesystem path.
        source: String,
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Normalized package-relative attachment path using forward slashes; absolute paths
        /// and .. components are invalid.
        path: String,
    },
    /// Delete an unreferenced attachment.
    Delete {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    /// Delete an attachment and explicitly redirect or remove references.
    DeleteWithReplacement {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Replacement attachment ID for existing references; None explicitly removes those
        /// references.
        replacement: Option<String>,
    },
    /// Reorder selected attachment identities.
    Reorder {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Set or clear a motion registration sound attachment reference.
    SetSound {
        /// Motion-registration ID whose sound attachment is being changed.
        registration: String,
        /// Resolved attachment identity or explicitly unresolved package path.
        reference: Option<AttachmentRef>,
    },
    /// Set or clear the user-data attachment reference.
    SetUserData {
        /// Resolved attachment identity or explicitly unresolved package path.
        reference: Option<AttachmentRef>,
    },
}
