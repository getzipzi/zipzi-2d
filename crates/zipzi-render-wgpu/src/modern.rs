//! Persistent model state; GPU commands own immutable geometry and uniforms.
use crate::{
    api::*,
    geometry::MeshBuffers,
    mask::{CachedMask, MaskCacheKey},
    pipeline::{Pipelines, UniformKey},
};
use std::{collections::HashMap, sync::Arc};
use zipzi_core::{evaluation::DrawableFrame, Status, Vec2};
use zipzi_render::{ScenePlan, ViewportConfig};

pub struct WgpuRenderer {
    pub(crate) device: wgpu::Device,
    pub(crate) target: WgpuTargetConfig,
    pub(crate) pipelines: Pipelines,
    pub(crate) scene: ScenePlan,
    pub(crate) frame: Arc<DrawableFrame>,
    pub(crate) viewport: Option<ViewportConfig>,
    pub(crate) geometry: Vec<Option<MeshBuffers>>,
    pub(crate) next_geometry_revision: u64,
    pub(crate) pending_uploads: u64,
    pub(crate) pending_vertex_bytes: u64,
    pub(crate) pending_index_bytes: u64,
    pub(crate) mask_cache: HashMap<MaskCacheKey, CachedMask>,
    pub(crate) atlas_pages: Vec<crate::surface::PooledTexture>,
    pub(crate) quad: MeshBuffers,
    pub(crate) uniforms: HashMap<UniformKey, wgpu::Buffer>,
    pub(crate) fallback: crate::surface::PooledTexture,
}

/// Submit this encoder before encoding another frame with the same renderer.
/// Cached mask writes and reads must use the same device queue.
pub struct WgpuEncodeTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub output: &'a wgpu::TextureView,
    pub output_mode: WgpuOutputMode,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WgpuOutputMode {
    #[default]
    Replace,
    Composite,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WgpuRenderStats {
    pub active_surfaces: usize,
    pub masks: usize,
    pub destination_targets: usize,
    pub vertex_upload_bytes: u64,
    pub index_upload_bytes: u64,
    pub geometry_buffer_creations: u64,
    pub uniform_buffer_creations: u64,
    pub bind_group_creations: u64,
    pub mask_redraws: usize,
    pub mask_cache_hits: usize,
    pub attachment_bytes: u64,
    pub color_attachment_bytes: u64,
    pub mask_resolution_limit: u32,
    pub draw_calls: usize,
    pub render_passes: usize,
    pub destination_copies: usize,
}
impl WgpuRenderer {
    pub fn new(device: &wgpu::Device, target: WgpuTargetConfig) -> Result<Self, Status> {
        validate_target(device, target)?;
        Ok(Self {
            device: device.clone(),
            target,
            pipelines: Pipelines::new(device, target.format),
            scene: ScenePlan::default(),
            frame: Arc::default(),
            viewport: None,
            geometry: Vec::new(),
            next_geometry_revision: 1,
            pending_uploads: 0,
            pending_vertex_bytes: 0,
            pending_index_bytes: 0,
            mask_cache: HashMap::new(),
            atlas_pages: Vec::new(),
            quad: MeshBuffers::quad(device),
            uniforms: HashMap::new(),
            fallback: crate::surface::PooledTexture::new(
                device,
                (1, 1),
                wgpu::TextureFormat::Rgba8Unorm,
            ),
        })
    }
    pub fn target(&self) -> WgpuTargetConfig {
        self.target
    }
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        target: WgpuTargetConfig,
    ) -> Result<(), Status> {
        self.ensure_device(device)?;
        validate_target(device, target)?;
        if target.format != self.target.format {
            self.pipelines = Pipelines::new(device, target.format);
        }
        if target != self.target {
            self.viewport = None;
        }
        self.target = target;
        Ok(())
    }
    pub fn update_view(
        &mut self,
        device: &wgpu::Device,
        view: ViewportConfig,
    ) -> Result<(), Status> {
        self.ensure_device(device)?;
        if self.scene.targets().is_empty() {
            return Err(Status::error("MISSING_MODEL", "Submit a model first"));
        }
        if view.target_extent != Vec2::new(self.target.width as f32, self.target.height as f32) {
            return Err(Status::error(
                "INVALID_TARGET",
                "Viewport extent must match output",
            ));
        }
        if !view.mask_scale.is_finite() || view.mask_scale <= 0. {
            return Err(Status::error(
                "INVALID_MASK_SCALE",
                "Mask scale must be positive and finite",
            ));
        }
        zipzi_render::gpu::inverse_affine(view.transform)?;
        let layouts = self.color_layout(view, WgpuOutputMode::Replace)?;
        self.validate_layouts(&layouts)?;
        self.viewport = Some(view);
        Ok(())
    }
    pub(crate) fn ensure_device(&self, device: &wgpu::Device) -> Result<(), Status> {
        if *device != self.device {
            return Err(Status::error(
                "DEVICE_MISMATCH",
                "Recreate renderer after changing devices",
            ));
        }
        Ok(())
    }
    pub(crate) fn validate_layouts(
        &self,
        layout: &crate::surface::ColorLayout,
    ) -> Result<(), Status> {
        let limit = self.device.limits().max_texture_dimension_2d;
        if layout
            .surfaces
            .iter()
            .any(|s| s.size.0 > limit || s.size.1 > limit)
        {
            return Err(Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "Surface exceeds device limits",
            ));
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: &wgpu::TextureView,
        frame: &DrawableFrame,
        textures: &WgpuTextureCatalog<'_>,
        viewport: ViewportConfig,
    ) -> Result<WgpuRenderStats, Status> {
        self.sync_model(device, frame, textures)?;
        self.update_view(device, viewport)?;
        let mut encoder = device.create_command_encoder(&Default::default());
        let stats = self.encode(
            WgpuEncodeTarget {
                device,
                queue,
                encoder: &mut encoder,
                output,
                output_mode: WgpuOutputMode::Replace,
            },
            textures,
        )?;
        queue.submit([encoder.finish()]);
        Ok(stats)
    }
}
pub(crate) fn validate_target(
    device: &wgpu::Device,
    target: WgpuTargetConfig,
) -> Result<(), Status> {
    let limit = device.limits().max_texture_dimension_2d;
    if target.width == 0 || target.height == 0 || target.width > limit || target.height > limit {
        return Err(Status::error(
            "INVALID_TARGET",
            "Output extent exceeds device limits",
        ));
    }
    if !matches!(
        target.format,
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        return Err(Status::error(
            "UNSUPPORTED_TARGET_FORMAT",
            "Output must be 8-bit RGBA or BGRA",
        ));
    }
    if u64::from(target.width) * u64::from(target.height) * 4
        > zipzi_render::OFFSCREEN_BUDGET_BYTES as u64
    {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "Output exceeds attachment budget",
        ));
    }
    Ok(())
}
