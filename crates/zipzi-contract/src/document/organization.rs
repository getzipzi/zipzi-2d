#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select an organization collection independently of deformation and drawing order.
pub enum OrganizationCollection {
    /// Select parameters in one group or at the root.
    Parameters {
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
    },
    /// Select child parameter groups under a parent or at the root.
    ParameterGroups {
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
    },
    /// Select members of a Part or the root organization.
    PartMembers {
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Select whether deleting organization containers also deletes their contents.
pub enum OrganizationDeletePolicy {
    /// Delete the selected containers with their contents.
    Contents,
    /// Remove containers while preserving their contents.
    Dissolve,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Create, move, duplicate or delete organizational objects and parameter groups.
pub enum OrganizationEdit {
    /// Create a parameter in the selected group.
    CreateParameter {
        /// Complete parameter record with identity, finite range, default and runtime behavior.
        parameter: Parameter,
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Create a parameter group.
    CreateGroup {
        /// Complete CDI parameter-group metadata record to create or replace.
        group: CdiParameterGroup,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Create a Part in the organizational hierarchy.
    CreatePart {
        /// Complete Part record, including identity, hierarchy context and enabled/drawing state.
        part: Part,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Change a parameter group assignment.
    SetParameterGroup {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
    },
    /// Replace group metadata under an exact precondition.
    PatchGroup {
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: CdiParameterGroup,
        /// Complete CDI parameter-group metadata record to create or replace.
        group: CdiParameterGroup,
    },
    /// Move selected identities to an organization collection.
    Move {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: OrganizationCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Duplicate selected identities into an organization collection.
    Duplicate {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: OrganizationCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// Delete or dissolve selected containers under the chosen policy.
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: OrganizationDeletePolicy,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape)]
/// Select a document metadata field or repair domain.
pub enum DocumentField {
    /// Model3 layout metadata.
    Layout,
    /// Export frame settings.
    ExportSettings,
    /// Source canvas metadata.
    SourceCanvas,
    /// Preserved opaque document metadata.
    OpaqueMetadata,
    /// Unresolved imported attachment paths.
    MissingAttachments,
    /// CDI display information.
    DisplayInfo,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "field",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Typed replacement value for a writable document metadata field.
pub enum DocumentValue {
    /// Replace or remove model3 layout metadata.
    Layout(
        /// Replacement model3 layout map; None explicitly removes the layout.
        Option<BTreeMap<String, f64>>,
    ),
    /// Replace export-frame settings.
    ExportSettings(#[doc = "Replacement export-frame scale and padding settings."] ExportSettings),
    /// Replace or remove source canvas metadata.
    SourceCanvas(
        /// Replacement source-canvas metadata; None explicitly removes the canvas provenance.
        Option<Canvas>,
    ),
}
impl DocumentValue {
    pub fn field(&self) -> DocumentField {
        match self {
            Self::Layout(_) => DocumentField::Layout,
            Self::ExportSettings(_) => DocumentField::ExportSettings,
            Self::SourceCanvas(_) => DocumentField::SourceCanvas,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Patch or repair document metadata under explicit preconditions.
pub enum DocumentAuthoringEdit {
    /// Replace a typed document value under an exact precondition.
    Patch {
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: DocumentValue,
        /// Replacement typed document value; its variant must match expected.
        value: DocumentValue,
    },
    /// Resolve an imported attachment path to an attachment or clear it.
    RepairMissingAttachment {
        /// Unresolved imported attachment path to repair; this command does not read client-
        /// side files.
        path: String,
        /// Stable package-attachment ID, distinct from its relative package path.
        attachment_id: Option<String>,
    },
    /// Resolve or clear an imported CDI parameter reference.
    RepairCdiParameter {
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        runtime_id: String,
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: Option<String>,
    },
    /// Resolve or clear an imported CDI Part reference.
    RepairCdiPart {
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        runtime_id: String,
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
    },
}
