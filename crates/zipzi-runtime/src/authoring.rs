//! Selected typed API for zipzi-python2. Both local
//! and IPC adapters use these values and the same Runtime handlers.
use crate::SourceRef;
use serde::{Deserialize, Serialize};

pub use zipzi_contract::runtime::MeshSnapshot;
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

pub use zipzi_contract::runtime::EditOperation;

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
#[derive(Debug, Serialize)]
pub struct ModelingSampleBatch {
    #[serde(skip)]
    pub lease: Option<std::sync::Arc<zipzi_project::TransferReservation>>,
    #[serde(flatten)]
    pub data: zipzi_contract::runtime::ModelingSampleData,
}
impl std::ops::Deref for ModelingSampleBatch {
    type Target = zipzi_contract::runtime::ModelingSampleData;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

pub use zipzi_contract::runtime::PlanIntent;

pub use zipzi_contract::runtime::PlanSnapshot;

pub const MAX_IMAGE_BYTES: usize = 256 << 20;
/// JSON controls remain bounded; larger content uses binary blob chunks.
pub const MAX_INLINE_IMAGE_BYTES: usize = 1 << 20;

pub use zipzi_contract::runtime::ImagePixels;
pub use zipzi_contract::runtime::ImageSnapshot;
pub use zipzi_contract::runtime::PreparedImageInfo;

pub use zipzi_contract::runtime::MeshQuery;
/// Typed registration is executable, not a list of strings naming a handler.
pub struct Registered<I, O> {
    pub name: &'static str,
    handler: fn(&mut crate::Runtime, crate::ClientId, I) -> crate::Result<O>,
}
impl<I: zipzi_core::api_shape::ApiShape, O: zipzi_core::api_shape::ApiShape> Registered<I, O> {
    pub const fn bind(
        operation: zipzi_contract::operations::Operation<I, O>,
        handler: fn(&mut crate::Runtime, crate::ClientId, I) -> crate::Result<O>,
    ) -> Self {
        Self {
            name: operation.name,
            handler,
        }
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
pub use zipzi_contract::runtime::ResourceOperation;

pub use zipzi_contract::runtime::ImageQuery;
pub use zipzi_contract::runtime::PlanQuery;
pub use zipzi_contract::runtime::PreparedPixelsQuery;
macro_rules! handler {
    (MESH_QUERY) => {
        |rt: &mut crate::Runtime, client, input: MeshQuery| {
            rt.mesh_snapshot(client, input.session, input.edit, &input.id)
        }
    };
    (IMAGE_QUERY) => {
        |rt: &mut crate::Runtime, client, input: ImageQuery| {
            rt.image_snapshot(client, input.session, input.edit, &input.id)
        }
    };
    (PREPARED_PIXELS_QUERY) => {
        |rt: &mut crate::Runtime, client, input: PreparedPixelsQuery| {
            rt.prepared_image_pixels(client, input.image)
        }
    };
    (PLAN_QUERY) => {
        |rt: &mut crate::Runtime, client, input: PlanQuery| {
            rt.plan_authoring(client, input.session, input.edit, input.intent)
        }
    };
}
macro_rules! runtime_bindings {
    ($($id:ident: $input:ty => $output:ty = $name:literal;)*) => {
        $(pub const $id: Registered<$input, $output> = Registered::bind(zipzi_contract::operations::$id, handler!($id));)*
    };
}
zipzi_contract::authoring_operations!(runtime_bindings);

pub use zipzi_contract::runtime::ResourceUsage;
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
