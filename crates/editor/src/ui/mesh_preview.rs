//! Candidate texture rendering uses the same Metal mesh pipeline as the canvas.
use editor_core::{CanvasMetrics, CanvasPoint, SceneSnapshot, ViewportTransform};
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use gpui_kit::*;

pub(super) struct MeshPreview {
    scene: SceneSnapshot,
    renderer: Option<MetalSceneRenderer>,
    device: Option<metal::Device>,
    cached: Option<([u32; 6], metal::Texture)>,
}
impl MeshPreview {
    pub fn new(scene: SceneSnapshot) -> Self {
        Self {
            scene,
            renderer: None,
            device: None,
            cached: None,
        }
    }
    /// Upload textures and prepare geometry off the UI thread before publication.
    pub fn prepare(scene: SceneSnapshot, gpu: Option<SharedMetalContext>) -> anyhow::Result<Self> {
        let mut preview = Self::new(scene);
        if let Some(gpu) = gpu {
            let config = MetalTargetConfig {
                width: 1,
                height: 1,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            };
            let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
            let submitted = renderer.submit_transformed(
                &gpu.queue,
                config,
                &preview.scene,
                ViewportTransform {
                    scale: 1.
                        / preview
                            .scene
                            .viewport_frame
                            .canvas
                            .width
                            .max(preview.scene.viewport_frame.canvas.height),
                    offset: CanvasPoint::default(),
                    metrics: CanvasMetrics {
                        width: 1.,
                        height: 1.,
                        scale_factor: 1.,
                    },
                },
            )?;
            submitted.command.wait_until_completed();
            preview.renderer = Some(renderer);
            preview.device = Some(gpu.device);
        }
        Ok(preview)
    }
    pub fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        content: Bounds<Pixels>,
        window: &mut Window,
    ) -> anyhow::Result<()> {
        let Some(gpu) = window.metal_context() else {
            // GPUI's logic-test window has no GPU. Native windows use Metal below.
            return Ok(());
        };
        let sf = window.scale_factor();
        let metrics = CanvasMetrics {
            width: bounds.size.width.as_f32(),
            height: bounds.size.height.as_f32(),
            scale_factor: sf,
        };
        let (width, height) = metrics
            .physical_size()
            .ok_or_else(|| anyhow::anyhow!("Preview has no extent"))?;
        let config = MetalTargetConfig {
            width,
            height,
            format: metal::MTLPixelFormat::RGBA8Unorm,
        };
        if self
            .device
            .as_ref()
            .is_none_or(|d| !std::ptr::eq(d.as_ref(), gpu.device.as_ref()))
        {
            self.renderer = Some(MetalSceneRenderer::new(gpu.device.clone(), config)?);
            self.device = Some(gpu.device.clone());
            self.cached = None;
        }
        let offset = content.origin - bounds.origin;
        let transform = ViewportTransform {
            scale: content.size.width.as_f32() / self.scene.viewport_frame.canvas.width,
            offset: CanvasPoint {
                x: offset.x.as_f32(),
                y: offset.y.as_f32(),
            },
            metrics,
        };
        let key = [
            width,
            height,
            transform.scale.to_bits(),
            transform.offset.x.to_bits(),
            transform.offset.y.to_bits(),
            sf.to_bits(),
        ];
        if self.cached.as_ref().is_none_or(|(old, _)| *old != key) {
            let image = self.renderer.as_mut().unwrap().draw_transformed(
                &gpu.queue,
                config,
                &self.scene,
                transform,
            )?;
            self.cached = Some((key, image));
        }
        window.paint_surface(
            bounds,
            SurfaceSource::MetalTexture(self.cached.as_ref().unwrap().1.clone()),
        );
        Ok(())
    }
}
