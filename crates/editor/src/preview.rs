//! Immutable Metal frames produced on GPUI's device and command queue.
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use editor_scene::{SceneSnapshot, ViewState};
use gpui_kit::{IntoElement, Styled, Window};
use metal::MTLPixelFormat;
use std::sync::mpsc;

pub const LAYERS_WIDTH: f32 = 192.;
pub const PROPERTIES_WIDTH: f32 = 256.;
pub const DOCUMENT_BAR_HEIGHT: f32 = 40.;
pub const STATUS_BAR_HEIGHT: f32 = 32.;

pub type Image = metal::Texture;

pub struct Request {
    pub revision: u64,
    pub scene: SceneSnapshot,
    pub view: ViewState,
    pub target: MetalTargetConfig,
}

pub struct Frame {
    pub revision: u64,
    pub image: Result<Image, String>,
}
type Connection = (
    mpsc::Sender<Request>,
    async_channel::Receiver<Frame>,
    Option<metal::Device>,
);

pub fn target_config(window: &Window) -> MetalTargetConfig {
    let viewport = window.viewport_size();
    let scale = window.scale_factor();
    let pixels = |logical: f32| (logical.max(1.) * scale).round().clamp(1., 16_384.) as u32;
    MetalTargetConfig {
        width: pixels(viewport.width.as_f32() - LAYERS_WIDTH - PROPERTIES_WIDTH),
        height: pixels(viewport.height.as_f32() - DOCUMENT_BAR_HEIGHT - STATUS_BAR_HEIGHT),
        format: MTLPixelFormat::RGBA8Unorm,
    }
}

pub fn start(window: &Window) -> anyhow::Result<Connection> {
    let gpu = window
        .metal_context()
        .ok_or_else(|| anyhow::anyhow!("The window did not provide a native Metal device"))?;
    let device = gpu.device.clone();
    let (requests, incoming) = mpsc::channel::<Request>();
    let (frames, receiver) = async_channel::bounded(1);
    let initial_target = target_config(window);
    std::thread::spawn(move || {
        let renderer =
            objc2::rc::autoreleasepool(|_| MetalSceneRenderer::new(gpu.device, initial_target));
        let mut renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) => {
                let _ = frames.send_blocking(Frame {
                    revision: 0,
                    image: Err(error.to_string()),
                });
                return;
            }
        };
        while let Ok(mut request) = incoming.recv() {
            while let Ok(latest) = incoming.try_recv() {
                request = latest;
            }
            // Background Metal calls need a pool per frame; NSApplication's pool
            // does not drain autoreleased objects created on this thread.
            let frame = objc2::rc::autoreleasepool(|_| {
                renderer
                    .draw(&gpu.queue, request.target, &request.scene, request.view)
                    .map_err(|e| e.to_string())
            });
            if frames
                .send_blocking(Frame {
                    revision: request.revision,
                    image: frame,
                })
                .is_err()
            {
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
