//! Native Metal preview adapter. No document editing or project IO lives here.
mod mipmaps;
use anyhow::{Result, anyhow, ensure};
use editor_scene::{SceneSnapshot, ViewState};
use kasane_core::Vec2;
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    MetalContext, MetalOutputMode, MetalRenderer, MetalTexture, MetalTextureCatalog, metal,
};
use mipmaps::straight_rgba_mipmaps;
use std::collections::HashMap;

pub use kasane_render_metal::MetalTargetConfig;

struct UploadedTexture {
    hash: String,
    texture: metal::Texture,
    width: u32,
    height: u32,
    revision: u64,
}

pub struct MetalSceneRenderer {
    context: MetalContext,
    renderer: MetalRenderer,
    textures: HashMap<String, UploadedTexture>,
    next_revision: u64,
}

fn status(error: kasane_core::Status) -> anyhow::Error {
    anyhow!("{}: {}", error.code, error.message)
}

impl MetalSceneRenderer {
    pub fn new(device: metal::Device, config: MetalTargetConfig) -> Result<Self> {
        let context = MetalContext::from_device(device);
        let renderer = MetalRenderer::new(&context, config).map_err(status)?;
        Ok(Self {
            context,
            renderer,
            textures: HashMap::new(),
            next_revision: 1,
        })
    }

    /// Produce an immutable frame on the consumer's queue, without a CPU wait/readback.
    pub fn draw(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        view: ViewState,
    ) -> Result<metal::Texture> {
        ensure!(
            view.zoom.is_finite() && view.zoom > 0.,
            "Invalid viewport zoom"
        );
        self.textures
            .retain(|id, _| scene.textures.iter().any(|source| source.asset_id == *id));
        for source in scene.textures.iter() {
            if self.textures.get(&source.asset_id).is_some_and(|cached| {
                cached.hash == source.sha256
                    && cached.width == source.width
                    && cached.height == source.height
            }) {
                continue;
            }
            let mipmaps = straight_rgba_mipmaps(source.width, source.height, &source.rgba);
            let texture = self
                .context
                .upload_rgba8(source.width, source.height, &source.rgba, &mipmaps)
                .map_err(status)?;
            self.textures.insert(
                source.asset_id.clone(),
                UploadedTexture {
                    hash: source.sha256.clone(),
                    texture,
                    width: source.width,
                    height: source.height,
                    revision: self.next_revision,
                },
            );
            self.next_revision = self
                .next_revision
                .checked_add(1)
                .ok_or_else(|| anyhow!("Texture revision exhausted"))?;
        }
        let mut textures = MetalTextureCatalog::new(
            self.textures
                .iter()
                .map(|(id, texture)| {
                    (
                        id.clone(),
                        MetalTexture {
                            view: &texture.texture,
                            width: texture.width,
                            height: texture.height,
                        },
                    )
                })
                .collect(),
        );
        for (id, texture) in &self.textures {
            textures.set_revision(id.clone(), texture.revision);
            textures.set_repeat(id.clone(), true);
        }
        let output = self.context.output_texture(config).map_err(status)?;
        self.renderer.resize(config).map_err(status)?;
        self.renderer
            .sync_model(&scene.frame, &textures)
            .map_err(status)?;
        let canvas = scene.frame.canvas;
        let scale = (config.width as f32 / canvas.width).min(config.height as f32 / canvas.height)
            * view.zoom;
        self.renderer
            .update_view(ViewportConfig {
                transform: Affine2 {
                    a: Vec2::new(scale, 0.),
                    b: Vec2::new(0., scale),
                    origin: Vec2::new(
                        (config.width as f32 - canvas.width * scale) / 2.,
                        (config.height as f32 - canvas.height * scale) / 2.,
                    ),
                },
                target_extent: Vec2::new(config.width as f32, config.height as f32),
                mask_scale: scale as f64,
            })
            .map_err(status)?;
        // Use GPUI's queue, not MetalContext's convenience queue, to preserve ordering.
        let command = queue.new_command_buffer();
        self.renderer
            .encode(command, &output, MetalOutputMode::Replace, &textures)
            .map_err(status)?;
        command.commit();
        Ok(output)
    }
}
