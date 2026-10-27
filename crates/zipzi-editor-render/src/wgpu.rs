//! Editor scene adapter using the cross-platform wgpu renderer.
use anyhow::{Result, anyhow, ensure};
use std::collections::HashMap;
use std::sync::Arc;
use zipzi_core::Vec2;
use zipzi_editor_core::{SceneSnapshot, ViewState, ViewportMetrics, ViewportTransform};
use zipzi_render::texture::straight_rgba_mipmaps;
use zipzi_render::{Affine2, ViewportConfig};
use zipzi_render_wgpu::{
    WgpuContext, WgpuOutputMode, WgpuRenderer, WgpuTexture, WgpuTextureCatalog,
};

pub use zipzi_render_wgpu::WgpuTargetConfig;

/// A submitted image and its completion index on this adapter's wgpu queue.
pub struct WgpuSubmittedFrame {
    pub image: wgpu::Texture,
    pub submission: wgpu::SubmissionIndex,
}

struct UploadedTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    revision: u64,
    used: u64,
    bytes: u64,
}

pub struct WgpuSceneRenderer {
    context: WgpuContext,
    renderer: WgpuRenderer,
    textures: HashMap<String, UploadedTexture>,
    next_revision: u64,
    draws: u64,
    uploads: u64,
    cache_budget: u64,
    synced_scene: Option<SceneSnapshot>,
    last_stats: zipzi_render_wgpu::WgpuRenderStats,
}

fn status(error: zipzi_core::Status) -> anyhow::Error {
    anyhow!("{}: {}", error.code, error.message)
}

impl WgpuSceneRenderer {
    pub fn new(config: WgpuTargetConfig) -> Result<Self> {
        let context = WgpuContext::new().map_err(status)?;
        let renderer = WgpuRenderer::new(context.device(), config).map_err(status)?;
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

    /// Submit an immutable image on the adapter's queue without a CPU wait or readback.
    pub fn draw(
        &mut self,
        config: WgpuTargetConfig,
        scene: &SceneSnapshot,
        view: ViewState,
    ) -> Result<wgpu::Texture> {
        let metrics = ViewportMetrics {
            width: config.width as f32,
            height: config.height as f32,
            scale_factor: 1.,
        };
        let transform =
            ViewportTransform::new(metrics, view).ok_or_else(|| anyhow!("Invalid viewport"))?;
        self.draw_transformed(config, scene, transform)
    }

    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.draws,
            self.uploads,
            self.textures.values().map(|t| t.bytes).sum(),
        )
    }
    pub fn last_frame_stats(&self) -> zipzi_render_wgpu::WgpuRenderStats {
        self.last_stats
    }
    pub fn draw_transformed(
        &mut self,
        config: WgpuTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
    ) -> Result<wgpu::Texture> {
        Ok(self.submit_transformed(config, scene, transform)?.image)
    }

    pub fn submit_transformed(
        &mut self,
        config: WgpuTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
    ) -> Result<WgpuSubmittedFrame> {
        self.submit_transformed_traced(config, scene, transform, |_| {})
    }

    /// Optional CPU boundary observer; the normal path monomorphizes to a no-op.
    pub fn submit_transformed_traced(
        &mut self,
        config: WgpuTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
        mut boundary: impl FnMut(&'static str),
    ) -> Result<WgpuSubmittedFrame> {
        ensure!(transform.metrics.visible(), "Hidden canvas");
        self.draws += 1;
        let key = |source: &zipzi_editor_core::SceneTexture| {
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
                    view: texture.create_view(&Default::default()),
                    _texture: texture,
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
        let mut textures = WgpuTextureCatalog::new(
            scene
                .textures
                .iter()
                .map(|source| {
                    let texture = &self.textures[&key(source)];
                    (
                        source.asset_id.clone(),
                        WgpuTexture {
                            view: &texture.view,
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
        self.renderer
            .resize(self.context.device(), config)
            .map_err(status)?;
        if !self.synced_scene.as_ref().is_some_and(|last| {
            Arc::ptr_eq(&last.viewport_frame, &scene.viewport_frame)
                && Arc::ptr_eq(&last.textures, &scene.textures)
                && last.model_opacity == scene.model_opacity
        }) {
            self.renderer
                .sync_model_shared(self.context.device(), scene.render_frame()?, &textures)
                .map_err(status)?;
            self.synced_scene = Some(scene.clone());
        }
        boundary("model_synced");
        let scale = transform.scale * transform.metrics.scale_factor;
        self.renderer
            .update_view(
                self.context.device(),
                ViewportConfig {
                    transform: Affine2 {
                        a: Vec2::new(scale, 0.),
                        b: Vec2::new(0., -scale),
                        origin: Vec2::new(
                            transform.offset.x * transform.metrics.scale_factor,
                            transform.offset.y * transform.metrics.scale_factor,
                        ),
                    },
                    target_extent: Vec2::new(config.width as f32, config.height as f32),
                    mask_scale: scale as f64,
                },
            )
            .map_err(status)?;
        let view = output.create_view(&Default::default());
        let mut encoder = self
            .context
            .device()
            .create_command_encoder(&Default::default());
        boundary("encode_begin");
        self.last_stats = self
            .renderer
            .encode(
                zipzi_render_wgpu::WgpuEncodeTarget {
                    device: self.context.device(),
                    queue: self.context.queue(),
                    encoder: &mut encoder,
                    output: &view,
                    output_mode: WgpuOutputMode::Replace,
                },
                &textures,
            )
            .map_err(status)?;
        boundary("preview_commit_begin");
        let submission = self.context.queue().submit([encoder.finish()]);
        boundary("preview_submitted");
        Ok(WgpuSubmittedFrame {
            image: output,
            submission,
        })
    }
    pub fn context(&self) -> &WgpuContext {
        &self.context
    }
}
