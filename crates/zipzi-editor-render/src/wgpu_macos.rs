//! Temporary GPUI/macOS presentation bridge. Model encoding stays entirely in wgpu.
//! GPU completion precedes handoff between independent queues; there is no readback.
use crate::{SubmittedFrame, WgpuSceneRenderer};
use anyhow::{Result, ensure};
use zipzi_editor_core::{SceneSnapshot, ViewState, ViewportMetrics, ViewportTransform};
use zipzi_render_metal::{MetalRenderStats, MetalTargetConfig, metal};

pub struct WgpuMetalSceneRenderer {
    inner: WgpuSceneRenderer,
    device: metal::Device,
}
fn target(config: MetalTargetConfig) -> Result<zipzi_render_wgpu::WgpuTargetConfig> {
    let format = match config.format {
        metal::MTLPixelFormat::RGBA8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        metal::MTLPixelFormat::BGRA8Unorm => wgpu::TextureFormat::Bgra8Unorm,
        _ => anyhow::bail!("Unsupported GPUI preview format"),
    };
    Ok(zipzi_render_wgpu::WgpuTargetConfig {
        width: config.width,
        height: config.height,
        format,
    })
}
impl WgpuMetalSceneRenderer {
    pub fn new(device: metal::Device, config: MetalTargetConfig) -> Result<Self> {
        let inner = WgpuSceneRenderer::new(target(config)?)?;
        let wgpu_device_id = unsafe {
            let hal = inner
                .context()
                .device()
                .as_hal::<wgpu::hal::api::Metal>()
                .ok_or_else(|| anyhow::anyhow!("GPUI bridge requires the wgpu Metal adapter"))?;
            let raw = &**hal.raw_device() as *const _ as *const metal::DeviceRef;
            (&*raw).registry_id()
        };
        ensure!(
            wgpu_device_id == device.registry_id(),
            "wgpu and GPUI selected different GPUs"
        );
        Ok(Self { inner, device })
    }
    pub fn stats(&self) -> (u64, u64, u64) {
        self.inner.stats()
    }
    pub fn last_frame_stats(&self) -> MetalRenderStats {
        let s = self.inner.last_frame_stats();
        MetalRenderStats {
            draw_calls: s.draw_calls,
            render_passes: s.render_passes,
            masks: s.mask_redraws,
            mask_cache_hits: s.mask_cache_hits,
            buffer_uploads: s.geometry_buffer_creations as usize,
            active_surfaces: s.active_surfaces,
            destination_copies: s.destination_copies,
            color_attachment_bytes: s.color_attachment_bytes,
            attachment_budget_bytes: s.attachment_bytes,
            mask_resolution_limit: s.mask_resolution_limit,
        }
    }
    pub fn draw(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        view: ViewState,
    ) -> Result<metal::Texture> {
        let metrics = ViewportMetrics {
            width: config.width as f32,
            height: config.height as f32,
            scale_factor: 1.,
        };
        let transform = ViewportTransform::new(metrics, view)
            .ok_or_else(|| anyhow::anyhow!("Invalid viewport"))?;
        self.draw_transformed(queue, config, scene, transform)
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
    pub fn submit_transformed_traced(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
        mut boundary: impl FnMut(&'static str),
    ) -> Result<SubmittedFrame> {
        let submitted = self.inner.submit_transformed_traced(
            target(config)?,
            scene,
            transform,
            &mut boundary,
        )?;
        // wgpu and GPUI have separate queues. Wait on the worker before publishing,
        // so the consumer never samples an unfinished image. The UI thread never waits.
        self.inner.context().device().poll(wgpu::PollType::Wait {
            submission_index: Some(submitted.submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        // SAFETY: the HAL guard keeps the texture alive. Retain the Objective-C
        // object before releasing the guard; GPUI only samples it after completion.
        // Neither this bridge nor GPUI destroys/mutates the wgpu-owned resource.
        let image = unsafe {
            let hal = submitted
                .image
                .as_hal::<wgpu::hal::api::Metal>()
                .ok_or_else(|| anyhow::anyhow!("GPUI bridge requires the wgpu Metal adapter"))?;
            let raw = hal.raw_handle() as *const _ as *const metal::TextureRef;
            (&*raw).to_owned()
        };
        ensure!(
            image.device().registry_id() == self.device.registry_id(),
            "wgpu and GPUI selected different GPUs"
        );
        let command = queue.new_command_buffer();
        command.commit();
        boundary("wgpu_complete");
        Ok(SubmittedFrame {
            image,
            command: command.to_owned(),
        })
    }
}
