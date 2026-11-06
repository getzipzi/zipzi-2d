//! Single-owner host of immutable authoring Sessions. Workers carry frozen
//! inputs or one isolated Draft; formal Session and history never leave Runtime.
pub mod host;
mod jobs;
pub mod local;
pub mod metrics;
pub mod protocol;
mod query;
mod runtime;
mod types;
mod workers;

pub use jobs::{Cancellation, Completion, Executor, ReadProgress, Work};
pub use runtime::Runtime;
pub use types::*;
pub use workers::WorkerPool;
pub use zipzi_sdk::{DraftId, DraftSource, QuerySource, SessionKey, SourcedSnapshot, Version};
