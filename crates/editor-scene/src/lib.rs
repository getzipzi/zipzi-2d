//! Small Kasane scene, independent of the UI host.
use anyhow::{Result, anyhow};
use kasane_core::{
    evaluation::{Drawable, DrawableFrame},
    types::{Canvas, Vec2},
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_wgpu::{
    WgpuEncodeTarget, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig, WgpuTexture,
    WgpuTextureCatalog,
};
use std::{collections::HashMap, sync::Arc};

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

pub struct SceneRenderer {
    renderer: WgpuRenderer,
    source: wgpu::TextureView,
}
impl SceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: WgpuTargetConfig,
    ) -> Result<Self> {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kasane.accent"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &[100, 205, 230, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            texture.size(),
        );
        Ok(Self {
            renderer: WgpuRenderer::new(device, config).map_err(|e| anyhow!("{e:?}"))?,
            source: texture.create_view(&Default::default()),
        })
    }
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: &wgpu::TextureView,
        config: WgpuTargetConfig,
        document: Document,
    ) -> Result<()> {
        let textures = WgpuTextureCatalog::new(HashMap::from([(
            "accent".into(),
            WgpuTexture {
                view: &self.source,
                width: 1,
                height: 1,
            },
        )]));
        self.renderer
            .resize(device, config)
            .map_err(|e| anyhow!("{e:?}"))?;
        self.renderer
            .sync_model(device, &document.frame(), &textures)
            .map_err(|e| anyhow!("{e:?}"))?;
        let scale = config.width.min(config.height) as f32 / 320.0 * document.zoom;
        self.renderer
            .update_view(
                device,
                ViewportConfig {
                    transform: Affine2 {
                        a: Vec2::new(scale, 0.0),
                        b: Vec2::new(0.0, scale),
                        origin: Vec2::new(
                            (config.width as f32 - 256.0 * scale) / 2.0,
                            (config.height as f32 - 256.0 * scale) / 2.0,
                        ),
                    },
                    target_extent: Vec2::new(config.width as f32, config.height as f32),
                    mask_scale: 1.0,
                },
            )
            .map_err(|e| anyhow!("{e:?}"))?;
        let mut encoder = device.create_command_encoder(&Default::default());
        self.renderer
            .encode(
                WgpuEncodeTarget {
                    device,
                    queue,
                    encoder: &mut encoder,
                    output,
                    output_mode: WgpuOutputMode::Replace,
                },
                &textures,
            )
            .map_err(|e| anyhow!("{e:?}"))?;
        queue.submit([encoder.finish()]);
        Ok(())
    }
}
pub use kasane_render_wgpu::WgpuTargetConfig as TargetConfig;
