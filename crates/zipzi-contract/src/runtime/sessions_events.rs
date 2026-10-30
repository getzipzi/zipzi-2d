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

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("{message}")]
    Export {
        message: String,
        report: Box<crate::project::ProjectReport>,
    },
    #[error(transparent)]
    Sdk(#[from] crate::authoring::SdkError),
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

#[derive(Debug, Clone, zipzi_api_derive::ApiFields)]
pub enum EventKind {
    Published(
        /// Confirmed edit publication receipt, including before/after versions and changes.
        Arc<EditReceipt>,
    ),
    Saved(
        /// Save receipt with persistence paths, warnings and durability status.
        SaveReceipt,
    ),
    StateChanged(
        /// Session metadata captured at this lifecycle state transition.
        SessionInfo,
    ),
    Created(#[doc = "Session metadata captured when the Session was created."] SessionInfo),
    Closed(#[doc = "Final Session metadata captured when the document closed."] SessionInfo),
}
#[derive(Debug, Clone, zipzi_api_derive::ApiFields)]
pub struct RuntimeEvent {
    /// Exact event-stream position captured for this event; cursors are scoped to Runtime and
    /// stream.
    pub cursor: EventCursor,
    /// Originating client identity; None means no client was attributed to the event.
    pub client: Option<ClientId>,
    /// Related Job identity; None means this event has no associated Job.
    pub job: Option<JobId>,
    /// Runtime operation identity used to correlate a published event with its original operation.
    pub operation: OperationId,
    /// Typed event payload; publication, save and lifecycle events have different payload types.
    pub kind: EventKind,
}
