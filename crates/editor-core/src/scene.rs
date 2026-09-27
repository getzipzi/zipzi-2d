use kasane_core::DrawableFrame;
use std::sync::Arc;

/// Decoded source pixels owned by the document, shared with render workers.
#[derive(Clone, Debug)]
pub struct SceneTexture {
    pub asset_id: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

/// A frame and its resources remain valid after editing or closing the source document.
#[derive(Clone, Debug)]
pub struct SceneSnapshot {
    pub frame: Arc<DrawableFrame>,
    pub textures: Arc<[SceneTexture]>,
}

/// Editor viewport state. Never saved to the model or recorded in SDK history.
#[derive(Clone, Copy, Debug)]
pub struct ViewState {
    pub zoom: f32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { zoom: 1.0 }
    }
}
