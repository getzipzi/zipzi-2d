//! Wire-major 4 requests use exact immutable query and candidate sources.
use crate::{authoring::ObjectKind, kernel::*, kernel_commands::*, model::ModelSetup};
use serde::{Deserialize, Serialize};
use zipzi_api_derive::ApiShape;

#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Summary,
    Objects {
        addresses: Vec<ObjectAddress>,
    },
    Collection {
        kind: ObjectKind,
        offset: usize,
        limit: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    RuntimeCapabilities,
    RuntimeLimits,
    ListSessions,
    CreateSession {
        document_id: Option<String>,
        #[serde(default)]
        setup: ModelSetup,
    },
    AttachSession {
        session: SessionKey,
    },
    DetachSession {
        session: SessionKey,
    },
    SessionInfo {
        session: SessionKey,
    },
    CloseSession {
        session: SessionKey,
        discard_modified: bool,
    },
    BeginDraft {
        expected: Version,
    },
    Batch {
        expected: DraftSource,
        commands: Vec<Command>,
    },
    Preview {
        expected: DraftSource,
        commands: Vec<Command>,
    },
    BatchJob {
        expected: DraftSource,
        commands: Vec<Command>,
    },
    Publish {
        expected: DraftSource,
        description: String,
    },
    PublishJob {
        expected: DraftSource,
        description: String,
    },
    CancelDraft {
        draft: DraftId,
    },
    Query {
        source: QuerySource,
        input: Query,
    },
    Undo {
        expected: Version,
    },
    Redo {
        expected: Version,
    },
    Open {
        expected: Version,
        path: std::path::PathBuf,
    },
    Save {
        expected: Version,
    },
    SaveAs {
        expected: Version,
        path: std::path::PathBuf,
        fingerprint: Option<crate::authoring::ProjectFingerprint>,
    },
    Job {
        job: JobId,
    },
    CancelJob {
        job: JobId,
    },
    ReleaseJob {
        job: JobId,
    },
    Subscribe {
        session: SessionKey,
    },
    Events {
        cursor: Version,
    },
    ReleaseClient,
    RequestStatus {
        request_id: u64,
    },
    EditorMeshGenerate {
        input: serde_json::Value,
    },
    OpenProjects {
        paths: Vec<std::path::PathBuf>,
    },
    ShowWindow {
        session: Option<SessionKey>,
    },
}
