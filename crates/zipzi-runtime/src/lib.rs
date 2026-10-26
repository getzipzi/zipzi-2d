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
#[cfg(unix)]
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

pub use config::RuntimeLimits;
pub use events::{EventCursor, EventKind, RuntimeEvent};
pub use jobs::{Completion, Executor, PreparedModelingPlan, Work};
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
