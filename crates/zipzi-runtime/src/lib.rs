//! Single-owner, UI-independent session host. Mutation requires `&mut Runtime`;
//! callers receive immutable SDK borrows or explicitly versioned snapshots.
//! There are no callbacks under session borrows and no global blocking mutex.
pub mod addressing;
pub mod authoring;
mod config;
pub mod editor_context;
mod events;
pub mod host;
mod jobs;
pub mod local;
pub mod metrics;
pub mod modeling;
pub mod observation;
pub mod preview;
pub mod project_io;
pub mod protocol;
mod runtime;
pub mod spatial;
mod types;
mod workers;
pub use workers::WorkerPool;

pub use addressing::{AddressPatch, AddressSelector, Capabilities, CapabilityDomain};
pub use authoring::{
    EditOperation, ImagePixels, ImageQuery, ImageSnapshot, MeshQuery, MeshSnapshot, PlanIntent,
    PlanQuery, PlanSnapshot, PreparedImageInfo, PreparedPixelsQuery, ResourceOperation,
    ResourceUsage,
};
pub use config::RuntimeLimits;
pub use editor_context::EditorContext;
pub use events::{EventCursor, EventKind, RuntimeEvent};
pub use jobs::{Completion, Executor, PreparedModelingPlan, Work};
pub use modeling::ModelingCommand;
pub use observation::{
    ObservationBatch, ObservationImage, ObservationView, ObservedBounds, ObservedTexture,
};
pub use preview::{PreviewBatch, PreviewCommand, PreviewFrame, PreviewState};
pub use project_io::{
    ExportFormat, ImportFormat, ProjectDiagnostic, ProjectReport, ProjectRequest, TextureSlot,
};
pub use runtime::{
    CandidateWork, LabelAddressInfo, ModelingRecordBatch, ModelingRecordBlob,
    ModelingRecordResponse, ObjectRefInfo, RegistrySubscription, Runtime, SessionEventSubscription,
    SessionSubscription, AUTHORING_MAGIC,
};
pub use types::*;
pub use zipzi_project::TransferReservation;

#[cfg(feature = "test-support")]
pub mod test_support;

pub use runtime::{
    CollectionDomain, CollectionPage, CollectionQuery, ObjectSummary, RuntimeCapabilities,
    Selection,
};
