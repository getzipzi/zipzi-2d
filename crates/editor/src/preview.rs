//! Immutable Metal frames produced on GPUI's device and command queue.
use editor_scene::{Document, MetalSceneRenderer, MetalTargetConfig};
use gpui_kit::{IntoElement, Styled, Window};
use metal::MTLPixelFormat;
use std::sync::mpsc;

pub type Image = metal::Texture;
type Connection = (
    mpsc::Sender<Document>,
    async_channel::Receiver<Result<Image, String>>,
    Option<metal::Device>,
);

pub fn start(window: &Window) -> anyhow::Result<Connection> {
    let gpu = window
        .metal_context()
        .ok_or_else(|| anyhow::anyhow!("窗口未提供原生 Metal 设备"))?;
    let device = gpu.device.clone();
    let (requests, incoming) = mpsc::channel::<Document>();
    let (frames, receiver) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let config = MetalTargetConfig {
            width: 768,
            height: 768,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let renderer = objc2::rc::autoreleasepool(|_| MetalSceneRenderer::new(gpu.device, config));
        let mut renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) => {
                let _ = frames.send_blocking(Err(error.to_string()));
                return;
            }
        };
        while let Ok(mut document) = incoming.recv() {
            while let Ok(latest) = incoming.try_recv() {
                document = latest;
            }
            // Background Metal calls need a pool per frame; NSApplication's pool
            // does not drain autoreleased objects created on this thread.
            let frame = objc2::rc::autoreleasepool(|_| {
                renderer
                    .draw(&gpu.queue, config, document)
                    .map_err(|e| e.to_string())
            });
            if frames.send_blocking(frame).is_err() {
                break;
            }
        }
    });
    Ok((requests, receiver, Some(device)))
}

pub fn device_changed(window: &Window, device: Option<&metal::Device>) -> bool {
    window.metal_context().is_some_and(|ctx| {
        device.is_none_or(|device| !std::ptr::eq(ctx.device.as_ref(), device.as_ref()))
    })
}

pub fn element(image: Image) -> impl IntoElement {
    gpui_kit::surface(gpui_kit::SurfaceSource::MetalTexture(image)).size_full()
}
