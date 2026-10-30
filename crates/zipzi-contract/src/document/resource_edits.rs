#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ResourceEdit {
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    PatchName {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    SetContent {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current image-content metadata precondition; None requires the asset to have
        /// no loaded content.
        expected: Option<ImageContent>,
        /// Image content metadata; None explicitly represents an asset with no loaded image
        /// content.
        content: Option<ImageContent>,
    },
    Reorder {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Duplicate {
        /// Stable ID of the existing source object to duplicate, not a filesystem path.
        source: String,
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// None rejects references; Some explicitly redirects or detaches every user.
    Delete {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// None rejects deletion while referenced; Some explicitly redirects users to a
        /// resolved target or an unresolved/detached reference.
        replacement: Option<SceneReference>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ModelTargetRef {
    Unspecified,
    Resolved {
        /// Stable document ID of the object described or selected by this value.
        object_id: String,
    },
    Unresolved {
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        runtime_id: String,
    },
}
impl ModelTargetRef {
    pub fn object_id(&self) -> Option<&str> {
        match self {
            Self::Resolved { object_id } => Some(object_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ModelParameterGroup {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Ordered stable reference-entry records selecting model parameters; entry IDs are
    /// distinct from parameter IDs.
    pub parameters: Vec<ReferenceEntry<ModelTargetRef>>,
    #[serde(default)]
    /// Preserved opaque import/export extension fields; retain unknown keys when editing known
    /// fields.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ModelHitArea {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Resolved mesh target or preserved unresolved mesh runtime identifier.
    pub mesh: ModelTargetRef,
    #[serde(default)]
    /// Preserved opaque import/export extension fields; retain unknown keys when editing known
    /// fields.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct Model3Settings {
    /// Optional ordered model3 parameter groups; None preserves omission rather than an empty list.
    pub groups: Option<Vec<ModelParameterGroup>>,
    /// Optional model3 numeric layout map; None preserves omission and an empty map is
    /// explicitly empty.
    pub layout: Option<BTreeMap<String, f64>>,
    /// Optional ordered named hit-area records with stable mesh references; None preserves
    /// omission.
    pub hit_areas: Option<Vec<ModelHitArea>>,
    /// Optional model3 user-data attachment reference; None means the field was omitted.
    pub user_data: Option<AttachmentRef>,
    /// Preserved opaque import/export extension fields; retain unknown keys when editing known
    /// fields.
    pub extensions: BTreeMap<String, Value>,
    /// Provenance is needed only for opaque metadata and unmanaged IDs inside UserData.
    pub source_runtime_ids: Option<BTreeMap<String, String>>,
    /// Preserved opaque imported model3 source metadata; None means none was retained.
    pub source_content: Option<String>,
}
impl Model3Settings {
    pub fn references_to(&self, id: &str) -> bool {
        self.groups
            .iter()
            .flatten()
            .flat_map(|group| group.parameters.iter().map(|p| &p.target))
            .chain(self.hit_areas.iter().flatten().map(|area| &area.mesh))
            .any(|target| target.object_id() == Some(id))
    }
    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self
                .groups
                .iter()
                .flatten()
                .any(|g| !g.extensions.is_empty())
            || self
                .hit_areas
                .iter()
                .flatten()
                .any(|a| !a.extensions.is_empty())
    }
}
