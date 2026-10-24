//! Single-owner, UI-independent session host. Mutation requires `&mut Runtime`;
//! callers receive immutable SDK borrows or explicitly versioned snapshots.
//! There are no callbacks under session borrows and no global blocking mutex.
mod config;
mod events;
mod jobs;
mod runtime;
mod types;

pub use config::RuntimeLimits;
pub use events::{EventCursor, EventKind, RuntimeEvent};
pub use jobs::{Completion, Executor, Work};
pub use runtime::{CandidateWork, RegistrySubscription, Runtime, SessionSubscription};
pub use types::*;
