//! Wire-major 4 requests use exact immutable query and candidate sources.
use crate::{authoring::ObjectKind, kernel::*, kernel_commands::*, model::ModelSetup};
use serde::{Deserialize, Serialize};
use zipzi_api_derive::ApiShape;

#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
pub struct EditorContext {
    pub version: Version,
    pub view_id: String,
    pub selected_ids: Vec<String>,
    pub workspace: String,
    pub editing_context: String,
    pub active_tool: String,
    pub pending_interaction: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ApiShape)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    Model3,
    Psd,
    Cubism,
}

/// Stable collection selector, interpreted only within its accompanying QuerySource.
#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(
    tag = "domain",
    content = "target",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CollectionTarget {
    Organization(crate::document::OrganizationCollection),
    Scene(crate::document::SceneCollection),
    References(crate::document::ReferenceCollection),
    Pose(crate::document::PoseCollection),
    Physics(crate::authoring::PhysicsCollection),
    Motion(crate::document::MotionContainer),
    MotionGroups,
    Clips,
    Expressions,
    ExpressionEntries { expression_id: String },
    Resources,
    Attachments,
}

#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    CollectionCapabilities {
        target: CollectionTarget,
    },
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
    Sample {
        source: QuerySource,
        input: crate::kernel_tasks::SampleRequest,
    },
    Observe {
        source: QuerySource,
        input: crate::kernel_tasks::SampleInput,
        view: crate::observation::ObservationView,
    },
    Export {
        source: QuerySource,
        path: std::path::PathBuf,
        format: crate::kernel_tasks::ExportFormat,
        #[serde(default)]
        allow_loss: bool,
    },
    CreateBlob {
        length: usize,
    },
    AppendBlobBinary {
        blob: BlobId,
        offset: usize,
        length: usize,
        payload_sha256: String,
    },
    SealBlob {
        blob: BlobId,
    },
    ReadBlobBinary {
        blob: BlobId,
        offset: usize,
        length: usize,
    },
    ReleaseBlob {
        blob: BlobId,
    },
    ReplaceImageBlob {
        expected: DraftSource,
        id: String,
        blob: BlobId,
        width: u32,
        height: u32,
    },
    ReadImageBlob {
        source: QuerySource,
        id: String,
    },
    ReadJobBlob {
        job: JobId,
    },
    EditorContext {
        expected: Version,
    },
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
    Import {
        expected: Version,
        path: std::path::PathBuf,
        format: ImportFormat,
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
