//! Versioned local wire contract. The host calls this on its application thread;
//! decoding, sockets and workers never borrow a formal session.
use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
macro_rules! json { ($($value:tt)*) => {{ crate::metrics::value_build(); serde_json::json!($($value)*) }}; }
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use zipzi_core::{Document, Mesh, ModelSetup, Parameter, Vec2};
use zipzi_sdk::Version;

mod binary;
pub use binary::{BinaryReply, BinarySlice, MAX_BINARY_CHUNK, MAX_BINARY_HEADER};
mod values;
use values::*;
/// Wire major version, independent of package versions and API additions.
/// Bump only for incompatible framing, envelope or existing operation changes.
pub use zipzi_contract::protocol::CONTRACT;
pub const SDK_BUILD: &str = env!("ZIPZI_RUNTIME_SDK_BUILD");
pub const MAX_FRAME: usize = 16 << 20;

#[derive(Debug, Clone)]
pub struct ProtocolLimits {
    pub reconnect_grace: Duration,
    pub result_ttl: Duration,
    pub results_per_client: usize,
    pub result_bytes_per_client: usize,
    pub result_bytes_total: usize,
    pub binary_bytes_per_client: usize,
    pub binary_bytes_total: usize,
}
impl Default for ProtocolLimits {
    fn default() -> Self {
        Self {
            reconnect_grace: Duration::from_secs(30),
            result_ttl: Duration::from_secs(300),
            results_per_client: 1024,
            result_bytes_per_client: 8 << 20,
            result_bytes_total: 64 << 20,
            binary_bytes_per_client: 256 << 20,
            binary_bytes_total: 512 << 20,
        }
    }
}
#[derive(Deserialize)]
struct Hello {
    protocol: String,
    runtime: Option<RuntimeInstanceId>,
    client: Option<ClientId>,
    secret: Option<String>,
    sdk_build: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub protocol: String,
    pub runtime: RuntimeInstanceId,
    pub client: ClientId,
    pub request_id: u64,
    pub operation: Value,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    EditorMeshGenerate {
        input: Value,
    },
    RequestObservation {
        session: SessionId,
        edit: Option<EditId>,
        parameters: Vec<zipzi_core::PreviewValues>,
        view: crate::observation::ObservationView,
        expected: Option<SourceRef>,
    },
    RequestSpatialSamples {
        session: SessionId,
        edit: Option<EditId>,
        parameters: Vec<zipzi_core::PreviewValues>,
        queries: Vec<zipzi_sdk::SpatialQuery>,
        expected: Option<SourceRef>,
    },
    CreatePreview {
        session: SessionId,
        edit: Option<EditId>,
        expected: Option<SourceRef>,
    },
    PreviewState {
        preview: PreviewId,
    },
    ApplyPreview {
        preview: PreviewId,
        command: crate::preview::PreviewCommand,
    },
    SamplePreview {
        #[serde(default)]
        view: Option<crate::observation::ObservationView>,
        #[serde(default)]
        queries: Vec<zipzi_sdk::SpatialQuery>,
        preview: PreviewId,
        times: Vec<f32>,
        geometry: bool,
        incremental: bool,
    },
    ReleasePreview {
        preview: PreviewId,
    },
    ListSessions,
    RuntimeCapabilities,
    RuntimeLimits,
    CollectionPage {
        input: crate::CollectionQuery,
    },
    ReadSelection {
        session: SessionId,
        edit: Option<EditId>,
        domain: crate::CollectionDomain,
        ids: Vec<String>,
        expected: Option<SourceRef>,
    },
    DetachSession {
        session: SessionId,
    },
    CloseSession {
        session: SessionId,
        discard_modified: bool,
    },
    Statistics,
    EditorContext {
        session: SessionId,
    },
    PrepareImageJob {
        path: Option<std::path::PathBuf>,
        blob: Option<BlobId>,
    },
    CaptureImage {
        session: SessionId,
        edit: Option<EditId>,
        id: String,
    },
    ReleaseClient,
    #[cfg(feature = "bench-instrumentation")]
    Diagnostics {
        reset: bool,
    },
    CancelJob {
        job: JobId,
    },
    JobStatus {
        job: JobId,
    },
    #[cfg(feature = "test-support")]
    TestControl {
        action: String,
    },
    CreateSession {
        document_id: Option<String>,
        #[serde(default)]
        setup: ModelSetup,
    },
    AttachSession {
        session: SessionId,
    },
    SessionInfo {
        session: SessionId,
    },
    Query {
        session: SessionId,
        edit: Option<EditId>,
        meshes: Vec<String>,
        parameters: Vec<String>,
    },
    ModelingQuery {
        session: SessionId,
        edit: Option<EditId>,
        query: zipzi_sdk::ModelingQuery,
    },
    RequestModelingSamples {
        input: crate::authoring::ModelingSampleQuery,
    },
    RequestModelingPlan {
        session: SessionId,
        edit: Option<EditId>,
        intent: zipzi_sdk::ModelingIntent,
    },
    ApplyModeling {
        edit: EditId,
        commands: Vec<crate::modeling::ModelingCommand>,
    },
    ApplyAuthoring {
        edit: EditId,
        operations: Vec<crate::authoring::EditOperation>,
    },
    PrepareImage {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    PrepareImageBlob {
        width: u32,
        height: u32,
        blob: BlobId,
    },
    PreparedPixelsBlob {
        image: ImageId,
    },
    ImageSnapshotBlob {
        session: SessionId,
        edit: Option<EditId>,
        id: String,
    },
    PreparedPixels {
        image: ImageId,
    },
    ReleasePreparedImage {
        image: ImageId,
    },
    ImageSnapshot {
        session: SessionId,
        edit: Option<EditId>,
        id: String,
    },
    PlanAuthoring {
        session: SessionId,
        edit: Option<EditId>,
        intent: crate::authoring::PlanIntent,
    },
    MeshSnapshot(crate::authoring::MeshQuery),
    MeshSnapshots(crate::authoring::MeshBatchQuery),
    ParameterSnapshot(crate::authoring::ParameterQuery),
    MeshBindingSnapshot(crate::authoring::MeshBindingQuery),
    SceneSnapshot(crate::authoring::SceneQuery),
    CaptureObjectRef {
        session: SessionId,
        kind: zipzi_sdk::ObjectKind,
        id: String,
    },
    ResolveObjectRef {
        reference: ObjectRefId,
    },
    ObjectSceneCapabilities {
        reference: ObjectRefId,
    },
    CaptureLabelAddress {
        reference: ObjectRefId,
    },
    PlanLabelPatch {
        address: FieldAddressId,
        name: String,
    },
    CaptureAddress {
        session: SessionId,
        selector: crate::addressing::AddressSelector,
    },
    PlanAddressPatch {
        address: FieldAddressId,
        patch: crate::addressing::AddressPatch,
    },
    ModelingCapabilities {
        reference: ObjectRefId,
        domain: crate::addressing::CapabilityDomain,
    },
    CollectionCapabilities {
        session: SessionId,
        query: crate::addressing::CollectionQuery,
    },
    ReleaseObjectRef {
        reference: ObjectRefId,
    },
    ReleaseFieldAddress {
        address: FieldAddressId,
    },
    PlanSceneAuthoring {
        session: SessionId,
        edit: Option<EditId>,
        operation: zipzi_sdk::SceneOperation,
    },
    SampleModeling {
        session: SessionId,
        edit: Option<EditId>,
        parameters: Vec<zipzi_core::PreviewValues>,
        #[serde(default)]
        checks: zipzi_sdk::GeometryChecks,
        expected: Option<SourceRef>,
    },
    ApplyRgba {
        edit: EditId,
        id: String,
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        replace: bool,
    },
    CreateBlob {
        length: usize,
    },
    AppendBlob {
        blob: BlobId,
        offset: usize,
        bytes: Vec<u8>,
    },
    AppendBlobBinary {
        blob: BlobId,
        offset: usize,
        length: usize,
        payload_sha256: String,
    },
    ReadBlobBinary {
        blob: BlobId,
        offset: usize,
        length: usize,
    },
    ReadBlob {
        blob: BlobId,
        offset: usize,
        length: usize,
    },
    SealBlob {
        blob: BlobId,
    },
    ReleaseBlob {
        blob: BlobId,
    },
    ApplyRgbaBlob {
        edit: EditId,
        id: String,
        name: String,
        width: u32,
        height: u32,
        blob: BlobId,
        replace: bool,
    },
    StartAuthoringUpload {
        edit: EditId,
        length: usize,
    },
    ApplyAuthoringBlob {
        edit: EditId,
        blob: BlobId,
    },
    CollectImages,
    PlanModeling {
        session: SessionId,
        edit: Option<EditId>,
        intent: zipzi_sdk::ModelingIntent,
    },
    ApplyModelingPlan {
        edit: EditId,
        plan: PlanId,
    },
    DeletionReportPage {
        plan: PlanId,
        section: zipzi_sdk::DeletionSection,
        offset: usize,
        limit: usize,
    },
    ReleaseModelingPlan {
        plan: PlanId,
    },
    BeginEdit {
        session: SessionId,
        label: String,
        expected: Option<Version>,
    },
    Apply {
        edit: EditId,
        commands: Vec<Command>,
    },
    Commit {
        edit: EditId,
    },
    ReleaseEdit {
        edit: EditId,
    },
    Undo {
        session: SessionId,
    },
    Redo {
        session: SessionId,
    },
    Save {
        session: SessionId,
        path: std::path::PathBuf,
        expected: Option<Version>,
    },
    Project {
        session: Option<SessionId>,
        request: crate::project_io::ProjectRequest,
    },
    ImportProject {
        source: ImportSource,
    },
    Open {
        path: std::path::PathBuf,
    },
    Job {
        job: JobId,
    },
    ReleaseJob {
        job: JobId,
    },
    Subscribe {
        session: Option<SessionId>,
    },
    Events {
        cursor: EventCursor,
        limit: Option<usize>,
    },
    RequestStatus {
        request_id: u64,
    },
    ShowWindow {
        session: Option<SessionId>,
    },
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    CreateParameter {
        parameter: Parameter,
    },
    ReplaceParameter {
        parameter: Parameter,
        #[serde(default)]
        preserve_runtime_id: bool,
        #[serde(default)]
        preserve_kind: bool,
        #[serde(default)]
        preserve_decimal_places: bool,
    },
    CreateMesh {
        mesh: Mesh,
    },
    RenameMesh {
        id: String,
        name: String,
    },
    UpdatePositions {
        id: String,
        vertex_ids: Vec<u32>,
        positions: Vec<[f32; 2]>,
    },
    EraseObject {
        id: String,
    },
}
struct Record {
    id: u64,
    digest: [u8; 32],
    result: Value,
    binary: Option<BinarySlice>,
    bytes: usize,
    metadata_bytes: usize,
    at: Instant,
}
struct RemoteClient {
    secret: String,
    disconnected: Option<Instant>,
    connection: u64,
    highest: u64,
    records: VecDeque<Record>,
}
/// Every connection has an unguessable resume secret and a connection generation.
/// A late disconnect from a replaced socket cannot expire the resumed client.
pub struct Protocol {
    execution_leases: Vec<zipzi_project::TransferReservation>,
    limits: ProtocolLimits,
    clients: HashMap<ClientId, RemoteClient>,
    next_connection: u64,
    show_window: Option<Option<SessionId>>,
    windows: bool,
    mesh_generator: Option<fn(Value) -> Value>,
    binary_input: Option<Vec<u8>>,
    binary_output: Option<BinarySlice>,
}
impl Default for Protocol {
    fn default() -> Self {
        Self::new(ProtocolLimits::default())
    }
}
impl Protocol {
    pub fn receive(
        &mut self,
        runtime: &mut Runtime,
        auth: Option<crate::local::Auth>,
        value: Value,
        now: Instant,
    ) -> Value {
        match auth {
            None => self.hello(runtime, value, now),
            Some(auth) => match crate::metrics::from_value(value) {
                Ok(request) => self.handle(runtime, auth.client, auth.connection, request, now),
                Err(e) => failure("INVALID_REQUEST", &e.to_string()),
            },
        }
    }
    pub fn with_mesh_generator(mut self, generate: fn(Value) -> Value) -> Self {
        self.mesh_generator = Some(generate);
        self
    }
    pub fn with_window_support(mut self) -> Self {
        self.windows = true;
        self
    }
    pub fn new(limits: ProtocolLimits) -> Self {
        Self {
            execution_leases: vec![],
            limits,
            clients: HashMap::new(),
            next_connection: 0,
            show_window: None,
            windows: false,
            mesh_generator: None,
            binary_input: None,
            binary_output: None,
        }
    }
    /// Hello is the first frame on every socket. No Runtime registration or writes
    /// occur until the protocol major, optional build pin and old-instance checks pass.
    pub fn hello(&mut self, runtime: &mut Runtime, hello: Value, now: Instant) -> Value {
        self.expire(runtime, now);
        if hello.get("protocol").and_then(Value::as_str) != Some(CONTRACT) {
            return failure("PROTOCOL_MISMATCH", "Incompatible local protocol");
        }
        let hello: Hello = match crate::metrics::from_value(hello) {
            Ok(hello) => hello,
            Err(e) => return failure("INVALID_HANDSHAKE", &e.to_string()),
        };
        debug_assert_eq!(hello.protocol, CONTRACT);
        if hello.sdk_build.as_deref().is_some_and(|s| s != SDK_BUILD) {
            return failure("BUILD_MISMATCH", "Requested SDK build is unavailable");
        }
        if hello
            .runtime
            .is_some_and(|old| old != runtime.instance_id())
        {
            return failure("HOST_RESTARTED", "The original Runtime no longer exists");
        }
        if hello.client.is_some() && (hello.runtime.is_none() || hello.secret.is_none()) {
            return failure(
                "INVALID_HANDSHAKE",
                "Resume needs instance, client and secret",
            );
        }
        let _client_memory = if hello.client.is_none() {
            match runtime.reserve_transfer(1024) {
                Ok(lease) => Some(lease),
                Err(e) => return error(e),
            }
        } else {
            None
        };
        let client = if let Some(id) = hello.client {
            let Some(remote) = self.clients.get(&id) else {
                return failure("CLIENT_EXPIRED", "Reconnect grace expired");
            };
            if hello.secret.as_deref() != Some(&remote.secret) {
                return failure("WRONG_OWNER", "Invalid resume secret");
            }
            id
        } else {
            let id = match runtime.register_client() {
                Ok(id) => id,
                Err(e) => return error(e),
            };
            self.clients.insert(
                id,
                RemoteClient {
                    secret: uuid::Uuid::new_v4().to_string(),
                    disconnected: None,
                    connection: 0,
                    highest: 0,
                    records: VecDeque::new(),
                },
            );
            id
        };
        self.next_connection += 1;
        let remote = self.clients.get_mut(&client).unwrap();
        remote.disconnected = None;
        remote.connection = self.next_connection;
        let mut capabilities = vec![
            "collection_pages",
            "session_lifecycle",
            "runtime_introspection",
            "typed_binary_arrays",
            "typed_full_modeling",
            "typed_field_addresses",
            "read_jobs",
            "project_io",
            "resource_jobs",
            "editor_context",
            "typed_mesh_batch",
            "typed_mesh_bindings",
            "typed_modeling_samples",
            "typed_scene_authoring",
            "typed_object_references",
            "binary_blobs",
            "typed_event_pages",
            "typed_parameters",
            "typed_meshes",
            "typed_resources_plans",
            "typed_resource_values",
            "typed_job_lifecycle",
            "modeling_batch",
            "modeling_records",
            "modeling_cpu_samples",
            "image_rgba",
            "image_memory_accounting",
            "modeling_plans",
            "blobs",
            "sessions",
            "mesh_parameter_batch",
            "candidate_reads",
            "mesh_parameter_edits",
            "history",
            "save",
            "open",
            "events",
            "request_recovery",
        ];
        if self.mesh_generator.is_some() {
            capabilities.push("editor_mesh_generate");
        }
        if self.windows {
            capabilities.push("show_window");
        }
        let response = success(
            json!({"runtime": runtime.instance_id(), "client": client, "secret": remote.secret,
            "connection": remote.connection, "sdk_build": SDK_BUILD, "protocol": CONTRACT, "host_pid": std::process::id(),
            "capabilities": capabilities,
            "limits": {"frame_bytes": MAX_FRAME, "reconnect_seconds": self.limits.reconnect_grace.as_secs_f64(),
                "result_seconds": self.limits.result_ttl.as_secs_f64(), "results_per_client": self.limits.results_per_client,
                "result_bytes_per_client": self.limits.result_bytes_per_client, "result_bytes_total": self.limits.result_bytes_total,"binary_bytes_per_client":self.limits.binary_bytes_per_client,"binary_bytes_total":self.limits.binary_bytes_total}}),
        );
        self.refresh_memory(runtime);
        response
    }
    pub fn disconnect(&mut self, client: ClientId, connection: u64, now: Instant) {
        if let Some(c) = self
            .clients
            .get_mut(&client)
            .filter(|c| c.connection == connection)
        {
            c.disconnected = Some(now);
        }
    }
    /// Authenticated sockets still connected, excluding clients retained only
    /// for reconnect grace. In-process Runtime clients are not socket clients.
    pub fn has_connected_clients(&self) -> bool {
        self.clients.values().any(|c| c.disconnected.is_none())
    }
    /// A desktop host about to exit may end reconnect grace early. Formal
    /// sessions survive so the host can still ask to save their unsaved content.
    pub fn release_disconnected_clients(&mut self, runtime: &mut Runtime) {
        self.clients.retain(|id, c| {
            if c.disconnected.is_some() {
                runtime.release_client(*id);
                false
            } else {
                true
            }
        });
        self.refresh_memory(runtime);
    }
    pub fn expire(&mut self, runtime: &mut Runtime, now: Instant) {
        self.clients.retain(|id, c| {
            if c.disconnected
                .is_some_and(|at| now.saturating_duration_since(at) >= self.limits.reconnect_grace)
            {
                runtime.release_client(*id);
                return false;
            }
            while c
                .records
                .front()
                .is_some_and(|r| now.saturating_duration_since(r.at) >= self.limits.result_ttl)
            {
                c.records.pop_front();
            }
            true
        });
        self.refresh_memory(runtime);
    }
    fn admit_value<T: serde::Serialize>(&mut self, rt: &Runtime, value: &T) -> Result<Value> {
        let bytes = bounded_json_bytes(value)?;
        self.execution_leases
            .push(rt.reserve_transfer(bytes.saturating_mul(128).saturating_add(4096))?);
        Ok(json!(value))
    }
    fn binary_bytes<'a>(records: impl Iterator<Item = &'a Record>) -> usize {
        let mut allocations = HashMap::new();
        for r in records {
            if let Some(b) = &r.binary {
                let (key, bytes) = b.allocation();
                allocations.insert(key, bytes);
            }
        }
        allocations.values().sum()
    }
    fn retained_metadata(&self) -> usize {
        self.clients
            .values()
            .map(|c| {
                c.records
                    .iter()
                    .map(|r| r.metadata_bytes)
                    .sum::<usize>()
                    .saturating_add(1024)
            })
            .sum()
    }
    /// Conservative JSON-node/record estimates; binary backing allocations are
    /// shared with the resource pool and are not charged a second time here.
    pub fn retained_memory_bytes(&self) -> usize {
        self.retained_metadata()
    }
    fn refresh_memory(&mut self, runtime: &mut Runtime) {
        while !runtime.external_bytes_fit(self.retained_metadata()) {
            let oldest = self
                .clients
                .iter()
                .filter_map(|(id, c)| c.records.front().map(|r| (*id, r.at)))
                .min_by_key(|(_, at)| *at)
                .map(|(id, _)| id);
            let Some(oldest) = oldest else {
                break;
            };
            self.clients.get_mut(&oldest).unwrap().records.pop_front();
        }
        runtime.set_external_bytes(self.retained_metadata());
    }
    pub fn retained_results(&self) -> (usize, usize) {
        self.clients
            .values()
            .flat_map(|c| &c.records)
            .fold((0, 0), |(n, b), r| (n + 1, b + r.bytes))
    }
    pub fn take_window_request(&mut self) -> Option<Option<SessionId>> {
        self.show_window.take()
    }
    pub fn handle(
        &mut self,
        runtime: &mut Runtime,
        client: ClientId,
        connection: u64,
        request: Request,
        now: Instant,
    ) -> Value {
        self.execution_leases.clear();
        self.expire(runtime, now);
        if request.protocol != CONTRACT {
            return failure("PROTOCOL_MISMATCH", "Incompatible local protocol");
        }
        if request.runtime != runtime.instance_id() {
            return failure("HOST_RESTARTED", "Original Runtime unavailable");
        }
        let Some(remote) = self
            .clients
            .get(&client)
            .filter(|c| c.connection == connection && c.disconnected.is_none())
        else {
            return failure("CLIENT_EXPIRED", "Connection replaced or expired");
        };
        if request.client != client {
            return failure(
                "WRONG_OWNER",
                "Request client differs from authenticated connection",
            );
        }
        if matches!(
            request.operation["op"].as_str(),
            Some("append_blob_binary" | "read_blob_binary")
        ) && self.binary_input.is_none()
        {
            return failure(
                "INVALID_REQUEST",
                "Binary operation requires binary framing",
            );
        }
        // Status queries never advance the admission watermark. Otherwise asking
        // whether an unreceived request ran would make its original ID expire.
        if request.operation.get("op").and_then(Value::as_str) == Some("request_status") {
            return match crate::metrics::from_value::<Operation>(request.operation) {
                Ok(Operation::RequestStatus { request_id }) => {
                    if let Some(r) = remote.records.iter().find(|r| r.id == request_id) {
                        success(json!({"state": "completed", "response": r.result}))
                    } else if request_id <= remote.highest {
                        failure("REQUEST_EXPIRED", "Original result has expired")
                    } else {
                        success(json!({"state": "not_executed"}))
                    }
                }
                _ => failure("INVALID_REQUEST", "Invalid request status query"),
            };
        }
        // Value's sorted map gives a canonical key order. Hashes bound input storage,
        // including large apply batches, without a collision-prone short hash.
        let digest: [u8; 32] =
            Sha256::digest(crate::metrics::encode(&request.operation).unwrap()).into();
        if let Some(record) = remote.records.iter().find(|r| r.id == request.request_id) {
            return if record.digest == digest {
                self.binary_output = record.binary.clone();
                record.result.clone()
            } else {
                failure("REQUEST_ID_REUSED", "Request ID has different arguments")
            };
        }
        if request.request_id == 0 || request.request_id <= remote.highest {
            return failure("REQUEST_EXPIRED", "Old request will not execute again");
        }
        let target_edit = request
            .operation
            .get("edit")
            .and_then(|v| crate::metrics::from_value::<EditId>(v.clone()).ok());
        let mut result = match crate::metrics::from_value::<Operation>(request.operation) {
            Ok(op) => self.execute(runtime, client, op).unwrap_or_else(error),
            Err(e) => failure("UNSUPPORTED_OPERATION", &e.to_string()),
        };
        if result.get("error").is_some() {
            if let Some(edit) = target_edit {
                result["error"]["edit_state"] = json!(match runtime.edit_state(client, edit) {
                    Ok(EditState::Editing) => "editing",
                    Ok(EditState::Prepared) => "prepared",
                    Ok(EditState::Aborted) => "aborted",
                    Err(_) => "consumed",
                });
            }
        }
        // Reads as well as writes share monotonically increasing IDs. A result
        // larger than the retention budget is explicitly unknown, never re-run.
        let binary = self.binary_output.clone();
        let encoded_bytes = crate::metrics::encode(&result).unwrap().len();
        let bytes = encoded_bytes + 128 + binary.as_ref().map_or(0, |b| b.bytes().len());
        let c = self.clients.get_mut(&client).unwrap();
        c.highest = request.request_id;
        c.records.push_back(Record {
            id: request.request_id,
            digest,
            result: result.clone(),
            binary,
            bytes,
            metadata_bytes: encoded_bytes.saturating_mul(128).saturating_add(256),
            at: now,
        });
        while c.records.len() > self.limits.results_per_client
            || c.records.iter().map(|r| r.bytes).sum::<usize>()
                > self.limits.result_bytes_per_client
        {
            c.records.pop_front();
        }
        while Self::binary_bytes(c.records.iter()) > self.limits.binary_bytes_per_client {
            c.records.pop_front();
        }
        while self.retained_results().1 > self.limits.result_bytes_total
            || Self::binary_bytes(self.clients.values().flat_map(|c| c.records.iter()))
                > self.limits.binary_bytes_total
        {
            let oldest = self
                .clients
                .iter()
                .filter_map(|(id, c)| c.records.front().map(|r| (*id, r.at)))
                .min_by_key(|(_, at)| *at)
                .map(|(id, _)| id)
                .unwrap();
            self.clients.get_mut(&oldest).unwrap().records.pop_front();
        }
        self.refresh_memory(runtime);
        result
    }
    fn execute(
        &mut self,
        rt: &mut Runtime,
        client: ClientId,
        op: Operation,
    ) -> crate::Result<Value> {
        let value = match op {
            Operation::EditorMeshGenerate { input } => {
                return Ok(match self.mesh_generator {
                    Some(generate) => generate(input),
                    None => failure(
                        "UNSUPPORTED_OPERATION",
                        "Mesh generation requires an Editor host",
                    ),
                })
            }
            Operation::RuntimeCapabilities => {
                let mut c = rt.runtime_capabilities();
                c.editor_windows = self.windows;
                json!(c)
            }
            Operation::RuntimeLimits => json!(rt.limits()),
            Operation::CollectionPage { input } => json!(rt.collection_page(client, input)?),
            Operation::ReadSelection {
                session,
                edit,
                domain,
                ids,
                expected,
            } => {
                rt.check_authoring_source(client, session, edit, expected)?;
                if ids.len() > rt.limits().collection_records {
                    return Err(RuntimeError::Budget("collection_records"));
                }
                match domain {
                    CollectionDomain::Meshes => {
                        return self.execute(
                            rt,
                            client,
                            Operation::MeshSnapshots(crate::authoring::MeshBatchQuery {
                                session,
                                edit,
                                ids,
                            }),
                        );
                    }
                    CollectionDomain::Bindings => {
                        return self.execute(
                            rt,
                            client,
                            Operation::MeshBindingSnapshot(crate::authoring::MeshBindingQuery {
                                session,
                                edit,
                                ids: Some(ids),
                            }),
                        );
                    }
                    _ => match rt.read_selection(client, session, edit, domain, ids, expected)? {
                        Selection::Parameters(v) => json!(v),
                        Selection::Scene(v) => json!(v),
                        Selection::Resources(source, images) => {
                            rt.image_selection_blob(client, source, images)?
                        }
                        _ => unreachable!(),
                    },
                }
            }
            Operation::DetachSession { session } => {
                rt.detach_session(client, session)?;
                Value::Null
            }
            Operation::CloseSession {
                session,
                discard_modified,
            } => info(&rt.close_session(client, session, discard_modified)?),
            #[cfg(feature = "test-support")]
            Operation::TestControl { action } => json!(rt.test_controller().command(&action)?),
            Operation::CancelJob { job } => {
                rt.cancel_job(client, job)?;
                Value::Null
            }
            Operation::JobStatus { job } => json!(rt.job_info(client, job)?.state),
            #[cfg(feature = "bench-instrumentation")]
            Operation::Diagnostics { reset } => {
                let snapshot = if reset {
                    crate::metrics::reset()
                } else {
                    crate::metrics::snapshot()
                };
                json!(snapshot)
            }
            Operation::ReleaseClient => {
                rt.release_client(client);
                Value::Null
            }
            Operation::PrepareImageJob { path, blob } => {
                json!({"job":rt.request_image_preparation(client,path,blob)?})
            }
            Operation::CaptureImage { session, edit, id } => {
                let (image, source) = rt.capture_image(client, session, edit, &id)?;
                json!({"image":image.image,"width":image.width,"height":image.height,"source":source})
            }
            Operation::EditorContext { session } => {
                let version = rt.session_info(session)?.version;
                json!({"source":SourceRef::Formal{session,version},"context":rt.editor_context(session)?})
            }
            Operation::Statistics => {
                let (clients, sessions, edits, jobs) = rt.live_counts();
                let (results, result_bytes) = self.retained_results();
                let mut value = json!({"clients": clients, "sessions": sessions, "edits": edits, "jobs": jobs,
                    "binary_cached_bytes":Self::binary_bytes(self.clients.values().flat_map(|c|c.records.iter())),
                    "request_results": results, "request_result_bytes": result_bytes,
                    "accounted_document_bytes": rt.accounted_bytes().saturating_sub(rt.image_memory_usage().total_bytes()).saturating_sub(self.retained_metadata()),
                    "accounted_transport_result_bytes": self.retained_metadata(),
                    "accounted_cpu_bytes": rt.accounted_bytes(),
                    "image_memory": rt.image_memory_usage(), "plans": rt.modeling_plan_count(), "prepared_images":rt.prepared_image_count(), "blobs": rt.blob_count(),
                    "memory_image_entries": rt.list_sessions().iter().filter_map(|s| rt.session(s.id).ok()).map(|s| s.memory_image_entries()).sum::<usize>()});
                if let (Value::Object(value), Value::Object(usage)) =
                    (&mut value, json!(rt.resource_usage()))
                {
                    value.extend(usage);
                }
                value
            }
            Operation::ListSessions => {
                json!(rt.list_sessions().iter().map(info).collect::<Vec<_>>())
            }
            Operation::CreateSession { document_id, setup } => {
                json!(rt.create_session_auto(client, document_id.as_deref(), setup)?)
            }
            Operation::AttachSession { session } => {
                rt.attach_session(client, session)?;
                info(&rt.session_info(session)?)
            }
            Operation::SessionInfo { session } => info(&rt.session_info(session)?),
            Operation::Query {
                session,
                edit,
                meshes,
                parameters,
            } => {
                let frozen;
                let session_info = rt.session_info(session)?;
                let (doc, version, sequence) = if let Some(edit) = edit {
                    let source = rt.candidate(client, edit)?;
                    // Reject mixing a token with a different supplied SessionId.
                    if source.base_version().session_id
                        != rt.session_info(session)?.version.session_id
                    {
                        return Err(RuntimeError::InvalidReference);
                    }
                    (
                        rt.edit_document(client, edit)?,
                        source.base_version(),
                        rt.edit_sequence(client, edit)?,
                    )
                } else {
                    if matches!(session_info.state, SessionState::Ready) {
                        (rt.document(session)?, session_info.version, 0)
                    } else {
                        frozen = rt.source_snapshot(session)?;
                        (frozen.snapshot.document(), session_info.version, 0)
                    }
                };
                if meshes.len().saturating_add(parameters.len()) > 256 {
                    return Err(RuntimeError::Budget("inline query selection"));
                }
                let mut bytes = 4096usize.saturating_add(
                    doc.mesh_order()
                        .iter()
                        .chain(doc.parameter_order())
                        .map(|id| id.len().saturating_mul(6).saturating_add(8))
                        .sum::<usize>(),
                );
                for id in &meshes {
                    bytes = bytes.saturating_add(bounded_json_bytes(&doc.get_mesh(id))?);
                }
                for id in &parameters {
                    bytes = bytes.saturating_add(bounded_json_bytes(&doc.get_parameter(id))?);
                }
                if bytes > 1 << 20 {
                    return Err(RuntimeError::Budget("inline query bytes"));
                }
                self.execution_leases
                    .push(rt.reserve_transfer(bytes.saturating_mul(128))?);
                let mut value = query(doc, version, sequence, meshes, parameters);
                value["source"] =
                    json!({"session":session, "edit":edit, "version":version, "sequence":sequence});
                value["state"] = info(&session_info)["state"].clone();
                value
            }
            Operation::CreateBlob { length } => json!(rt.create_blob(client, length)?),
            Operation::AppendBlob {
                blob,
                offset,
                bytes,
            } => json!(rt.append_blob(client, blob, offset, &bytes)?),
            Operation::AppendBlobBinary {
                blob,
                offset,
                length,
                payload_sha256,
            } => {
                let bytes = self
                    .binary_input
                    .as_deref()
                    .ok_or(RuntimeError::InvalidReference)?;
                if bytes.len() != length || payload_sha256.len() != 64 {
                    return Err(RuntimeError::InvalidReference);
                }
                json!(rt.append_blob(client, blob, offset, bytes)?)
            }
            Operation::ReadBlobBinary {
                blob,
                offset,
                length,
            } => {
                self.binary_output =
                    Some(BinarySlice::new(rt.blob(client, blob)?, offset, length)?);
                json!({"length": length})
            }
            Operation::ReadBlob {
                blob,
                offset,
                length,
            } => {
                if length > 1 << 20 {
                    return Err(RuntimeError::Budget("blob chunk"));
                }
                let data = rt.blob(client, blob)?;
                let end = offset
                    .checked_add(length)
                    .ok_or(RuntimeError::InvalidReference)?;
                json!(data
                    .get(offset..end)
                    .ok_or(RuntimeError::InvalidReference)?)
            }
            Operation::SealBlob { blob } => json!(rt.seal_blob(client, blob)?),
            Operation::ReleaseBlob { blob } => {
                rt.release_blob(client, blob)?;
                Value::Null
            }
            Operation::ApplyRgbaBlob {
                edit,
                id,
                name,
                width,
                height,
                blob,
                replace,
            } => {
                rt.apply_rgba_blob(client, edit, id, name, width, height, blob, replace)?;
                Value::Null
            }
            Operation::StartAuthoringUpload { edit, length } => {
                json!({"blob":rt.start_authoring_upload(client,edit,length)?})
            }
            Operation::ApplyAuthoringBlob { edit, blob } => {
                json!(rt.apply_authoring_blob(client, edit, blob)?)
            }
            Operation::PlanModeling {
                session,
                edit,
                intent,
            } => {
                let (plan, source, report) = rt.plan_modeling(client, session, edit, intent)?;
                json!({"plan": plan, "source": source, "report": report})
            }
            Operation::ApplyModelingPlan { edit, plan } => {
                rt.apply_modeling_plan(client, edit, plan)?;
                Value::Null
            }
            Operation::DeletionReportPage {
                plan,
                section,
                offset,
                limit,
            } => {
                json!(rt.deletion_report_page(client, plan, section, offset, limit)?)
            }
            Operation::ReleaseModelingPlan { plan } => {
                rt.release_modeling_plan(client, plan)?;
                Value::Null
            }
            Operation::CollectImages => {
                json!({"collected": rt.collect_image_resources(), "usage": rt.image_memory_usage()})
            }
            Operation::ModelingQuery {
                session,
                edit,
                query,
            } => match rt.modeling_record_response(client, session, edit, query)? {
                crate::ModelingRecordResponse::Inline(value) => {
                    let crate::ModelingRecordBatch {
                        source,
                        version,
                        state,
                        records,
                        lease,
                    } = value;
                    self.execution_leases.push(lease);
                    json!({"source":source,"version":version,"state":state,"records":records})
                }
                crate::ModelingRecordResponse::Blob(value) => json!(value),
            },
            Operation::RequestModelingSamples { input } => {
                json!({"job":rt.request_modeling_samples(client,input)?})
            }
            Operation::RequestSpatialSamples {
                session,
                edit,
                parameters,
                queries,
                expected,
            } => {
                json!({"job":rt.request_spatial_samples(client,session,edit,parameters,queries,expected)?})
            }
            Operation::RequestObservation {
                session,
                edit,
                parameters,
                view,
                expected,
            } => {
                json!({"job":rt.request_observation(client,session,edit,parameters,view,expected)?})
            }
            Operation::CreatePreview {
                session,
                edit,
                expected,
            } => {
                let (preview, source) = rt.create_preview(client, session, edit, expected)?;
                json!({"preview":preview,"serial":preview.serial(),"source":source})
            }
            Operation::PreviewState { preview } => {
                let (source, value) = rt.preview_state(client, preview)?;
                json!({"source":source,"value":value})
            }
            Operation::ApplyPreview { preview, command } => {
                let (source, value) = rt.apply_preview(client, preview, command)?;
                json!({"source":source,"value":value})
            }
            Operation::SamplePreview {
                view,
                queries,
                preview,
                times,
                geometry,
                incremental,
            } => {
                json!({"job":rt.request_preview_frames(client,preview,times,geometry,incremental,queries,view)?})
            }
            Operation::ReleasePreview { preview } => {
                rt.release_preview(client, preview)?;
                Value::Null
            }
            Operation::RequestModelingPlan {
                session,
                edit,
                intent,
            } => json!({"job":rt.request_modeling_plan(client,session,edit,intent)?}),
            Operation::ApplyModeling { edit, commands } => {
                rt.apply_modeling(client, edit, commands)?;
                Value::Null
            }
            Operation::ApplyAuthoring { edit, operations } => {
                json!(rt.apply_authoring(client, edit, operations)?)
            }
            Operation::PrepareImage {
                width,
                height,
                rgba,
            } => {
                if rgba.len() > crate::authoring::MAX_INLINE_IMAGE_BYTES {
                    return Err(RuntimeError::Budget("inline image upload"));
                }
                json!(rt.prepare_image(client, width, height, rgba)?)
            }
            Operation::PreparedPixels { image } => {
                let pixels = crate::authoring::PREPARED_PIXELS_QUERY.call(
                    rt,
                    client,
                    crate::authoring::PreparedPixelsQuery { image },
                )?;
                if pixels.rgba.len() > 65536 {
                    let blob = rt.retain_blob(client, pixels.rgba)?;
                    return Ok(success(
                        json!({"width": pixels.width, "height": pixels.height, "blob": blob}),
                    ));
                }
                self.execution_leases
                    .push(rt.reserve_transfer(pixels.rgba.len().saturating_mul(128))?);
                json!(pixels)
            }
            Operation::PrepareImageBlob {
                width,
                height,
                blob,
            } => json!(rt.prepare_image_blob(client, width, height, blob)?),
            Operation::PreparedPixelsBlob { image } => {
                let pixels = rt.prepared_image_pixels(client, image)?;
                let blob = rt.retain_blob(client, pixels.rgba)?;
                json!({"width": pixels.width, "height": pixels.height, "blob": blob})
            }
            Operation::ImageSnapshotBlob { session, edit, id } => {
                let value = rt.image_snapshot(client, session, edit, &id)?;
                let blob = rt.retain_blob(client, value.pixels.rgba)?;
                json!({"source": value.source, "id": value.id, "name": value.name, "pixels": {"width": value.pixels.width, "height": value.pixels.height}, "blob": blob})
            }
            Operation::ReleasePreparedImage { image } => {
                rt.release_prepared_image(client, image)?;
                Value::Null
            }
            Operation::ImageSnapshot { session, edit, id } => {
                let snapshot = crate::authoring::IMAGE_QUERY.call(
                    rt,
                    client,
                    crate::authoring::ImageQuery { session, id, edit },
                )?;
                if snapshot.pixels.rgba.len() > 65536 {
                    let blob = rt.retain_blob(client, snapshot.pixels.rgba)?;
                    return Ok(success(
                        json!({"source": snapshot.source, "id": snapshot.id, "name": snapshot.name,
                        "pixels": {"width": snapshot.pixels.width, "height": snapshot.pixels.height}, "blob": blob}),
                    ));
                }
                self.execution_leases
                    .push(rt.reserve_transfer(snapshot.pixels.rgba.len().saturating_mul(128))?);
                json!(snapshot)
            }
            Operation::PlanAuthoring {
                session,
                edit,
                intent,
            } => json!(crate::authoring::PLAN_QUERY.call(
                rt,
                client,
                crate::authoring::PlanQuery {
                    session,
                    edit,
                    intent
                }
            )?),
            Operation::MeshSnapshot(input) => {
                if let Some(value) = rt.mesh_response(
                    client,
                    input.session,
                    input.edit,
                    vec![input.id.clone()],
                    true,
                )? {
                    value
                } else {
                    let value = crate::authoring::MESH_QUERY.call(rt, client, input)?;
                    self.admit_value(rt, &value)?
                }
            }
            Operation::MeshSnapshots(input) => {
                if let Some(value) =
                    rt.mesh_response(client, input.session, input.edit, input.ids.clone(), false)?
                {
                    value
                } else {
                    let value = rt.mesh_batch(client, input)?;
                    self.admit_value(rt, &value)?
                }
            }
            Operation::ParameterSnapshot(input) => {
                let value = rt.parameter_snapshot(client, input)?;
                self.admit_value(rt, &value)?
            }
            Operation::MeshBindingSnapshot(input) => rt.binding_response(client, input)?,
            Operation::SceneSnapshot(input) => {
                let value = rt.scene_snapshot(client, input)?;
                self.admit_value(rt, &value)?
            }
            Operation::CaptureObjectRef { session, kind, id } => {
                json!(rt.capture_object_ref(client, session, kind, &id)?)
            }
            Operation::ResolveObjectRef { reference } => {
                json!(rt.resolve_object_ref(client, reference)?)
            }
            Operation::ObjectSceneCapabilities { reference } => {
                json!(rt.object_scene_capabilities(client, reference)?)
            }
            Operation::CaptureLabelAddress { reference } => {
                json!(rt.capture_label_address(client, reference)?)
            }
            Operation::PlanLabelPatch { address, name } => {
                json!(rt.plan_label_patch(client, address, name)?)
            }
            Operation::CaptureAddress { session, selector } => {
                let address = rt.capture_address(client, session, selector)?;
                json!({"address":address})
            }
            Operation::PlanAddressPatch { address, patch } => {
                json!(rt.plan_address_patch(client, address, patch)?)
            }
            Operation::ModelingCapabilities { reference, domain } => {
                let source = rt.resolve_object_ref(client, reference)?;
                json!({"source":source,"capabilities":rt.modeling_capabilities(client,reference,domain)?})
            }
            Operation::CollectionCapabilities { session, query } => {
                let version = rt.session_info(session)?.version;
                json!({"source":SourceRef::Formal{session,version},"capabilities":rt.collection_capabilities(client,session,query)?})
            }
            Operation::ReleaseObjectRef { reference } => {
                rt.release_object_ref(client, reference)?;
                Value::Null
            }
            Operation::ReleaseFieldAddress { address } => {
                rt.release_field_address(client, address)?;
                Value::Null
            }
            Operation::PlanSceneAuthoring {
                session,
                edit,
                operation,
            } => json!(rt.plan_scene_authoring(client, session, edit, operation)?),
            Operation::SampleModeling {
                session,
                edit,
                parameters,
                checks,
                expected,
            } => {
                let value = rt.sample_modeling(
                    client,
                    crate::authoring::ModelingSampleQuery {
                        session,
                        edit,
                        parameters,
                        checks,
                        expected,
                    },
                )?;
                rt.sample_response(client, &value)?
            }
            Operation::ApplyRgba {
                edit,
                id,
                name,
                width,
                height,
                rgba,
                replace,
            } => {
                rt.apply_rgba_image(client, edit, id, name, width, height, rgba, replace)?;
                Value::Null
            }
            Operation::BeginEdit {
                session,
                label,
                expected,
            } => json!(rt.begin_edit(client, session, label, expected)?),
            Operation::Apply { edit, commands } => {
                rt.apply_edit(client, edit, |candidate| {
                    for command in commands {
                        match command {
                            Command::CreateParameter { parameter } => {
                                candidate.create_parameter(parameter)?
                            }
                            Command::ReplaceParameter {
                                mut parameter,
                                preserve_runtime_id,
                                preserve_kind,
                                preserve_decimal_places,
                            } => {
                                if let Some(previous) =
                                    candidate.candidate_document().get_parameter(&parameter.id)
                                {
                                    if preserve_runtime_id {
                                        parameter.runtime_id = previous.runtime_id.clone();
                                    }
                                    if preserve_kind {
                                        parameter.kind = previous.kind;
                                    }
                                    if preserve_decimal_places {
                                        parameter.decimal_places = previous.decimal_places;
                                    }
                                }
                                candidate.replace_parameter(parameter)?
                            }
                            Command::CreateMesh { mesh } => candidate.create_mesh(mesh)?,
                            Command::RenameMesh { id, name } => candidate.rename_mesh(&id, name)?,
                            Command::UpdatePositions {
                                id,
                                vertex_ids,
                                positions,
                            } => candidate.update_positions(
                                &id,
                                &vertex_ids,
                                &positions
                                    .into_iter()
                                    .map(|p| Vec2::new(p[0], p[1]))
                                    .collect::<Vec<_>>(),
                            )?,
                            Command::EraseObject { id } => candidate.erase_object(&id)?,
                        }
                    }
                    Ok(())
                })?;
                Value::Null
            }
            Operation::Commit { edit } => json!({"job": rt.request_commit(client, edit)?}),
            Operation::ReleaseEdit { edit } => {
                rt.release_edit(client, edit)?;
                Value::Null
            }
            Operation::Undo { session } => json!({"job": rt.request_undo(client, session)?}),
            Operation::Redo { session } => json!({"job": rt.request_redo(client, session)?}),
            Operation::Save {
                session,
                path,
                expected,
            } => {
                if let Some(expected) = expected {
                    rt.session(session)?
                        .check_version(Some(expected), "save_project")?;
                }
                json!({"job": rt.request_save(client, session, &path)?})
            }
            Operation::Project { session, request } => {
                json!({"job":rt.request_project_target(client,session,request)?})
            }
            Operation::ImportProject { source } => json!({"job":rt.request_import(client,source)?}),
            Operation::Open { path } => match rt.request_open(client, &path)? {
                OpenRequest::Existing(s) => json!({"session": s}),
                OpenRequest::Pending(job) => json!({"job": job}),
            },
            Operation::Job { job } => {
                let info = rt.job_info(client, job)?;
                if let Some(Ok(value)) = &info.result {
                    if !matches!(
                        value,
                        JobValue::Sampled(_)
                            | JobValue::Previewed(_)
                            | JobValue::Spatial(_)
                            | JobValue::Observed(_)
                    ) {
                        self.execution_leases.push(
                            rt.reserve_transfer(value.retained_bytes().saturating_mul(32))
                                .map_err(|_| RuntimeError::Busy)?,
                        );
                    }
                }
                let value = if let Some(Ok(JobValue::Sampled(samples))) = &info.result {
                    // Serialize metadata only; sampled numeric arrays stay binary.
                    let mut metadata = info.clone();
                    metadata.result = None;
                    let mut value = job_info(&metadata);
                    value["result"] = success(rt.sample_response(client, samples)?);
                    value
                } else if let Some(Ok(JobValue::Previewed(samples))) = &info.result {
                    let mut metadata = info.clone();
                    metadata.result = None;
                    let mut value = job_info(&metadata);
                    value["result"] = success(rt.preview_response(client, samples)?);
                    value
                } else if let Some(Ok(JobValue::Observed(samples))) = &info.result {
                    let mut metadata = info.clone();
                    metadata.result = None;
                    let mut value = job_info(&metadata);
                    value["result"] = success(rt.observation_response(client, samples)?);
                    value
                } else if let Some(Ok(JobValue::Spatial(samples))) = &info.result {
                    let mut metadata = info.clone();
                    metadata.result = None;
                    let mut value = job_info(&metadata);
                    value["result"] = success(rt.spatial_response(client, samples)?);
                    value
                } else {
                    job_info(&info)
                };
                if info.state.is_terminal() {
                    rt.release_job(client, job)?;
                }
                value
            }
            Operation::ReleaseJob { job } => {
                rt.release_job(client, job)?;
                Value::Null
            }
            Operation::Subscribe { session } => {
                if let Some(session) = session {
                    let sub = rt.subscribe_info(session)?;
                    json!({"info": info(&sub.info), "cursor": sub.cursor})
                } else {
                    let sub = rt.subscribe_registry();
                    json!({"sessions": sub.sessions.iter().map(info).collect::<Vec<_>>(), "cursor": sub.cursor})
                }
            }
            Operation::Events { cursor, limit } => json!(rt
                .events_page(cursor, limit.unwrap_or(128))?
                .iter()
                .map(|e| event(e))
                .collect::<Vec<_>>()),
            Operation::ShowWindow { session } => {
                if !self.windows {
                    return Ok(failure(
                        "UNSUPPORTED_OPERATION",
                        "Host has no Editor windows",
                    ));
                }
                if let Some(s) = session {
                    rt.attach_session(client, s)?;
                }
                self.show_window = Some(session);
                Value::Null
            }
            Operation::RequestStatus { .. } => unreachable!(),
        };
        Ok(success(value))
    }
}
pub fn success(value: Value) -> Value {
    json!({"ok": value})
}
pub fn failure(code: &str, message: &str) -> Value {
    json!({"error": {"category": "runtime", "code": code, "message": message}})
}

