use std::path::PathBuf;
use zipzi_sdk::EditReceiptSize;
use zipzi_sdk::{AuthoringSnapshot, EditReceipt, SaveReceipt};

pub use zipzi_contract::runtime::{
    BlobId, ClientId, EditId, FieldAddressId, ImageId, JobId, ObjectRefId, OperationId, PlanId,
    PreviewId, RuntimeInstanceId, SessionId, SourceRef,
};

pub use zipzi_contract::runtime::{RuntimeError, SessionInfo, SessionState};

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
    Observed(std::sync::Arc<crate::observation::ObservationSampleBatch>),
    Spatial(std::sync::Arc<crate::spatial::SpatialSampleBatch>),
    Previewed(std::sync::Arc<crate::preview::PreviewSampleBatch>),
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

pub type Result<T> = std::result::Result<T, RuntimeError>;

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

impl JobValue {
    pub(crate) fn retained_bytes(&self) -> usize {
        use zipzi_core::binary_wire::WireWrite;
        match self {
            Self::Project { report, .. } => report.expansion_bytes(),
            Self::Spatial(value) => value.samples.expansion_bytes(),
            Self::Observed(value) => value.samples.expansion_bytes(),
            Self::Previewed(value) => value.samples.expansion_bytes(),
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

pub(crate) trait RuntimeErrorBytes {
    fn retained_bytes(&self) -> usize;
}
impl RuntimeErrorBytes for RuntimeError {
    fn retained_bytes(&self) -> usize {
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
