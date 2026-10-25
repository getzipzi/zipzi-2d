//! Selected typed API for zipzi-python2. Both local
//! and IPC adapters use these values and the same Runtime handlers.
use crate::{ImageId, PlanId, SourceRef};
use serde::{Deserialize, Serialize};

zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSnapshot {
    pub source: SourceRef,
    pub id: String,
    pub name: String,
    pub geometry: zipzi_sdk::MeshGeometry,
}
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
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
pub struct ParameterBatch {
    pub source: SourceRef,
    pub parameters: Vec<zipzi_core::Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditOperation {
    MeshBinding {
        operation: zipzi_sdk::MeshBindingOperation,
    },
    Parameter {
        operation: zipzi_sdk::ParameterOperation,
    },
    Mesh {
        operation: zipzi_sdk::MeshOperation,
    },
    CreateImage {
        spec: zipzi_sdk::ImageCreateSpec,
        image: ImageId,
    },
    ReplaceImage {
        id: String,
        image: ImageId,
    },
    ApplyPlan {
        plan: PlanId,
    },
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
pub struct ModelingSampleQuery {
    pub session: crate::SessionId,
    pub edit: Option<crate::EditId>,
    pub parameters: Vec<zipzi_core::PreviewValues>,
    pub checks: zipzi_sdk::GeometryChecks,
    pub expected: Option<SourceRef>,
}
#[derive(Debug, Serialize)]
pub struct ModelingSampleBatch {
    pub source: SourceRef,
    pub state: &'static str,
    pub samples: zipzi_sdk::ModelingSamples,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanIntent {
    MeshRename {
        id: String,
        expected_name: String,
        name: String,
    },
    ImageRename {
        id: String,
        expected_name: String,
        name: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanSnapshot {
    pub plan: PlanId,
    pub source: SourceRef,
    pub report: zipzi_sdk::ModelingPlanReport,
}

/// The initial control transport intentionally limits pixel payloads to 1 MiB.
/// Larger/binary payloads require the later resource transport stage.
pub const MAX_IMAGE_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Serialize)]
pub struct PreparedImageInfo {
    pub image: ImageId,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImagePixels {
    pub width: u32,
    pub height: u32,
    pub rgba: std::sync::Arc<[u8]>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImageSnapshot {
    pub source: SourceRef,
    pub id: String,
    pub name: String,
    pub pixels: ImagePixels,
}

zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MeshQuery {
        pub session: crate::SessionId,
        pub id: String,
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