/// Serialized while still on the owner thread, then held by an actual charged
/// Arc until the socket finishes. Cache eviction cannot release its accounting.
pub struct EncodedReply {
    pub bytes: std::sync::Arc<[u8]>,
    pub authenticated: Option<crate::local::Auth>,
    pub(crate) _transfer: zipzi_project::TransferReservation,
}
impl Protocol {
    pub fn receive_encoded(
        &mut self,
        runtime: &mut Runtime,
        auth: Option<crate::local::Auth>,
        value: Value,
        now: Instant,
    ) -> Option<EncodedReply> {
        let result = self.receive(runtime, auth, value, now);
        let authenticated = result
            .get("ok")
            .and_then(|ok| {
                serde_json::from_value(ok["client"].clone())
                    .ok()
                    .zip(ok["connection"].as_u64())
            })
            .map(|(client, connection)| crate::local::Auth { client, connection });
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(b.len())
                    .filter(|n| *n <= MAX_FRAME)
                    .ok_or_else(|| std::io::Error::other("response limit"))?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, &result).ok()?;
        // Failure closes the transport. An executed write is resolved using its
        // original RequestId; it is never mislabeled a known budget refusal.
        let reservation = runtime.reserve_transfer(count.0.saturating_mul(2)).ok()?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(count.0).ok()?;
        serde_json::to_writer(&mut bytes, &result).ok()?;
        let bytes: std::sync::Arc<[u8]> = bytes.into();
        drop(result);
        self.execution_leases.clear();
        Some(EncodedReply {
            bytes,
            authenticated,
            _transfer: reservation,
        })
    }
}

fn bounded_json_bytes<T: serde::Serialize>(value: &T) -> Result<usize> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= 1 << 20)
                .ok_or_else(|| std::io::Error::other("inline query bytes"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value)
        .map_err(|_| RuntimeError::Budget("inline query bytes"))?;
    Ok(count.0)
}
