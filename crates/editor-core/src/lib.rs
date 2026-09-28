//! UI-independent editor coordination, SDK-backed document editing and CPU-only previews.
mod document;
mod editor;
mod scene;
mod session;
mod tree;
mod workspace;

pub use document::{Document, Layer};
pub use editor::{CloseDisposition, EditorCommand, EditorPreview, LoadRequest};
pub use scene::{SceneSnapshot, SceneTexture, ViewState};
pub use tree::{TreeNode, TreeNodeKind};

use session::DocumentSession;
pub use workspace::{
    DocumentId, DocumentSummary, Editor, LoadCompletion, LoadTask, OperationId, SaveCompletion,
    SaveTask,
};

mod commands;
mod recent_files;
mod settings;
pub use commands::*;
pub use recent_files::*;
pub use settings::*;

mod tools;
mod viewport;
pub use tools::*;
pub use viewport::*;
