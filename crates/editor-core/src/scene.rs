use kasane_core::DrawableFrame;
use std::sync::Arc;

pub(crate) const MIN_ZOOM: f32 = 0.04;
pub(crate) const MAX_ZOOM: f32 = 24.0;

/// Decoded source pixels owned by the document, shared with render workers.
#[derive(Clone, Debug)]
pub struct SceneTexture {
    pub asset_id: String,
    /// Identifies immutable pixels, including PSD drafts without encoded PNGs.
    pub cache_key: String,
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
    pub pan: crate::CanvasPoint,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: crate::CanvasPoint::default(),
        }
    }
}
