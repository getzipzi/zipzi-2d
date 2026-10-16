//! Candidate texture rendering uses the same Metal mesh pipeline as the canvas.
use editor_core::{SceneSnapshot, ViewportMetrics, ViewportPoint, ViewportTransform};
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use gpui_kit::*;
use std::sync::{Arc, Mutex};

/// Shared by candidates in one dialog so parameter edits retain GPU textures.
pub(super) type MeshPreviewCache = Arc<Mutex<MeshPreviewResources>>;

#[derive(Default)]
pub(super) struct MeshPreviewResources {
    renderer: Option<MetalSceneRenderer>,
    device: Option<metal::Device>,
}

impl MeshPreviewResources {
    fn renderer(
        &mut self,
        gpu: &SharedMetalContext,
        config: MetalTargetConfig,
    ) -> anyhow::Result<&mut MetalSceneRenderer> {
        if self
            .device
            .as_ref()
            .is_none_or(|d| !std::ptr::eq(d.as_ref(), gpu.device.as_ref()))
        {
            self.renderer = Some(MetalSceneRenderer::new(gpu.device.clone(), config)?);
            self.device = Some(gpu.device.clone());
        }
        Ok(self.renderer.as_mut().unwrap())
    }
}

pub(super) struct MeshPreview {
    scene: SceneSnapshot,
    resources: MeshPreviewCache,
    cached: Option<([u32; 6], metal::Texture)>,
}
impl MeshPreview {
    /// Upload textures and prepare geometry off the UI thread before publication.
    pub fn prepare(
        scene: SceneSnapshot,
        gpu: Option<SharedMetalContext>,
        resources: MeshPreviewCache,
    ) -> anyhow::Result<Self> {
        let preview = Self {
            scene,
            resources,
            cached: None,
        };
        if let Some(gpu) = gpu {
            let config = MetalTargetConfig {
                width: 1,
                height: 1,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            };
            let mut resources = preview.resources.lock().unwrap();
            let renderer = resources.renderer(&gpu, config)?;
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
                    offset: ViewportPoint::default(),
                    metrics: ViewportMetrics {
                        width: 1.,
                        height: 1.,
                        scale_factor: 1.,
                    },
                },
            )?;
            submitted.command.wait_until_completed();
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
        let metrics = ViewportMetrics {
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
        let mut resources = self.resources.lock().unwrap();
        if resources
            .device
            .as_ref()
            .is_none_or(|d| !std::ptr::eq(d.as_ref(), gpu.device.as_ref()))
        {
            self.cached = None;
        }
        let renderer = resources.renderer(&gpu, config)?;
        let offset = content.origin - bounds.origin;
        let transform = ViewportTransform {
            scale: content.size.width.as_f32() / self.scene.viewport_frame.canvas.width,
            offset: ViewportPoint {
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
            let image = renderer.draw_transformed(&gpu.queue, config, &self.scene, transform)?;
            self.cached = Some((key, image));
        }
        window.paint_surface(
            bounds,
            SurfaceSource::MetalTexture(self.cached.as_ref().unwrap().1.clone()),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MeshPreview, MeshPreviewCache};
    use editor_core::{SceneSnapshot, SceneTexture};
    use gpui_apple::metal_renderer::MetalRenderer;
    use kasane_core::{Canvas, DrawableFrame, Vec2};
    use std::sync::Arc;

    #[test]
    fn candidate_preparation_reuses_pixels_and_uploads_changed_textures() -> anyhow::Result<()> {
        objc2::rc::autoreleasepool(|_| {
            let gpui = MetalRenderer::new_headless(Default::default());
            let gpu = gpui.shared_metal_context();
            let cache = MeshPreviewCache::default();
            let frame = Arc::new(DrawableFrame {
                canvas: Canvas::new(64., 64., Vec2::default(), 1.),
                ..Default::default()
            });
            let scene = SceneSnapshot {
                model_opacity: 1.,
                runtime_frame: frame.clone(),
                viewport_frame: frame,
                textures: Arc::from([SceneTexture {
                    asset_id: "source".into(),
                    cache_key: "immutable-source".into(),
                    width: 64,
                    height: 64,
                    rgba: vec![255; 64 * 64 * 4].into(),
                }]),
            };
            let first = MeshPreview::prepare(scene.clone(), Some(gpu.clone()), cache.clone())?;
            let mut changed = scene;
            Arc::make_mut(&mut changed.viewport_frame).canvas.width = 128.;
            let second = MeshPreview::prepare(changed.clone(), Some(gpu.clone()), cache.clone())?;
            assert!(Arc::ptr_eq(&first.resources, &second.resources));
            let resources = cache.lock().unwrap();
            let renderer = resources.renderer.as_ref().unwrap();
            assert_eq!(
                renderer.stats().0,
                2,
                "both candidate frames were submitted"
            );
            assert_eq!(
                renderer.stats().1,
                1,
                "parameter edits must not upload the texture again"
            );
            assert_eq!(second.scene.viewport_frame.canvas.width, 128.);
            drop(resources);
            // Immutable content identity, rather than the local asset alias,
            // controls reuse when a different source texture is supplied.
            Arc::make_mut(&mut changed.textures)[0].cache_key = "changed-source".into();
            Arc::make_mut(&mut changed.textures)[0].rgba = vec![128; 64 * 64 * 4].into();
            let _third = MeshPreview::prepare(changed, Some(gpu), cache.clone())?;
            assert_eq!(
                cache.lock().unwrap().renderer.as_ref().unwrap().stats().1,
                2
            );
            Ok(())
        })
    }
}
