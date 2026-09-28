//! Native Metal preview adapter. No document editing or project IO lives here.
mod mipmaps;
use anyhow::{Result, anyhow, ensure};
use editor_core::{CanvasMetrics, SceneSnapshot, ViewState, ViewportTransform};
use kasane_core::Vec2;
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    MetalContext, MetalOutputMode, MetalRenderer, MetalTexture, MetalTextureCatalog, metal,
};
use mipmaps::straight_rgba_mipmaps;
use std::collections::HashMap;
use std::sync::Arc;

pub use kasane_render_metal::MetalTargetConfig;

/// The image is ordered on the consumer queue. Retain the command to bound
/// producer work by GPU completion without ever waiting on the UI thread.
pub struct SubmittedFrame {
    pub image: metal::Texture,
    pub command: metal::CommandBuffer,
}

struct UploadedTexture {
    texture: metal::Texture,
    width: u32,
    height: u32,
    revision: u64,
    used: u64,
    bytes: u64,
}

pub struct MetalSceneRenderer {
    context: MetalContext,
    renderer: MetalRenderer,
    textures: HashMap<String, UploadedTexture>,
    next_revision: u64,
    draws: u64,
    uploads: u64,
    cache_budget: u64,
    synced_scene: Option<SceneSnapshot>,
    last_stats: kasane_render_metal::MetalRenderStats,
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
            draws: 0,
            uploads: 0,
            cache_budget: 128 * 1024 * 1024,
            synced_scene: None,
            last_stats: Default::default(),
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
        let metrics = CanvasMetrics {
            width: config.width as f32,
            height: config.height as f32,
            scale_factor: 1.,
        };
        let transform = ViewportTransform::new(scene.frame.canvas, metrics, view)
            .ok_or_else(|| anyhow!("Invalid viewport"))?;
        self.draw_transformed(queue, config, scene, transform)
    }

    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.draws,
            self.uploads,
            self.textures.values().map(|t| t.bytes).sum(),
        )
    }
    pub fn last_frame_stats(&self) -> kasane_render_metal::MetalRenderStats {
        self.last_stats
    }
    pub fn draw_transformed(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
    ) -> Result<metal::Texture> {
        Ok(self
            .submit_transformed(queue, config, scene, transform)?
            .image)
    }

    pub fn submit_transformed(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
    ) -> Result<SubmittedFrame> {
        ensure!(transform.metrics.visible(), "Hidden canvas");
        self.draws += 1;
        let key = |source: &editor_core::SceneTexture| {
            format!("{}:{}:{}", source.sha256, source.width, source.height)
        };
        for source in scene.textures.iter() {
            let key = key(source);
            if let Some(cached) = self.textures.get_mut(&key) {
                cached.used = self.draws;
                continue;
            }
            let mipmaps = straight_rgba_mipmaps(source.width, source.height, &source.rgba);
            let bytes =
                source.rgba.len() as u64 + mipmaps.iter().map(|m| m.2.len() as u64).sum::<u64>();
            let texture = self
                .context
                .upload_rgba8(source.width, source.height, &source.rgba, &mipmaps)
                .map_err(status)?;
            self.textures.insert(
                key,
                UploadedTexture {
                    texture,
                    width: source.width,
                    height: source.height,
                    revision: self.next_revision,
                    used: self.draws,
                    bytes,
                },
            );
            self.next_revision += 1;
            self.uploads += 1;
        }
        while self.textures.values().map(|t| t.bytes).sum::<u64>() > self.cache_budget {
            let old = self
                .textures
                .iter()
                .filter(|(_, t)| t.used != self.draws)
                .min_by_key(|(_, t)| t.used)
                .map(|(k, _)| k.clone());
            if let Some(old) = old {
                self.textures.remove(&old);
            } else {
                break;
            }
        }
        // Asset ids are local aliases for this scene; cache identity is immutable content.
        let mut textures = MetalTextureCatalog::new(
            scene
                .textures
                .iter()
                .map(|source| {
                    let texture = &self.textures[&key(source)];
                    (
                        source.asset_id.clone(),
                        MetalTexture {
                            view: &texture.texture,
                            width: texture.width,
                            height: texture.height,
                        },
                    )
                })
                .collect(),
        );
        for source in scene.textures.iter() {
            let texture = &self.textures[&key(source)];
            textures.set_revision(source.asset_id.clone(), texture.revision);
            textures.set_repeat(source.asset_id.clone(), true);
        }
        let output = self.context.output_texture(config).map_err(status)?;
        self.renderer.resize(config).map_err(status)?;
        if !self.synced_scene.as_ref().is_some_and(|last| {
            Arc::ptr_eq(&last.frame, &scene.frame) && Arc::ptr_eq(&last.textures, &scene.textures)
        }) {
            self.renderer
                .sync_model_shared(scene.frame.clone(), &textures)
                .map_err(status)?;
            self.synced_scene = Some(scene.clone());
        }
        let scale = transform.scale * transform.metrics.scale_factor;
        self.renderer
            .update_view(ViewportConfig {
                transform: Affine2 {
                    a: Vec2::new(scale, 0.),
                    b: Vec2::new(0., scale),
                    origin: Vec2::new(
                        transform.offset.x * transform.metrics.scale_factor,
                        transform.offset.y * transform.metrics.scale_factor,
                    ),
                },
                target_extent: Vec2::new(config.width as f32, config.height as f32),
                mask_scale: scale as f64,
            })
            .map_err(status)?;
        // Use GPUI's queue, not MetalContext's convenience queue, to preserve ordering.
        let command = queue.new_command_buffer();
        self.last_stats = self
            .renderer
            .encode(command, &output, MetalOutputMode::Replace, &textures)
            .map_err(status)?;
        command.commit();
        Ok(SubmittedFrame {
            image: output,
            command: command.to_owned(),
        })
    }
}
