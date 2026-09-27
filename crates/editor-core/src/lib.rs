//! SDK-backed document editing and immutable, CPU-only preview inputs.
mod document;
mod scene;

pub use document::{Document, Layer};
pub use scene::{SceneSnapshot, SceneTexture, ViewState};
