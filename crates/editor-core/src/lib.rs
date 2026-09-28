//! UI-independent editor coordination, SDK-backed document editing and CPU-only previews.
mod document;
mod editor;
mod scene;

pub use document::{Document, Layer};
pub use editor::{CloseDisposition, Editor, EditorCommand, EditorPreview, LoadRequest};
pub use scene::{SceneSnapshot, SceneTexture, ViewState};
