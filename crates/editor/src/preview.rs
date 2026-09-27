//! Prototype bridge: Kasane wgpu → CPU BGRA → GPUI image (not shared-device).
use editor_scene::{Document, SceneRenderer, TargetConfig};
use gpui_kit::RenderImage;
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

const SIDE: u32 = 768; // Raster resolution; rows are aligned to wgpu's 256-byte requirement.
pub type Frame = Result<Arc<RenderImage>, String>;

pub fn start() -> (mpsc::Sender<Document>, async_channel::Receiver<Frame>) {
    let (requests, incoming) = mpsc::channel::<Document>();
    let (frames, receiver) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let mut renderer = match Preview::new() {
            Ok(renderer) => renderer,
            Err(error) => {
                let _ = frames.send_blocking(Err(error.to_string()));
                return;
            }
        };
        while let Ok(mut document) = incoming.recv() {
            // Coalesce fast slider changes; never queue GPU work on the UI thread.
            while let Ok(latest) = incoming.try_recv() {
                document = latest;
            }
            let frame = renderer.render(document).map_err(|e| e.to_string());
            if frames.send_blocking(frame).is_err() {
                break;
            }
        }
    });
    (requests, receiver)
}

struct Preview {
    device: wgpu::Device,
    queue: wgpu::Queue,
    scene: SceneRenderer,
    texture: wgpu::Texture,
    buffer: wgpu::Buffer,
}
impl Preview {
    fn new() -> anyhow::Result<Self> {
        let adapter =
            pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
        let scene = SceneRenderer::new(&device, &queue, Self::config())?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kit.kasane-preview"),
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::config().format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kit.preview-readback"),
            size: u64::from(SIDE * SIDE * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            device,
            queue,
            scene,
            texture,
            buffer,
        })
    }
    fn config() -> TargetConfig {
        TargetConfig {
            width: SIDE,
            height: SIDE,
            format: wgpu::TextureFormat::Rgba8Unorm,
        }
    }
    fn render(&mut self, document: Document) -> anyhow::Result<Arc<RenderImage>> {
        self.scene.draw(
            &self.device,
            &self.queue,
            &self.texture.create_view(&Default::default()),
            Self::config(),
            document,
        )?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIDE * 4),
                    rows_per_image: Some(SIDE),
                },
            },
            self.texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = tx.send(result);
            });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })?;
        rx.recv_timeout(Duration::from_secs(10))??;
        let mut pixels = self.buffer.get_mapped_range(..).to_vec();
        self.buffer.unmap();
        for pixel in pixels.chunks_exact_mut(4) {
            // Kasane's render target is premultiplied; GPUI images use straight
            // alpha and multiply during compositing. Undo premultiplication first.
            let alpha = u32::from(pixel[3]);
            if alpha > 0 {
                for channel in &mut pixel[..3] {
                    *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
            pixel.swap(0, 2);
        }
        let image =
            image::RgbaImage::from_raw(SIDE, SIDE, pixels).expect("fixed raster dimensions");
        Ok(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_preserves_color_opacity_and_visibility() -> anyhow::Result<()> {
        let mut preview = Preview::new()?;
        let center = ((SIDE / 2 * SIDE + SIDE / 2) * 4) as usize;
        let frame = preview.render(Document::default())?;
        assert_eq!(
            &frame.as_bytes(0).unwrap()[center..center + 4],
            &[230, 205, 100, 255]
        );
        let frame = preview.render(Document {
            opacity: 0.5,
            ..Default::default()
        })?;
        let pixel = &frame.as_bytes(0).unwrap()[center..center + 4];
        assert!((pixel[3] as i16 - 128).abs() <= 1);
        for (actual, expected) in pixel[..3].iter().zip([230i16, 205, 100]) {
            assert!(
                (*actual as i16 - expected).abs() <= 2,
                "straight BGRA: {pixel:?}"
            );
        }
        let frame = preview.render(Document {
            visible: false,
            ..Default::default()
        })?;
        assert!(frame.as_bytes(0).unwrap().iter().all(|p| *p == 0));
        Ok(())
    }
}
