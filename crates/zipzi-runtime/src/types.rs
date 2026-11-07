use std::sync::Arc;
use zipzi_project::TransferReservation;
use zipzi_sdk::{DraftSource, OpenReceipt, Publication, QuerySource, SaveReceipt, Status};

pub use zipzi_contract::kernel::{BlobId, ClientId, JobId, RuntimeInstanceId};

pub type Result<T> = std::result::Result<T, Status>;
pub(crate) fn error(code: &str, message: &str) -> Status {
    Status::error(code, message)
}

/// Task slots and actual temporary/result storage, not document-size multipliers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeLimits {
    pub clients: usize,
    pub sessions: usize,
    pub drafts: usize,
    pub jobs: usize,
    pub running_jobs: usize,
    pub events_per_session: usize,
    pub task_bytes: usize,
    pub result_bytes: usize,
}
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            clients: 64,
            sessions: 64,
            drafts: 256,
            jobs: 256,
            running_jobs: 4,
            events_per_session: 256,
            task_bytes: 1024 * 1024 * 1024,
            result_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Reservation is acquired before entering a worker and follows its actual
/// lifetime. Read output must fit within the requested result bound.
#[derive(Debug, Clone, Copy, Default)]
pub struct TaskBudget {
    pub workspace_bytes: usize,
    pub result_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}
impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Clones share both bytes and their accounting responsibility, even after the
/// job or Runtime is released. Published content has its own ResourceView.
#[derive(Clone)]
pub struct ReadResult {
    pub source: QuerySource,
    bytes: Arc<Vec<u8>>,
    _lease: Arc<TransferReservation>,
}
impl ReadResult {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(crate) fn new(
        source: QuerySource,
        bytes: Vec<u8>,
        lease: Arc<TransferReservation>,
    ) -> Self {
        Self {
            source,
            bytes: Arc::new(bytes),
            _lease: lease,
        }
    }
}
impl std::fmt::Debug for ReadResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadResult")
            .field("source", &self.source)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}
#[derive(Debug, Clone)]
pub enum JobValue {
    DraftUpdated(DraftSource),
    Published(Publication),
    Opened(OpenReceipt),
    Imported(ImportReceipt),
    Saved(SaveReceipt),
    Read(ReadResult),
    Exported(ExportReceipt),
}
#[derive(Debug, Clone)]
pub struct ImportReceipt {
    pub publication: Publication,
    pub warnings: Vec<String>,
    pub diagnostics: Vec<zipzi_project::ResourceDiagnostic>,
    pub name: Option<String>,
}
#[derive(Debug, Clone)]
pub struct JobInfo {
    pub id: JobId,
    pub client: ClientId,
    pub source: QuerySource,
    pub state: JobState,
    pub result: Option<Result<JobValue>>,
}

/// Captured on the owner thread together, with no publication between state and cursor.
#[derive(Debug)]
pub struct SessionSubscription {
    pub snapshot: zipzi_sdk::SourcedSnapshot,
    pub history: zipzi_sdk::HistoryState,
    pub cursor: zipzi_sdk::Version,
}

/// Publication origin belongs to the host event log, not persistent content.
#[derive(Debug, Clone)]
pub struct PublicationEvent {
    pub client: ClientId,
    pub publication: Publication,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionOverview {
    pub session: crate::SessionKey,
    pub version: crate::Version,
    pub document_id: String,
    pub path: Option<std::path::PathBuf>,
    pub dirty: bool,
    pub saving: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportReceipt {
    pub source: QuerySource,
    pub path: std::path::PathBuf,
    pub published: bool,
    pub durable: bool,
    pub warnings: Vec<String>,
    pub notices: Vec<String>,
}
