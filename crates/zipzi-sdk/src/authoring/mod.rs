//! Snapshot authoring, native project I/O and sole formal publication ownership.
mod commands;
mod draft;
mod history;
mod project;
mod session;
mod snapshot;
pub use project::{OpenReceipt, PreparedProject, ProjectContext, SaveReceipt, SavedMarker};

pub use commands::{Commands, NameTarget};
pub use draft::{Draft, PreparedChange};
pub use history::{HistoryLimits, HistoryState};
pub use session::{AuthoringSession, Publication, PublicationKind};
pub use snapshot::{Snapshot, SourcedSnapshot};
pub use zipzi_contract::kernel::{
    ContentId, DraftId, DraftSource, QuerySource, SessionKey, Version,
};

use zipzi_core::Status;
fn error(code: &str, message: &str) -> Status {
    Status::error(code, message)
}

#[cfg(test)]
mod tests;
