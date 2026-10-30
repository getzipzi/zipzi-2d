#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ReferenceEntry<T> {
    /// Stable UUID of this reference entry; not the identity of its target.
    pub id: String,
    /// Resolved document target or preserved unresolved runtime identifier; entry identity
    /// remains independent of the target.
    pub target: T,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub enum DisplayInfoOrigin {
    #[default]
    Generated,
    Imported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct DisplayInfo {
    /// Import/generation provenance of these display metadata records.
    pub origin: DisplayInfoOrigin,
    /// `None` and `Some([])` retain CDI collection omission and emptiness.
    pub parameters: Option<Vec<CdiParameterEntry>>,
    /// Optional ordered CDI parameter groups metadata; None preserves omission and an empty
    /// list explicitly preserves emptiness.
    pub parameter_groups: Option<Vec<CdiParameterGroup>>,
    /// Optional ordered CDI parts metadata; None preserves omission and an empty list
    /// explicitly preserves emptiness.
    pub parts: Option<Vec<CdiPartEntry>>,
    /// Optional ordered CDI combined parameters metadata; None preserves omission and an empty
    /// list explicitly preserves emptiness.
    pub combined_parameters: Option<Vec<CdiCombinedSet>>,
    /// Preserved opaque import/export extension fields; retain unknown keys when editing known
    /// fields.
    pub extensions: BTreeMap<String, Value>,
    /// Complete namespace identity at import when opaque extensions exist.
    pub opaque_source_ids: Option<CdiNamespaceIds>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct CdiParameterGroup {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Parent object ID in this hierarchy; independent of storage and drawing order. None
    /// selects the root/unassigned container.
    pub parent_id: Option<String>,
    /// Preserved opaque import/export extension fields; retain unknown keys when editing known
    /// fields.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub enum CdiParameterEntry {
    Resolved {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
        /// Preserved opaque import/export extension fields; retain unknown keys when editing
        /// known fields.
        extensions: BTreeMap<String, Value>,
    },
    Unresolved {
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        runtime_id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
        /// Preserved opaque import/export extension fields; retain unknown keys when editing
        /// known fields.
        extensions: BTreeMap<String, Value>,
    },
}

impl CdiParameterEntry {
    pub fn group_id(&self) -> Option<&str> {
        match self {
            Self::Resolved { group_id, .. } | Self::Unresolved { group_id, .. } => {
                group_id.as_deref()
            }
        }
    }

    pub fn extensions(&self) -> &BTreeMap<String, Value> {
        match self {
            Self::Resolved { extensions, .. } | Self::Unresolved { extensions, .. } => extensions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub enum CdiPartEntry {
    Resolved {
        /// Resolved stable document Part ID; use an Unresolved entry to preserve an external runtime identifier.
        part_id: String,
        /// Preserved opaque import/export extension fields; retain unknown keys when editing
        /// known fields.
        extensions: BTreeMap<String, Value>,
    },
    Unresolved {
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        runtime_id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Preserved opaque import/export extension fields; retain unknown keys when editing
        /// known fields.
        extensions: BTreeMap<String, Value>,
    },
}

impl CdiPartEntry {
    pub fn extensions(&self) -> &BTreeMap<String, Value> {
        match self {
            Self::Resolved { extensions, .. } | Self::Unresolved { extensions, .. } => extensions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum CdiParameterRef {
    Unspecified,
    Resolved {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
    },
    Unresolved {
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        runtime_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct CdiCombinedSet {
    /// Stable UUID of this CDI combined set; not the identity of its target.
    pub id: String,
    /// Ordered stable reference-entry records; entry identity is distinct from the referenced
    /// object identity.
    pub members: Vec<ReferenceEntry<CdiParameterRef>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct CdiNamespaceIds {
    /// Imported parameter document-ID to runtime-ID map, retained to validate opaque metadata.
    pub parameters: BTreeMap<String, String>,
    /// Imported Part document-ID to runtime-ID map, retained to validate opaque metadata.
    pub parts: BTreeMap<String, String>,
    /// Imported group document-ID to runtime-ID map, retained to validate opaque metadata.
    pub groups: BTreeMap<String, String>,
}

impl DisplayInfo {
    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self
                .parameter_groups
                .as_ref()
                .is_some_and(|items| items.iter().any(|x| !x.extensions.is_empty()))
            || self
                .parameters
                .as_ref()
                .is_some_and(|items| items.iter().any(|x| !x.extensions().is_empty()))
            || self
                .parts
                .as_ref()
                .is_some_and(|items| items.iter().any(|x| !x.extensions().is_empty()))
    }

    pub fn first_extension_path(&self) -> Option<String> {
        if let Some(key) = self.extensions.keys().next() {
            return Some(format!("$.{key}"));
        }
        if let Some(groups) = &self.parameter_groups {
            for (i, group) in groups.iter().enumerate() {
                if let Some(key) = group.extensions.keys().next() {
                    return Some(format!("$.ParameterGroups[{i}].{key}"));
                }
            }
        }
        if let Some(parameters) = &self.parameters {
            for (i, entry) in parameters.iter().enumerate() {
                if let Some(key) = entry.extensions().keys().next() {
                    return Some(format!("$.Parameters[{i}].{key}"));
                }
            }
        }
        if let Some(parts) = &self.parts {
            for (i, entry) in parts.iter().enumerate() {
                if let Some(key) = entry.extensions().keys().next() {
                    return Some(format!("$.Parts[{i}].{key}"));
                }
            }
        }
        None
    }
}

impl PoseAsset {
    pub fn empty(id: String) -> Self {
        Self {
            id,
            file_type: Some("Live2D Pose".into()),
            fade_in: None,
            groups: Default::default(),
            extensions: BTreeMap::new(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        }
    }
}

impl MotionClip {
    pub fn display_tracks(&self) -> Vec<&MotionTrack> {
        if self.track_order.is_empty() {
            self.tracks.iter().collect()
        } else {
            self.track_order
                .iter()
                .filter_map(|id| self.tracks.iter().find(|t| t.id == *id))
                .collect()
        }
    }
}
