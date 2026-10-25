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

mod modeling_query;
mod values;
use values::*;
pub const CONTRACT: &str = "zipzi-runtime-r4-1";
pub const SDK_BUILD: &str = env!("ZIPZI_RUNTIME_SDK_BUILD");
pub const MAX_FRAME: usize = 16 << 20;

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
#[serde(deny_unknown_fields)]
struct Hello {
    protocol: String,
    runtime: Option<RuntimeInstanceId>,
    client: Option<ClientId>,
    secret: Option<String>,
    sdk_build: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    ListSessions,
    Statistics,
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
        document_id: String,
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
    ApplyModelingBlob {
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
    bytes: usize,
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
    limits: ProtocolLimits,
    clients: HashMap<ClientId, RemoteClient>,
    next_connection: u64,
    show_window: Option<Option<SessionId>>,
    windows: bool,
}
impl Default for Protocol {
    fn default() -> Self {
        Self::new(ProtocolLimits::default())
    }
}
impl Protocol {
    #[cfg(unix)]
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
    pub fn with_window_support(mut self) -> Self {
        self.windows = true;
        self
    }
    pub fn new(limits: ProtocolLimits) -> Self {
        Self {
            limits,
            clients: HashMap::new(),
            next_connection: 0,
            show_window: None,
            windows: false,
        }
    }
    /// Hello is the first frame on every socket. No Runtime registration or writes
    /// occur until the complete protocol/build/old-instance check has passed.
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
            "typed_mesh_batch",
            "typed_mesh_bindings",
            "typed_modeling_samples",
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
        if self.windows {
            capabilities.push("show_window");
        }
        success(
            json!({"runtime": runtime.instance_id(), "client": client, "secret": remote.secret,
            "connection": remote.connection, "sdk_build": SDK_BUILD, "protocol": CONTRACT, "host_pid": std::process::id(),
            "capabilities": capabilities,
            "limits": {"frame_bytes": MAX_FRAME, "reconnect_seconds": self.limits.reconnect_grace.as_secs_f64(),
                "result_seconds": self.limits.result_ttl.as_secs_f64(), "results_per_client": self.limits.results_per_client,
                "result_bytes_per_client": self.limits.result_bytes_per_client, "result_bytes_total": self.limits.result_bytes_total}}),
        )
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
        let bytes = crate::metrics::encode(&result).unwrap().len() + 128;
        let c = self.clients.get_mut(&client).unwrap();
        c.highest = request.request_id;
        c.records.push_back(Record {
            id: request.request_id,
            digest,
            result: result.clone(),
            bytes,
            at: now,
        });
        while c.records.len() > self.limits.results_per_client
            || c.records.iter().map(|r| r.bytes).sum::<usize>()
                > self.limits.result_bytes_per_client
        {
            c.records.pop_front();
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
    fn execute(
        &mut self,
        rt: &mut Runtime,
        client: ClientId,
        op: Operation,
    ) -> crate::Result<Value> {
        let value = match op {
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
            Operation::Statistics => {
                let (clients, sessions, edits, jobs) = rt.live_counts();
                let (results, result_bytes) = self.retained_results();
                let mut value = json!({"clients": clients, "sessions": sessions, "edits": edits, "jobs": jobs,
                    "request_results": results, "request_result_bytes": result_bytes,
                    "accounted_document_bytes": rt.accounted_bytes().saturating_sub(rt.image_memory_usage().total_bytes()),
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
                json!(rt.create_session(client, &document_id, setup)?)
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
                json!(
                    data.get(offset..end)
                        .ok_or(RuntimeError::InvalidReference)?
                )
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
            Operation::ApplyModelingBlob { edit, blob } => {
                let bytes = rt.blob(client, blob)?;
                let commands: Vec<crate::modeling::ModelingCommand> =
                    crate::metrics::decode(&bytes).map_err(|_| RuntimeError::InvalidReference)?;
                rt.apply_modeling(client, edit, commands)?;
                Value::Null
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
            } => {
                let frozen;
                let session_info = rt.session_info(session)?;
                let (doc, source) = if let Some(edit) = edit {
                    let candidate = rt.candidate(client, edit)?;
                    if candidate.base_version().session_id != session_info.version.session_id {
                        return Err(RuntimeError::InvalidReference);
                    }
                    (
                        rt.edit_document(client, edit)?,
                        SourceRef::Edit {
                            session,
                            edit,
                            base: candidate.base_version(),
                            sequence: rt.edit_sequence(client, edit)?,
                        },
                    )
                } else if matches!(session_info.state, SessionState::Ready) {
                    (
                        rt.document(session)?,
                        SourceRef::Formal {
                            session,
                            version: session_info.version,
                        },
                    )
                } else {
                    frozen = rt.source_snapshot(session)?;
                    (frozen.snapshot.document(), frozen.source)
                };
                modeling_query::response(
                    doc,
                    &query,
                    source,
                    &session_info,
                    rt.limits()
                        .accounted_bytes
                        .saturating_sub(rt.accounted_bytes())
                        .saturating_sub(
                            self.retained_results()
                                .1
                                .saturating_mul(modeling_query::JSON_MEMORY_FACTOR),
                        ),
                    self.limits
                        .result_bytes_per_client
                        .min(self.limits.result_bytes_total),
                )?
            }
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
            } => json!(rt.prepare_image(client, width, height, rgba)?),
            Operation::PreparedPixels { image } => {
                json!(crate::authoring::PREPARED_PIXELS_QUERY.call(
                    rt,
                    client,
                    crate::authoring::PreparedPixelsQuery { image }
                )?)
            }
            Operation::ReleasePreparedImage { image } => {
                rt.release_prepared_image(client, image)?;
                Value::Null
            }
            Operation::ImageSnapshot { session, edit, id } => {
                json!(crate::authoring::IMAGE_QUERY.call(
                    rt,
                    client,
                    crate::authoring::ImageQuery { session, id, edit }
                )?)
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
                json!(crate::authoring::MESH_QUERY.call(rt, client, input)?)
            }
            Operation::MeshSnapshots(input) => json!(rt.mesh_snapshots(client, input)?),
            Operation::ParameterSnapshot(input) => json!(rt.parameter_snapshot(client, input)?),
            Operation::MeshBindingSnapshot(input) => {
                json!(rt.mesh_binding_snapshot(client, input)?)
            }
            Operation::SampleModeling {
                session,
                edit,
                parameters,
                checks,
                expected,
            } => {
                json!(rt.sample_modeling(
                    client,
                    crate::authoring::ModelingSampleQuery {
                        session,
                        edit,
                        parameters,
                        checks,
                        expected,
                    }
                )?)
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
            Operation::Open { path } => match rt.request_open(client, &path)? {
                OpenRequest::Existing(s) => json!({"session": s}),
                OpenRequest::Pending(job) => json!({"job": job}),
            },
            Operation::Job { job } => {
                let info = rt.job_info(client, job)?;
                let value = job_info(&info);
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
            Operation::Events { cursor, limit } => json!(
                rt.events_page(cursor, limit.unwrap_or(128))?
                    .iter()
                    .map(|e| event(e))
                    .collect::<Vec<_>>()
            ),
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
