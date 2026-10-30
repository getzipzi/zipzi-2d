crate::api_record! {
    /// Metadata projection; geometry remains in the explicit mesh/CPU queries.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SceneSummary {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        pub id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// External format/runtime identifier retained for import/export; not the document
        /// object ID.
        pub runtime_id: String,
        /// Object category for this metadata record; use it to select the corresponding domain API.
        pub object_kind: String,
        /// Optional organizational Part ID; None means root or not applicable for this object kind.
        pub part_id: Option<String>,
        /// Optional deformation parent for meshes/transforms or organization parent for
        /// Parts/groups; None means root or not applicable.
        pub parent_id: Option<String>,
        /// Whether this object participates in evaluation; editor hiding is a separate state.
        pub enabled: bool,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct PartCreateSpec {
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// Optional new object UUID; None allocates a fresh UUID in the SDK when creation executes.
        pub id: Option<String>,
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        pub parent_id: Option<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        pub before: Option<String>,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct ParameterGroupCreateSpec {
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// Optional new object UUID; None allocates a fresh UUID in the SDK when creation executes.
        pub id: Option<String>,
        /// Optional external group identifier; None uses the native default empty string.
        pub runtime_id: Option<String>,
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        pub parent_id: Option<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        pub before: Option<String>,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct ScenePlacement {
        /// Optional organizational Part ID; None uses the root organization.
        pub part_id: Option<String>,
        /// Optional deformation-parent transform ID; None uses model space.
        pub parent_id: Option<String>,
        /// Optional organization-order sibling ID; None appends in the selected organization
        /// collection.
        pub before: Option<String>,
        /// Optional deformation-order sibling ID; None appends in the selected deformation
        /// collection.
        pub deformation_before: Option<String>,
    }
}
