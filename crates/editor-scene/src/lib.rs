//! Minimal Kasane scene and native Metal rendering for the macOS editor.
use kasane_core::{
    evaluation::{Drawable, DrawableFrame},
    types::{Canvas, Vec2},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Document {
    pub zoom: f32,
    pub opacity: f32,
    pub visible: bool,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            opacity: 1.0,
            visible: true,
        }
    }
}
impl Document {
    pub fn frame(&self) -> DrawableFrame {
        DrawableFrame {
            canvas: Canvas::new(256.0, 256.0, Vec2::new(0.0, 0.0), 1.0),
            drawables: vec![Drawable {
                id: "diamond".into(),
                texture_asset_id: "accent".into(),
                positions: vec![
                    Vec2::new(128.0, -24.0),
                    Vec2::new(232.0, -128.0),
                    Vec2::new(128.0, -232.0),
                    Vec2::new(24.0, -128.0),
                ],
                uvs: Arc::from([
                    Vec2::new(0.5, 0.0),
                    Vec2::new(1.0, 0.5),
                    Vec2::new(0.5, 1.0),
                    Vec2::new(0.0, 0.5),
                ]),
                indices: Arc::from([0, 1, 2, 0, 2, 3]),
                opacity: if self.visible { self.opacity } else { 0.0 },
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}

mod metal;
pub use kasane_render_metal::MetalTargetConfig;
pub use metal::MetalSceneRenderer;
