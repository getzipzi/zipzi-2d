use std::path::PathBuf;
use uuid::Uuid;
use zipzi_sdk::{AuthoringSnapshot, EditReceipt, SaveReceipt, SdkError, Version};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct RuntimeInstanceId(pub(crate) Uuid);
impl std::fmt::Display for RuntimeInstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::str::FromStr for RuntimeInstanceId {
    type Err = uuid::Error;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

macro_rules! ids {
    ($($name:ident),*) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        #[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiFields)]
        pub struct $name {
            /// Runtime instance that allocated this handle; handles are invalid across instances.
            pub(crate) runtime: RuntimeInstanceId,
            /// Runtime-allocated serial, meaningful only together with the instance and handle kind.
            pub(crate) serial: u64,
        }
        impl zipzi_core::api_shape::ApiShape for $name {
            fn api_shape() -> zipzi_core::api_shape::Shape {
                zipzi_core::api_shape::Shape::Opaque { name: concat!(module_path!(), "::", stringify!($name)).into() }
            }
        }
        impl $name {
            /// Construct an identity. Runtime still validates existence and ownership.
            pub fn from_parts(runtime: RuntimeInstanceId, serial: u64) -> Self { Self { runtime, serial } }
            pub fn runtime_instance(&self) -> RuntimeInstanceId { self.runtime }
            pub fn serial(&self) -> u64 { self.serial }
        }
    )*};
}
ids!(
    SessionId,
    ClientId,
    EditId,
    JobId,
    OperationId,
    PlanId,
    ImageId,
    BlobId,
    ObjectRefId,
    FieldAddressId
);
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

#[derive(Debug, Clone)]
pub enum JobValue {
    PreparedImage(std::sync::Arc<zipzi_sdk::PreparedImageContent>),
    ImagePrepared(crate::authoring::PreparedImageInfo),
    Project {
        session: SessionId,
        report: std::sync::Arc<crate::project_io::ProjectReport>,
    },
    Sampled(std::sync::Arc<crate::authoring::ModelingSampleBatch>),
    PreparedPlan(std::sync::Arc<crate::PreparedModelingPlan>),
    Planned(std::sync::Arc<crate::authoring::PlanSnapshot>),
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
    #[error("{message}")]
    Export {
        message: String,
        report: Box<crate::project_io::ProjectReport>,
    },
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
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

impl SourceRef {
    pub fn session(self) -> SessionId {
        match self {
            Self::Formal { session, .. } | Self::Edit { session, .. } => session,
        }
    }
    pub fn version(self) -> Version {
        match self {
            Self::Formal { version, .. } => version,
            Self::Edit { base, .. } => base,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceSnapshot {
    pub(crate) _lease: std::sync::Arc<zipzi_project::TransferReservation>,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "format", content = "path", rename_all = "snake_case")]
pub enum ImportSource {
    Model3(PathBuf),
    Psd(PathBuf),
    Cubism(PathBuf),
}

impl zipzi_core::api_shape::ApiShape for SourceRef {
    fn api_shape() -> zipzi_core::api_shape::Shape {
        zipzi_core::api_shape::Shape::Opaque {
            name: concat!(module_path!(), "::SourceRef").into(),
        }
    }
}

impl zipzi_core::binary_wire::WireWrite for RuntimeInstanceId {
    const SIGNATURE: u128 = zipzi_core::binary_wire::signature("runtime-instance-uuid-v1");
    fn encode<W: std::io::Write>(&self, out: &mut W) -> zipzi_core::binary_wire::Result<()> {
        out.write_all(self.0.as_bytes())?;
        Ok(())
    }
    fn expansion_bytes(&self) -> usize {
        128
    }
}
impl zipzi_core::binary_wire::WireRead for RuntimeInstanceId {
    fn input_cost(
        input: &mut zipzi_core::binary_wire::Reader<'_>,
    ) -> zipzi_core::binary_wire::Result<usize> {
        input.take(16)?;
        Ok(16)
    }
    fn decode(
        input: &mut zipzi_core::binary_wire::Reader<'_>,
    ) -> zipzi_core::binary_wire::Result<Self> {
        Ok(Self(Uuid::from_bytes(input.take(16)?.try_into().unwrap())))
    }
}

impl JobValue {
    pub(crate) fn retained_bytes(&self) -> usize {
        use zipzi_core::binary_wire::WireWrite;
        match self {
            Self::Project { report, .. } => report.expansion_bytes(),
            Self::Sampled(samples) => samples.samples.expansion_bytes(),
            Self::Planned(plan) => plan.report.expansion_bytes(),
            Self::PreparedPlan(plan) => plan.bytes,
            Self::PreparedImage(_) | Self::ImagePrepared(_) | Self::Opened(_) => 1024,
            Self::Published(receipt) => receipt.estimated_bytes().saturating_mul(4),
            Self::Saved(receipt) => receipt
                .manifest
                .as_os_str()
                .len()
                .saturating_add(
                    receipt
                        .warnings
                        .iter()
                        .chain(&receipt.history_warnings)
                        .map(|s| s.capacity().saturating_add(64))
                        .sum::<usize>(),
                )
                .saturating_mul(4)
                .saturating_add(1024),
            Self::Imported { notices, name, .. } => notices
                .iter()
                .chain(name.iter())
                .map(|s| s.capacity().saturating_add(64))
                .sum::<usize>()
                .saturating_mul(4)
                .saturating_add(1024),
        }
    }
}

impl RuntimeError {
    pub(crate) fn retained_bytes(&self) -> usize {
        use zipzi_core::binary_wire::WireWrite;
        match self {
            Self::Export { report, message } => report
                .expansion_bytes()
                .saturating_add(message.len().saturating_mul(4)),
            Self::Sdk(e) => e
                .code
                .len()
                .saturating_add(e.message.len())
                .saturating_add(e.field_path.as_ref().map_or(0, |v| v.len()))
                .saturating_add(
                    e.object_ids
                        .iter()
                        .chain(e.referrers.iter())
                        .map(|v| v.capacity().saturating_add(64))
                        .sum::<usize>(),
                )
                .saturating_mul(4)
                .saturating_add(4096),
            Self::Path(message) | Self::ExecutorFailure(message) | Self::WorkerPanic(message) => {
                message.len().saturating_mul(4).saturating_add(4096)
            }
            _ => 4096,
        }
    }
}
