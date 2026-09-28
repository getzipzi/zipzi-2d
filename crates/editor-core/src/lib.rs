//! UI-independent editor coordination, SDK-backed document editing and CPU-only previews.
mod document;
mod editor;
mod scene;
mod session;
mod workspace;

pub use document::{Document, Layer};
pub use editor::{CloseDisposition, EditorCommand, EditorPreview, LoadRequest};
pub use scene::{SceneSnapshot, SceneTexture, ViewState};

use session::DocumentSession;
pub use workspace::{
    DocumentId, DocumentSummary, Editor, LoadCompletion, LoadTask, OperationId, SaveCompletion,
    SaveTask,
};

mod commands;
mod settings;
pub use commands::*;
pub use settings::*;

mod tools;
mod viewport;
pub use tools::*;
pub use viewport::*;
