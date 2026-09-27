//! Kasane's native Metal renderer, using the host's device and submission queue.
use crate::Document;
use anyhow::{Result, anyhow};
use kasane_core::types::Vec2;
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    MetalContext, MetalOutputMode, MetalRenderer, MetalTargetConfig, MetalTexture,
    MetalTextureCatalog, metal,
};
use std::collections::HashMap;

pub struct MetalSceneRenderer {
    context: MetalContext,
    renderer: MetalRenderer,
    source: metal::Texture,
}
impl MetalSceneRenderer {
    pub fn new(device: metal::Device, config: MetalTargetConfig) -> Result<Self> {
        let context = MetalContext::from_device(device);
        let source = context
            .upload_rgba8(1, 1, &[100, 205, 230, 255], &[])
            .map_err(|e| anyhow!("{e:?}"))?;
        let renderer = MetalRenderer::new(&context, config).map_err(|e| anyhow!("{e:?}"))?;
        Ok(Self {
            context,
            renderer,
            source,
        })
    }
    /// Produce an immutable frame. The caller must use the same queue as the consumer.
    /// No CPU wait or readback is performed.
    pub fn draw(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        document: Document,
    ) -> Result<metal::Texture> {
        let output = self
            .context
            .output_texture(config)
            .map_err(|e| anyhow!("{e:?}"))?;
        let textures = MetalTextureCatalog::new(HashMap::from([(
            "accent".into(),
            MetalTexture {
                view: &self.source,
                width: 1,
                height: 1,
            },
        )]));
        self.renderer.resize(config).map_err(|e| anyhow!("{e:?}"))?;
        self.renderer
            .sync_model(&document.frame(), &textures)
            .map_err(|e| anyhow!("{e:?}"))?;
        let scale = config.width.min(config.height) as f32 / 320.0 * document.zoom;
        self.renderer
            .update_view(ViewportConfig {
                transform: Affine2 {
                    a: Vec2::new(scale, 0.),
                    b: Vec2::new(0., scale),
                    origin: Vec2::new(
                        (config.width as f32 - 256. * scale) / 2.,
                        (config.height as f32 - 256. * scale) / 2.,
                    ),
                },
                target_extent: Vec2::new(config.width as f32, config.height as f32),
                mask_scale: 1.,
            })
            .map_err(|e| anyhow!("{e:?}"))?;
        // MetalContext creates a separate convenience queue; deliberately don't use it.
        let command = queue.new_command_buffer();
        self.renderer
            .encode(command, &output, MetalOutputMode::Replace, &textures)
            .map_err(|e| anyhow!("{e:?}"))?;
        command.commit();
        Ok(output)
    }
}
