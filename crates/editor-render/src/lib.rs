//! Native Metal preview adapter. No document editing or project IO lives here.
mod overlay;
use anyhow::{Result, anyhow, ensure};
use editor_core::{CanvasMetrics, SceneSnapshot, ViewState, ViewportTransform};
use kasane_core::Vec2;
use kasane_render::texture::straight_rgba_mipmaps;
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    MetalContext, MetalOutputMode, MetalRenderer, MetalTexture, MetalTextureCatalog, metal,
};
pub use overlay::{
    MetalOverlayRenderer, OVERLAY_MARKER_RADIUS, OverlayPass, OverlayStyle, OverlayViewport,
};
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
        let transform = ViewportTransform::new(scene.viewport_frame.canvas, metrics, view)
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
        self.submit_transformed_traced(queue, config, scene, transform, |_| {})
    }

    /// Optional CPU boundary observer; the normal path monomorphizes to a no-op.
    pub fn submit_transformed_traced(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
        mut boundary: impl FnMut(&'static str),
    ) -> Result<SubmittedFrame> {
        ensure!(transform.metrics.visible(), "Hidden canvas");
        self.draws += 1;
        let key = |source: &editor_core::SceneTexture| {
            format!("{}:{}:{}", source.cache_key, source.width, source.height)
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
        let mut cached_bytes = self.textures.values().map(|t| t.bytes).sum::<u64>();
        if cached_bytes > self.cache_budget {
            // Sort idle entries once instead of scanning the whole cache for
            // every eviction when switching between texture-heavy documents.
            let mut idle: Vec<_> = self
                .textures
                .iter()
                .filter(|(_, t)| t.used != self.draws)
                .map(|(key, texture)| (key.clone(), texture.used))
                .collect();
            idle.sort_unstable_by_key(|(_, used)| *used);
            for (key, _) in idle {
                if cached_bytes <= self.cache_budget {
                    break;
                }
                if let Some(texture) = self.textures.remove(&key) {
                    cached_bytes -= texture.bytes;
                }
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
        boundary("textures_ready");
        let output = self.context.output_texture(config).map_err(status)?;
        self.renderer.resize(config).map_err(status)?;
        if !self.synced_scene.as_ref().is_some_and(|last| {
            Arc::ptr_eq(&last.viewport_frame, &scene.viewport_frame)
                && Arc::ptr_eq(&last.textures, &scene.textures)
                && last.model_opacity == scene.model_opacity
        }) {
            self.renderer
                .sync_model_shared(scene.render_frame()?, &textures)
                .map_err(status)?;
            self.synced_scene = Some(scene.clone());
        }
        boundary("model_synced");
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
        boundary("encode_begin");
        self.last_stats = self
            .renderer
            .encode(command, &output, MetalOutputMode::Replace, &textures)
            .map_err(status)?;
        boundary("preview_commit_begin");
        command.commit();
        boundary("preview_submitted");
        Ok(SubmittedFrame {
            image: output,
            command: command.to_owned(),
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use editor_core::{Document, SceneTexture};

    #[test]
    fn texture_eviction_keeps_recent_and_active_sources_without_reuploads() -> Result<()> {
        objc2::rc::autoreleasepool(|_| {
            let device = metal::Device::system_default().expect("Metal device");
            let queue = device.new_command_queue();
            let config = MetalTargetConfig {
                width: 1,
                height: 1,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            };
            let mut renderer = MetalSceneRenderer::new(device, config)?;
            renderer.cache_budget = 12; // Three 1x1 RGBA textures.
            let mut scene = Document::new_blank()?.snapshot()?;
            let mut draw = |ids: &[&str]| -> Result<(u64, u64, u64)> {
                scene.textures = ids
                    .iter()
                    .map(|id| SceneTexture {
                        asset_id: (*id).into(),
                        cache_key: (*id).into(),
                        width: 1,
                        height: 1,
                        rgba: Arc::from([255; 4]),
                    })
                    .collect::<Vec<_>>()
                    .into();
                let transform = ViewportTransform::new(
                    scene.viewport_frame.canvas,
                    CanvasMetrics {
                        width: 1.,
                        height: 1.,
                        scale_factor: 1.,
                    },
                    ViewState::default(),
                )
                .unwrap();
                let submitted = renderer.submit_transformed(&queue, config, &scene, transform)?;
                submitted.command.wait_until_completed();
                assert_eq!(
                    submitted.command.status(),
                    metal::MTLCommandBufferStatus::Completed
                );
                Ok(renderer.stats())
            };
            assert_eq!(draw(&["a", "b", "c"])?, (1, 3, 12));
            assert_eq!(draw(&["a", "b"])?, (2, 3, 12));
            assert_eq!(draw(&["a", "d", "e"])?, (3, 5, 12));
            // The current scene survives eviction even when it exceeds budget.
            assert_eq!(draw(&["a", "d", "e", "f"])?, (4, 6, 16));
            assert_eq!(draw(&["a", "d", "e", "f"])?, (5, 6, 16));
            assert_eq!(draw(&["a", "d"])?, (6, 6, 12));
            // The oldest source was evicted and must upload again.
            assert_eq!(draw(&["c"])?, (7, 7, 12));
            Ok(())
        })
    }
}
