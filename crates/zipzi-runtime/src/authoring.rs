//! Selected typed API for zipzi-python2. Both local
//! and IPC adapters use these values and the same Runtime handlers.
use crate::{ImageId, PlanId, SourceRef};
use serde::{Deserialize, Serialize};

zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryOutput)]
pub struct MeshSnapshot {
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Owned vertex IDs, positions, UVs and triangles; triangles contain vertex IDs, not
    /// position offsets.
    pub geometry: zipzi_sdk::MeshGeometry,
}
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub mesh_transfer: Option<zipzi_sdk::ManualMeshTransferReport>,
    pub source: SourceRef,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterQuery {
    pub session: crate::SessionId,
    pub edit: Option<crate::EditId>,
    pub ids: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshBatchQuery {
    pub session: crate::SessionId,
    pub edit: Option<crate::EditId>,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshBatch {
    pub source: SourceRef,
    pub items: Vec<MeshSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParameterBatch {
    pub source: SourceRef,
    pub parameters: Vec<zipzi_core::Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
pub enum EditOperation {
    Modeling {
        operation: Box<crate::modeling::ModelingCommand>,
    },
    Scene {
        operation: zipzi_sdk::SceneOperation,
    },
    MeshBinding {
        operation: zipzi_sdk::MeshBindingOperation,
    },
    Parameter {
        operation: zipzi_sdk::ParameterOperation,
    },
    Mesh {
        operation: zipzi_sdk::MeshOperation,
    },
    #[serde(untagged)]
    Resource(ResourceOperation),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshBindingQuery {
    pub session: crate::SessionId,
    pub edit: Option<crate::EditId>,
    pub ids: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MeshBindingBatch {
    pub source: SourceRef,
    pub bindings: Vec<zipzi_core::MeshBinding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneQuery {
    pub session: crate::SessionId,
    pub edit: Option<crate::EditId>,
    pub ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SceneBatch {
    pub source: SourceRef,
    pub items: Vec<zipzi_sdk::SceneSummary>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelingSampleQuery {
    pub session: crate::SessionId,
    pub edit: Option<crate::EditId>,
    pub parameters: Vec<zipzi_core::PreviewValues>,
    pub checks: zipzi_sdk::GeometryChecks,
    pub expected: Option<SourceRef>,
}
#[derive(Debug, Serialize, zipzi_api_derive::ApiFields)]
pub struct ModelingSampleBatch {
    #[serde(skip)]
    /// Client-owned frozen-sampling lease ID; release through the owning job/resource lifecycle.
    pub lease: Option<std::sync::Arc<zipzi_project::TransferReservation>>,
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Captured sampling source state, distinguishing formal content from an Edit candidate.
    pub state: &'static str,
    /// Owned CPU sampling result captured from this frozen source.
    pub samples: zipzi_sdk::ModelingSamples,
}

zipzi_core::api_union! {
tag = "kind";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanIntent {
    MeshRename {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Required current display name; None disables this name precondition.
        expected_name: Option<String>,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    ImageRename {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Required current display name; None disables this name precondition.
        expected_name: Option<String>,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
}
}

zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize)]
pub struct PlanSnapshot {
    /// Client-owned plan handle, valid only for its captured source and owning client.
    pub plan: PlanId,
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Cached native plan report containing source, estimated costs and predicted losses.
    pub report: zipzi_sdk::ModelingPlanReport,
}
}

pub const MAX_IMAGE_BYTES: usize = 256 << 20;
/// JSON controls remain bounded; larger content uses binary blob chunks.
pub const MAX_INLINE_IMAGE_BYTES: usize = 1 << 20;

zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize)]
pub struct PreparedImageInfo {
    /// Client-owned prepared-image handle; creating an operation value does not transfer ownership.
    pub image: ImageId,
    /// Image width in pixels; prepared image content requires a positive dimension.
    pub width: u32,
    /// Image height in pixels; prepared image content requires a positive dimension.
    pub height: u32,
}
}
zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct ImagePixels {
    /// Image width in pixels; prepared image content requires a positive dimension.
    pub width: u32,
    /// Image height in pixels; prepared image content requires a positive dimension.
    pub height: u32,
    /// Owned tightly packed RGBA8 pixels in row-major order, top row first; length is width *
    /// height * 4 bytes, with straight alpha.
    pub rgba: std::sync::Arc<[u8]>,
}
}
zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct ImageSnapshot {
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Owned decoded image pixels; reading this value does not perform another host request.
    pub pixels: ImagePixels,
}
}

zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MeshQuery {
        /// Runtime Session handle; distinct from the persistent document UUID.
        pub session: crate::SessionId,
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        pub id: String,
        /// Edit candidate handle; None selects formal document state.
        pub edit: Option<crate::EditId>,
    }
}
/// Typed registration is executable, not a list of strings naming a handler.
pub struct Registered<I, O> {
    pub name: &'static str,
    handler: fn(&mut crate::Runtime, crate::ClientId, I) -> crate::Result<O>,
}
impl<I: zipzi_core::api_shape::ApiShape, O: zipzi_core::api_shape::ApiShape> Registered<I, O> {
    pub const fn new(
        name: &'static str,
        handler: fn(&mut crate::Runtime, crate::ClientId, I) -> crate::Result<O>,
    ) -> Self {
        Self { name, handler }
    }
    pub fn call(
        &self,
        runtime: &mut crate::Runtime,
        client: crate::ClientId,
        input: I,
    ) -> crate::Result<O> {
        (self.handler)(runtime, client, input)
    }
    pub fn shapes(&self) -> (zipzi_core::api_shape::Shape, zipzi_core::api_shape::Shape) {
        (I::api_shape(), O::api_shape())
    }
}
pub const MESH_QUERY: Registered<MeshQuery, MeshSnapshot> =
    Registered::new("mesh_snapshot", |rt, client, input| {
        rt.mesh_snapshot(client, input.session, input.edit, &input.id)
    });

zipzi_core::api_union! {
tag = "domain";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryWire)]
pub enum ResourceOperation {
    CreateImage {
        /// Creation specification; native admission supplies identities/defaults and rejects
        /// invalid input.
        spec: zipzi_sdk::ImageCreateSpec,
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

zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ImageQuery {
        /// Runtime Session handle; distinct from the persistent document UUID.
        pub session: crate::SessionId,
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        pub id: String,
        /// Edit candidate handle; None selects formal document state.
        pub edit: Option<crate::EditId>,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PlanQuery {
        /// Runtime Session handle; distinct from the persistent document UUID.
        pub session: crate::SessionId,
        /// Edit candidate handle; None selects formal document state.
        pub edit: Option<crate::EditId>,
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        pub intent: PlanIntent,
    }
}
zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PreparedPixelsQuery {
        /// Client-owned prepared-image handle; creating an operation value does not transfer
        /// ownership.
        pub image: ImageId,
    }
}
pub const IMAGE_QUERY: Registered<ImageQuery, ImageSnapshot> =
    Registered::new("image_snapshot", |rt, client, input| {
        rt.image_snapshot(client, input.session, input.edit, &input.id)
    });
pub const PLAN_QUERY: Registered<PlanQuery, PlanSnapshot> =
    Registered::new("plan_authoring", |rt, client, input| {
        rt.plan_authoring(client, input.session, input.edit, input.intent)
    });
pub const PREPARED_PIXELS_QUERY: Registered<PreparedPixelsQuery, ImagePixels> =
    Registered::new("prepared_pixels", |rt, client, input| {
        rt.prepared_image_pixels(client, input.image)
    });

zipzi_core::api_record! {
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
impl crate::Runtime {
    /// Selected public counters; allocator/RSS and transport tables are separate.
    pub fn resource_usage(&self) -> ResourceUsage {
        let memory = self.image_memory_usage();
        ResourceUsage {
            previews: self.preview_count(),
            allocations: memory.allocations,
            resource_entries: self.memory_image_entries(),
            prepared_images: self.prepared_image_count(),
            plans: self.modeling_plan_count(),
            rgba_bytes: memory.rgba_bytes,
            png_bytes: memory.png_bytes,
            blob_bytes: memory.blob_bytes,
            preparing_bytes: memory.preparing_bytes,
            transfer_bytes: memory.transfer_bytes,
        }
    }
}
