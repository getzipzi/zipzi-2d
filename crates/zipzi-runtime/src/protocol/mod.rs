//! Authenticated v4 requests and exactly-once recovery on the host owner thread.
mod project_queue;
pub use project_queue::ProjectOpenQueue;
mod binary;
mod values;
use crate::*;
pub use binary::{BinaryReply, BinarySlice};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub use zipzi_contract::kernel_wire::{Operation, Query};
pub use zipzi_contract::protocol::CONTRACT;
pub const SDK_BUILD: &str = env!("ZIPZI_RUNTIME_SDK_BUILD");
pub const MAX_FRAME: usize = 16 << 20;
pub const MAX_BINARY_CHUNK: usize = 1 << 20;
pub const MAX_BINARY_HEADER: usize = 16 << 10;

#[derive(Debug, Clone)]
pub struct ProtocolLimits {
    pub reconnect_grace: Duration,
    pub result_ttl: Duration,
    pub results_per_client: usize,
    pub result_bytes_per_client: usize,
    pub result_bytes_total: usize,
}
impl Default for ProtocolLimits {
    fn default() -> Self {
        Self {
            reconnect_grace: Duration::from_secs(30),
            result_ttl: Duration::from_secs(300),
            results_per_client: 1024,
            result_bytes_per_client: 8 << 20,
            result_bytes_total: 64 << 20,
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
struct Record {
    id: u64,
    digest: [u8; 32],
    bytes: Arc<[u8]>,
    at: Instant,
    payload: Option<BinarySlice>,
    _lease: zipzi_project::TransferReservation,
}
impl Record {
    fn retained_bytes(&self) -> usize {
        self.bytes
            .len()
            .saturating_add(self.payload.as_ref().map_or(0, BinarySlice::retained_bytes))
    }
}
struct RemoteClient {
    secret: String,
    connection: u64,
    disconnected: Option<Instant>,
    highest: u64,
    records: VecDeque<Record>,
    released: bool,
}
pub struct Protocol {
    limits: ProtocolLimits,
    clients: HashMap<ClientId, RemoteClient>,
    next_connection: u64,
    mesh_generator: Option<fn(Value) -> Value>,
    windows: bool,
    show_window: Option<Option<SessionKey>>,
    project_queue: Option<Arc<Mutex<ProjectOpenQueue>>>,
    binary_input: Option<Vec<u8>>,
    binary_output: Option<BinarySlice>,
}
impl Default for Protocol {
    fn default() -> Self {
        Self::new(ProtocolLimits::default())
    }
}
impl Protocol {
    pub fn new(limits: ProtocolLimits) -> Self {
        Self {
            limits,
            clients: HashMap::new(),
            next_connection: 0,
            mesh_generator: None,
            windows: false,
            show_window: None,
            project_queue: None,
            binary_input: None,
            binary_output: None,
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
    pub fn with_project_open_support(mut self, queue: Arc<Mutex<ProjectOpenQueue>>) -> Self {
        self.project_queue = Some(queue);
        self
    }
    pub fn take_window_request(&mut self) -> Option<Option<SessionKey>> {
        self.show_window.take()
    }
    pub fn hello(&mut self, runtime: &mut Runtime, value: Value, now: Instant) -> Value {
        self.expire(runtime, now);
        if value["protocol"] != CONTRACT {
            return failure("PROTOCOL_MISMATCH", "Incompatible authoring wire major");
        }
        let hello: Hello = match serde_json::from_value(value) {
            Ok(v) => v,
            Err(e) => return failure("INVALID_HANDSHAKE", &e.to_string()),
        };
        debug_assert_eq!(hello.protocol, CONTRACT);
        if hello.sdk_build.as_deref().is_some_and(|s| s != SDK_BUILD) {
            return failure("BUILD_MISMATCH", "Requested build unavailable");
        }
        if hello.runtime.is_some_and(|s| s != runtime.instance_id()) {
            return failure("HOST_RESTARTED", "Original Runtime is unavailable");
        }
        let Some(connection) = self.next_connection.checked_add(1) else {
            return failure("RESOURCE_LIMIT", "Connection identity exhausted");
        };
        let client = if let Some(client) = hello.client {
            if hello.runtime.is_none() || hello.secret.is_none() {
                return failure("INVALID_HANDSHAKE", "Resume requires instance and secret");
            }
            let Some(remote) = self.clients.get(&client) else {
                return failure("CLIENT_EXPIRED", "Reconnect grace expired");
            };
            if hello.secret.as_deref() != Some(&remote.secret) {
                return failure("WRONG_OWNER", "Invalid resume secret");
            }
            client
        } else {
            let client = match runtime.register_client() {
                Ok(v) => v,
                Err(e) => return status(e),
            };
            self.clients.insert(
                client,
                RemoteClient {
                    secret: uuid::Uuid::new_v4().to_string(),
                    connection,
                    disconnected: None,
                    highest: 0,
                    records: VecDeque::new(),
                    released: false,
                },
            );
            client
        };
        self.next_connection = connection;
        let remote = self.clients.get_mut(&client).unwrap();
        remote.connection = connection;
        remote.disconnected = None;
        let secret = remote.secret.clone();
        success(
            json!({"protocol":CONTRACT, "runtime":runtime.instance_id(), "client":client,
            "secret":secret,"connection":connection,"sdk_build":SDK_BUILD,"host_pid":std::process::id(),
            "capabilities":self.capabilities(),"limits":{"frame_bytes":MAX_FRAME,
                "reconnect_seconds":self.limits.reconnect_grace.as_secs_f64(),"result_seconds":self.limits.result_ttl.as_secs_f64(),
                "results_per_client":self.limits.results_per_client,"result_bytes_per_client":self.limits.result_bytes_per_client,
                "result_bytes_total":self.limits.result_bytes_total}}),
        )
    }
    fn capabilities(&self) -> Vec<&str> {
        let mut out = vec![
            "immutable_authoring",
            "typed_commands",
            "sourced_queries",
            "isolated_drafts",
            "sessions",
            "jobs",
            "history",
            "native_project_io",
            "project_import",
            "events",
            "request_recovery",
            "binary_blobs",
            "spatial_samples",
            "animation_samples",
            "project_export",
        ];
        if cfg!(feature = "gpu-observation") {
            out.push("gpu_observation");
        }
        if self.mesh_generator.is_some() {
            out.push("editor_mesh_generate");
        }
        if self.windows {
            out.push("show_window");
            if self.project_queue.is_some() {
                out.push("open_projects");
            }
        }
        out
    }
    pub fn receive(
        &mut self,
        runtime: &mut Runtime,
        auth: Option<crate::local::Auth>,
        value: Value,
        now: Instant,
    ) -> Value {
        match auth {
            None => self.hello(runtime, value, now),
            Some(auth) => match serde_json::from_value(value) {
                Ok(request) => self.handle(runtime, auth.client, auth.connection, request, now),
                Err(e) => failure("INVALID_REQUEST", &e.to_string()),
            },
        }
    }
    pub fn disconnect(&mut self, client: ClientId, connection: u64, now: Instant) {
        if let Some(c) = self
            .clients
            .get_mut(&client)
            .filter(|c| c.connection == connection && c.disconnected.is_none())
        {
            c.disconnected = Some(now);
        }
    }
    pub fn has_connected_clients(&self) -> bool {
        self.clients.values().any(|c| c.disconnected.is_none())
    }
    pub fn release_disconnected_clients(&mut self, runtime: &mut Runtime) {
        self.clients.retain(|id, c| {
            if c.disconnected.is_some() {
                runtime.release_client(*id);
                false
            } else {
                true
            }
        });
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
    }
    pub fn retained_results(&self) -> (usize, usize) {
        self.clients
            .values()
            .flat_map(|c| &c.records)
            .fold((0, 0), |(n, b), r| (n + 1, b + r.retained_bytes()))
    }
    pub fn retained_memory_bytes(&self) -> usize {
        self.retained_results().1
    }
    pub fn handle(
        &mut self,
        runtime: &mut Runtime,
        client: ClientId,
        connection: u64,
        request: Request,
        now: Instant,
    ) -> Value {
        self.binary_output = None;
        self.expire(runtime, now);
        if request.protocol != CONTRACT {
            return failure("PROTOCOL_MISMATCH", "Incompatible authoring wire major");
        }
        if request.runtime != runtime.instance_id() {
            return failure("HOST_RESTARTED", "Original Runtime unavailable");
        }
        if request.client != client {
            return failure("WRONG_OWNER", "Request differs from authenticated client");
        }
        let Some(remote) = self
            .clients
            .get(&client)
            .filter(|c| c.connection == connection && c.disconnected.is_none())
        else {
            return failure("CLIENT_EXPIRED", "Connection replaced or expired");
        };
        if request.operation["op"] == "request_status" {
            return match serde_json::from_value::<Operation>(request.operation) {
                Ok(Operation::RequestStatus { request_id }) => {
                    if let Some(record) = remote.records.iter().find(|r| r.id == request_id) {
                        success(
                            json!({"state":"completed","response":serde_json::from_slice::<Value>(&record.bytes).unwrap()}),
                        )
                    } else if request_id <= remote.highest {
                        failure("REQUEST_EXPIRED", "Original result expired; never replay")
                    } else {
                        success(json!({"state":"not_executed"}))
                    }
                }
                _ => failure("INVALID_REQUEST", "Invalid recovery request"),
            };
        }
        let digest: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&request.operation).unwrap()).into();
        if let Some(record) = remote.records.iter().find(|r| r.id == request.request_id) {
            return if record.digest == digest {
                self.binary_output = record.payload.clone();
                serde_json::from_slice(&record.bytes).unwrap()
            } else {
                failure("REQUEST_ID_REUSED", "Request ID has different arguments")
            };
        }
        if request.request_id == 0 || request.request_id <= remote.highest {
            return failure("REQUEST_EXPIRED", "Old request will not execute again");
        }
        // A resumed released client may recover its final response but cannot
        // acquire new Sessions, Drafts or jobs under the retired identity.
        if remote.released {
            return failure("CLIENT_RELEASED", "Client ownership has been released");
        }
        let result = match serde_json::from_value::<Operation>(request.operation) {
            Ok(op) => self.execute_response(runtime, client, op),
            Err(e) => failure("INVALID_REQUEST", &e.to_string()),
        };
        // Advance even when output cannot be retained. Never re-execute a mutation
        // merely because its response expired or failed allocation/serialization.
        let remote = self.clients.get_mut(&client).unwrap();
        remote.highest = request.request_id;
        if let Ok(bytes) = encode_bounded(&result) {
            let retained = bytes.len().saturating_add(
                self.binary_output
                    .as_ref()
                    .map_or(0, BinarySlice::retained_bytes),
            );
            if retained <= self.limits.result_bytes_per_client
                && retained <= self.limits.result_bytes_total
            {
                if let Ok(lease) = runtime.reserve_transfer(bytes.len()) {
                    remote.records.push_back(Record {
                        id: request.request_id,
                        digest,
                        bytes: bytes.into(),
                        at: now,
                        payload: self.binary_output.clone(),
                        _lease: lease,
                    });
                }
            }
        }
        while remote.records.len() > self.limits.results_per_client
            || remote
                .records
                .iter()
                .map(Record::retained_bytes)
                .sum::<usize>()
                > self.limits.result_bytes_per_client
        {
            remote.records.pop_front();
        }
        while self.retained_results().1 > self.limits.result_bytes_total {
            let oldest = self
                .clients
                .iter()
                .filter_map(|(id, c)| c.records.front().map(|r| (*id, r.at)))
                .min_by_key(|(_, at)| *at)
                .map(|(id, _)| id)
                .unwrap();
            self.clients.get_mut(&oldest).unwrap().records.pop_front();
        }
        result
    }
    /// Response envelope shared by Local and Connected clients. Draft errors carry
    /// the post-operation lifecycle, which is retained with replayable responses.
    pub fn execute_response(&mut self, rt: &mut Runtime, client: ClientId, op: Operation) -> Value {
        let draft = match &op {
            Operation::Batch { expected, .. }
            | Operation::Preview { expected, .. }
            | Operation::BatchJob { expected, .. }
            | Operation::Publish { expected, .. }
            | Operation::PublishJob { expected, .. }
            | Operation::ReplaceImageBlob { expected, .. } => Some(expected.draft),
            Operation::Query {
                source: QuerySource::Draft(expected),
                ..
            } => Some(expected.draft),
            Operation::CancelDraft { draft } => Some(*draft),
            _ => None,
        };
        match self.execute(rt, client, op) {
            Ok(value) => success(value),
            Err(error) => {
                let mut response = status(error);
                if let Some(state) = draft.and_then(|id| rt.draft_state(client, id)) {
                    response["error"]["draft_state"] = json!(state);
                }
                response
            }
        }
    }
    pub fn execute(&mut self, rt: &mut Runtime, client: ClientId, op: Operation) -> Result<Value> {
        use Operation::*;
        Ok(match op {
            Sample { source, input } => json!(rt.request_samples(client, source, input)?),
            Observe {
                source,
                input,
                view,
            } => json!(rt.request_observation(client, source, input, view)?),
            Export {
                source,
                path,
                format,
                allow_loss,
            } => json!(rt.request_export(client, source, &path, format, allow_loss)?),
            CreateBlob { length } => json!(rt.create_blob(client, length)?),
            SealBlob { blob } => json!(rt.seal_blob(client, blob)?),
            ReleaseBlob { blob } => {
                rt.release_blob(client, blob)?;
                Value::Null
            }
            AppendBlobBinary {
                blob,
                offset,
                length,
                payload_sha256,
            } => {
                let bytes = self
                    .binary_input
                    .take()
                    .ok_or_else(|| error("INVALID_REQUEST", "Binary frame required"))?;
                binary::validate_payload(length, &payload_sha256, &bytes)?;
                json!(rt.append_blob(client, blob, offset, &bytes)?)
            }
            ReadBlobBinary {
                blob,
                offset,
                length,
            } => {
                if self.binary_input.take().is_none() {
                    return Err(error("INVALID_REQUEST", "Binary frame required"));
                }
                self.binary_output = Some(BinarySlice::new(
                    rt.read_blob_range(client, blob, offset, length)?,
                    0,
                    length,
                )?);
                json!({"length":length})
            }
            ReplaceImageBlob {
                expected,
                id,
                blob,
                width,
                height,
            } => {
                json!({"source":rt.replace_image_blob(client, expected, id, blob, width, height)?,"outcomes":[]})
            }
            ReadImageBlob { source, id } => rt.read_image_blob(client, source, &id)?,
            ReadJobBlob { job } => rt.read_job_blob(client, job)?,
            EditorContext { expected } => json!(rt.editor_context(client, expected)?),
            RuntimeCapabilities => json!(self.capabilities()),
            RuntimeLimits => json!(rt.limits()),
            ListSessions => json!(rt.list_sessions(client)?),
            CreateSession { document_id, setup } => {
                let id = document_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let session = rt.create_session(client, &id, setup)?;
                values::session(rt, client, session)?
            }
            AttachSession { session } => {
                rt.attach(client, session)?;
                values::session(rt, client, session)?
            }
            DetachSession { session } => {
                rt.detach(client, session)?;
                Value::Null
            }
            SessionInfo { session } => values::session(rt, client, session)?,
            CloseSession {
                session,
                discard_modified,
            } => {
                rt.close_session(client, session, discard_modified)?;
                Value::Null
            }
            BeginDraft { expected } => json!(rt.begin_draft(client, expected)?),
            Batch { expected, commands } => {
                let (source, outcomes) = rt.commands(client, expected, commands)?;
                json!({"source":source,"outcomes":outcomes})
            }
            BatchJob { expected, commands } => json!(rt.command_job(client, expected, commands)?),
            Preview { expected, commands } => {
                let (source, outcomes) = rt.preview_commands(client, expected, commands)?;
                json!({"source":source,"outcomes":outcomes})
            }
            Publish {
                expected,
                description,
            } => values::publication(&rt.publish(client, expected, description)?),
            PublishJob {
                expected,
                description,
            } => json!(rt.request_publish(client, expected, description, TaskBudget::default())?),
            CancelDraft { draft } => {
                rt.cancel_draft(client, draft)?;
                Value::Null
            }
            Query { source, input } => rt.query(client, source, input)?,
            Undo { expected } => values::publication(&rt.undo(client, expected)?),
            Redo { expected } => values::publication(&rt.redo(client, expected)?),
            Open { expected, path } => {
                let io = rt.project_io().clone();
                json!(rt.request_open(client, expected, &io, &path, TaskBudget::default())?)
            }
            Import {
                expected,
                path,
                format,
            } => {
                json!(rt.request_import(client, expected, &path, format, TaskBudget::default())?)
            }
            Save { expected } => {
                let io = rt.project_io().clone();
                json!(rt.request_save(client, expected, &io, TaskBudget::default())?)
            }
            SaveAs {
                expected,
                path,
                fingerprint,
            } => {
                let io = rt.project_io().clone();
                json!(rt.request_save_as(
                    client,
                    expected,
                    &io,
                    &path,
                    fingerprint.as_ref(),
                    TaskBudget::default()
                )?)
            }
            Job { job } => values::job(rt.job(client, job)?),
            CancelJob { job } => {
                rt.cancel_job(client, job)?;
                Value::Null
            }
            ReleaseJob { job } => {
                rt.release_job(client, job)?;
                Value::Null
            }
            Subscribe { session } => {
                let sub = rt.subscribe(client, session)?;
                json!({"info":values::session(rt, client, session)?,"cursor":sub.cursor})
            }
            Events { cursor } => values::publications(
                &rt.publications_since(client, cursor)?,
                rt.limits().result_bytes.min(MAX_FRAME),
            )?,
            ReleaseClient => {
                rt.release_client(client);
                if let Some(remote) = self.clients.get_mut(&client) {
                    remote.released = true;
                }
                Value::Null
            }
            EditorMeshGenerate { input } => {
                let f = self.mesh_generator.ok_or_else(|| {
                    error(
                        "UNSUPPORTED_OPERATION",
                        "Host has no mesh generation service",
                    )
                })?;
                let reply = f(input);
                if let Some(value) = reply.get("ok") {
                    value.clone()
                } else {
                    return Err(error(
                        reply["error"]["code"]
                            .as_str()
                            .unwrap_or("GENERATION_FAILED"),
                        reply["error"]["message"]
                            .as_str()
                            .unwrap_or("Mesh generation failed"),
                    ));
                }
            }
            OpenProjects { paths } => {
                self.project_queue
                    .as_ref()
                    .ok_or_else(|| error("UNSUPPORTED_OPERATION", "No project queue"))?
                    .lock()
                    .unwrap()
                    .enqueue(paths)
                    .map_err(|e| error("OPEN_QUEUE_LIMIT", e))?;
                Value::Null
            }
            ShowWindow { session } => {
                if !self.windows {
                    return Err(error("UNSUPPORTED_OPERATION", "Host has no windows"));
                }
                if let Some(s) = session {
                    rt.attach(client, s)?;
                }
                self.show_window = Some(session);
                Value::Null
            }
            RequestStatus { .. } => {
                return Err(error(
                    "INVALID_REQUEST",
                    "Request recovery requires an authenticated connection",
                ))
            }
        })
    }
}
pub fn success(value: Value) -> Value {
    json!({"ok":value})
}
pub fn failure(code: &str, message: &str) -> Value {
    json!({"error":{"category":"runtime","code":code,"message":message}})
}
fn status(e: zipzi_sdk::Status) -> Value {
    failure(&e.code, &e.message)
}

pub struct EncodedReply {
    pub bytes: Arc<[u8]>,
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
            .and_then(|v| {
                serde_json::from_value(v["client"].clone())
                    .ok()
                    .zip(v["connection"].as_u64())
            })
            .map(|(client, connection)| crate::local::Auth { client, connection });
        let bytes = encode_bounded(&result).ok()?;
        let transfer = runtime.reserve_transfer(bytes.capacity()).ok()?;
        Some(EncodedReply {
            bytes: bytes.into(),
            authenticated,
            _transfer: transfer,
        })
    }
}
fn encode_bounded(value: &impl Serialize) -> std::io::Result<Vec<u8>> {
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self
                .0
                .len()
                .checked_add(b.len())
                .is_none_or(|n| n > MAX_FRAME)
            {
                return Err(std::io::Error::other("Frame exceeds limit"));
            }
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded(vec![]);
    serde_json::to_writer(&mut out, value)?;
    Ok(out.0)
}

#[cfg(test)]
mod tests;
