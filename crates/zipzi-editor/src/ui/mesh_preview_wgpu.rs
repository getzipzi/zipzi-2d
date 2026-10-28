//! Candidate previews retain immutable slots in the Windows shared texture pool.
use crate::preview;
use gpui_kit::*;
use std::sync::{Arc, Mutex};
use zipzi_editor_core::{SceneSnapshot, ViewportMetrics, ViewportPoint, ViewportTransform};

pub(super) type MeshPreviewCache = Arc<Mutex<MeshPreviewResources>>;
#[derive(Default)]
pub(super) struct MeshPreviewResources {
    renderer: Option<preview::Renderer>,
    device: Option<preview::Device>,
}
impl MeshPreviewResources {
    fn renderer(&mut self, device: preview::Device) -> anyhow::Result<&mut preview::Renderer> {
        if self.device.as_ref() != Some(&device) || self.renderer.is_none() {
            self.renderer = Some(preview::Renderer::new(device.clone())?);
            self.device = Some(device);
        }
        Ok(self.renderer.as_mut().unwrap())
    }
}
pub(super) struct MeshPreview {
    image: preview::Image,
}
impl MeshPreview {
    pub fn prepare(
        scene: SceneSnapshot,
        source_size: [f32; 2],
        gpu: Option<preview::Context>,
        resources: MeshPreviewCache,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            source_size.into_iter().all(|v| v.is_finite() && v > 0.),
            "Invalid preview extent"
        );
        // Rasterize once on the generation worker; completion is a GPU fence.
        // The UI samples the immutable candidate. A 2048px longest
        // edge bounds each image to 16 MiB; zooming magnifies this preview raster.
        let scale = (2048. / source_size[0].max(source_size[1])).min(2.);
        let width = (source_size[0] * scale).ceil().max(1.) as u32;
        let height = (source_size[1] * scale).ceil().max(1.) as u32;
        let mut cache = resources.lock().unwrap();
        let renderer =
            cache.renderer(gpu.ok_or_else(|| anyhow::anyhow!("D3D11 device unavailable"))?)?;
        let slot = renderer
            .presenter
            .acquire([width, height])?
            .ok_or_else(|| anyhow::anyhow!("Candidate preview texture budget exhausted"))?;
        let frame = renderer.scene.submit_transformed(
            preview::target(width, height),
            &scene,
            ViewportTransform {
                scale,
                offset: ViewportPoint::default(),
                metrics: ViewportMetrics {
                    width: width as f32,
                    height: height as f32,
                    scale_factor: 1.,
                },
            },
        )?;
        Ok(Self {
            image: renderer.publish(slot, frame.image, 0)?,
        })
    }
    pub fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        content: Bounds<Pixels>,
        window: &mut Window,
    ) -> anyhow::Result<()> {
        preview::paint_in_bounds(&self.image, bounds, content, window)
    }
}
