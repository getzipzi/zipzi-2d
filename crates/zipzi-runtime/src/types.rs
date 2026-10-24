use std::path::PathBuf;
use uuid::Uuid;
use zipzi_sdk::{AuthoringSnapshot, EditReceipt, SaveReceipt, SdkError, Version};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuntimeInstanceId(pub(crate) Uuid);

macro_rules! ids {
    ($($name:ident),*) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name {
            pub(crate) runtime: RuntimeInstanceId,
            pub(crate) serial: u64,
        }
        impl $name {
            pub fn runtime_instance(&self) -> RuntimeInstanceId { self.runtime }
        }
    )*};
}
ids!(SessionId, ClientId, EditId, JobId, OperationId);
impl From<EditId> for OperationId {
    fn from(id: EditId) -> Self {
        Self {
            runtime: id.runtime,
            serial: id.serial,
        }
    }
}
impl From<JobId> for OperationId {
    fn from(id: JobId) -> Self {
        Self {
            runtime: id.runtime,
            serial: id.serial,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Ready,
    Busy(JobId),
    Faulted(JobId),
    Closing,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: SessionId,
    pub state: SessionState,
    pub version: Version,
    pub modified: bool,
    pub history_lengths: (usize, usize),
    pub path: Option<PathBuf>,
    /// Retained independently of client-owned Job results for fault inspection.
    pub fault: Option<RuntimeError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditState {
    /// Bounded preflight workspace, not eligible for formal publication.
    Prepared,
    Editing,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub enum JobValue {
    Imported {
        session: SessionId,
        notices: Vec<String>,
        name: Option<String>,
    },
    Opened(SessionId),
    Saved(SaveReceipt),
    Published(std::sync::Arc<EditReceipt>),
}

#[derive(Debug, Clone)]
pub struct JobInfo {
    pub id: JobId,
    pub client: ClientId,
    pub session: SessionId,
    pub state: JobState,
    pub result: Option<Result<JobValue>>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error(transparent)]
    Sdk(#[from] SdkError),
    #[error("Unknown or expired runtime reference")]
    InvalidReference,
    #[error("Reference belongs to another client")]
    WrongOwner,
    #[error("Session or path is busy")]
    Busy,
    #[error("Session is faulted")]
    Faulted,
    #[error("Session has unsaved changes")]
    Modified,
    #[error("Transaction was aborted")]
    Aborted,
    #[error("Runtime budget exceeded: {0}")]
    Budget(&'static str),
    #[error("Event cursor requires resynchronization")]
    ResyncRequired,
    #[error("Operation cancelled before execution")]
    Cancelled,
    #[error("Worker could not start: {0}")]
    ExecutorFailure(String),
    #[error("Worker panicked: {0}")]
    WorkerPanic(String),
    #[error("Invalid project path: {0}")]
    Path(String),
}
pub type Result<T> = std::result::Result<T, RuntimeError>;

/// The first R1 source kinds. Candidate sequence distinguishes frames even if
/// the formal SDK version did not change. Snapshot/Proposal tokens arrive later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRef {
    Formal {
        session: SessionId,
        version: Version,
    },
    Edit {
        session: SessionId,
        edit: EditId,
        base: Version,
        sequence: u64,
    },
}

#[derive(Debug, Clone)]
pub struct SourceSnapshot {
    pub source: SourceRef,
    pub snapshot: std::sync::Arc<AuthoringSnapshot>,
    /// Busy and faulted snapshots are explicitly frozen, never current reads.
    pub state: SessionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRequest {
    Existing(SessionId),
    Pending(JobId),
}

#[derive(Debug, Clone)]
pub enum ImportSource {
    Model3(PathBuf),
    Psd(PathBuf),
    Cubism(PathBuf),
}
