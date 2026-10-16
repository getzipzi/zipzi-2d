use std::sync::Arc;

use kasane_core::{
    evaluation::{Drawable, DrawableFrame},
    types::Vec2,
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal::MTLPixelFormat, read_rgba8, MetalContext, MetalOutputMode, MetalRenderStats,
    MetalRenderer, MetalTargetConfig, MetalTexture, MetalTextureCatalog,
};

pub(crate) fn quad(id: &str, texture: &str) -> Drawable {
    Drawable {
        id: id.into(),
        texture_asset_id: texture.into(),
        positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(64.0, 0.0),
            Vec2::new(64.0, -64.0),
            Vec2::new(0.0, -64.0),
        ],
        uvs: Arc::from([
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ]),
        indices: Arc::from([0, 1, 2, 0, 2, 3]),
        ..Default::default()
    }
}

pub(crate) fn fixture(frame: &DrawableFrame) -> (Vec<u8>, MetalRenderStats) {
    let context = MetalContext::new().unwrap();
    let colors = [
        ("red", [255, 0, 0, 255]),
        ("green", [0, 255, 0, 255]),
        ("blue", [0, 0, 255, 255]),
        ("half", [255, 255, 255, 128]),
    ];
    let sources: Vec<_> = colors
        .iter()
        .map(|(_, rgba)| context.upload_rgba8(1, 1, rgba, &[]).unwrap())
        .collect();
    let catalog = MetalTextureCatalog::new(
        colors
            .iter()
            .zip(&sources)
            .map(|((name, _), source)| {
                (
                    (*name).to_owned(),
                    MetalTexture {
                        view: source,
                        width: 1,
                        height: 1,
                    },
                )
            })
            .collect(),
    );
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let output = context.output_texture(target).unwrap();
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(64.0, 64.0),
            mask_scale: 1.0,
        })
        .unwrap();
    let command = context.queue().new_command_buffer();
    let stats = renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    command.commit();
    command.wait_until_completed();
    (read_rgba8(&output).unwrap(), stats)
}

pub(crate) fn center(pixels: &[u8]) -> [u8; 4] {
    pixels[32 * 256 + 32 * 4..][..4].try_into().unwrap()
}
