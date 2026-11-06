//! Immutable authoring snapshots, isolated Drafts, publication and native project I/O.
//! Domain algorithms and host/client consumers migrate in the following stages;
//! the old EditSession/DocumentSession execution path is not exported here.
mod authoring;
pub use authoring::*;
pub use zipzi_core::Status;
pub use zipzi_project::{ProjectIo, ResourceStore, ResourceView};
