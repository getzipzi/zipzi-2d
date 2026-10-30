#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
pub enum EditOperation {
    Modeling {
        operation: Box<ModelingCommand>,
    },
    Scene {
        operation: crate::authoring::SceneOperation,
    },
    MeshBinding {
        operation: crate::authoring::MeshBindingOperation,
    },
    Parameter {
        operation: crate::authoring::ParameterOperation,
    },
    Mesh {
        operation: crate::authoring::MeshOperation,
    },
    #[serde(untagged)]
    Resource(ResourceOperation),
}

crate::api_union! {
tag = "domain";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryWire)]
pub enum ResourceOperation {
    CreateImage {
        /// Creation specification; native admission supplies identities/defaults and rejects
        /// invalid input.
        spec: crate::authoring::ImageCreateSpec,
        /// Client-owned prepared-image handle; creating an operation value does not transfer
        /// ownership.
        image: ImageId,
    },
    ReplaceImage {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Client-owned prepared-image handle; creating an operation value does not transfer
        /// ownership.
        image: ImageId,
    },
    RenameImage {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Required current display name; None disables this name precondition.
        expected_name: Option<String>,
    },
    ApplyPlan {
        /// Client-owned plan handle, valid only for its captured source and owning client.
        plan: PlanId,
    },
}
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize)]
    pub struct ResourceUsage {
        /// Runtime-wide retained independent Preview leases; active Jobs can retain workspace after release.
        pub previews: usize,
        /// Number of distinct retained allocations counted by Runtime resource accounting.
        pub allocations: usize,
        /// Number of retained resource-accounting entries, including history/lease holders.
        pub resource_entries: usize,
        /// Runtime-wide number of retained client-owned prepared image handles.
        pub prepared_images: usize,
        /// Runtime-wide number of retained client-owned plans, not operations inside each plan.
        pub plans: usize,
        /// Retained decoded RGBA storage in bytes under Runtime accounting.
        pub rgba_bytes: usize,
        /// Retained encoded PNG storage in bytes under Runtime accounting.
        pub png_bytes: usize,
        /// Retained immutable binary blob storage in bytes under Runtime accounting.
        pub blob_bytes: usize,
        /// Reserved bytes for resource preparation that has not yet finished.
        pub preparing_bytes: usize,
        /// Reserved bytes for in-progress binary transfers.
        pub transfer_bytes: usize,
    }
}
