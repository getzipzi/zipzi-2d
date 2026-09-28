//! Immutable Metal frames produced on GPUI's device and command queue.
use editor_core::SceneSnapshot;
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use gpui_kit::{IntoElement, Styled, Window};
use metal::MTLPixelFormat;
use std::sync::{Arc, Condvar, Mutex};

pub type Image = metal::Texture;
#[derive(Clone)]
pub struct Presented {
    pub image: Image,
    pub canvas: editor_core::CanvasFrame,
}

pub struct Request {
    pub revision: u64,
    pub canvas: editor_core::CanvasFrame,
    pub scene: SceneSnapshot,
    pub target: MetalTargetConfig,
}

pub struct Frame {
    pub revision: u64,
    pub canvas: Option<editor_core::CanvasFrame>,
    pub stats: (u64, u64, u64),
    pub image: Result<Image, String>,
}
type Connection = (
    RequestSender,
    async_channel::Receiver<Frame>,
    Option<metal::Device>,
);

#[derive(Default)]
struct Inbox {
    latest: Option<Request>,
    closed: bool,
}
pub struct RequestSender {
    shared: Arc<(Mutex<Inbox>, Condvar)>,
}
impl RequestSender {
    pub fn send(&self, request: Request) -> Result<(), ()> {
        let (lock, ready) = &*self.shared;
        let mut inbox = lock.lock().map_err(|_| ())?;
        if inbox.closed {
            return Err(());
        }
        inbox.latest = Some(request);
        ready.notify_one();
        Ok(())
    }
}
impl Drop for RequestSender {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        if let Ok(mut inbox) = lock.lock() {
            inbox.closed = true;
            inbox.latest = None;
            ready.notify_one();
        }
    }
}

pub fn start(window: &Window) -> anyhow::Result<Connection> {
    let gpu = window
        .metal_context()
        .ok_or_else(|| anyhow::anyhow!("The window did not provide a native Metal device"))?;
    let device = gpu.device.clone();
    let shared = Arc::new((Mutex::new(Inbox::default()), Condvar::new()));
    let requests = RequestSender {
        shared: shared.clone(),
    };
    let (frames, receiver) = async_channel::bounded(1);
    let initial_target = MetalTargetConfig {
        width: 1,
        height: 1,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    std::thread::spawn(move || {
        let renderer =
            objc2::rc::autoreleasepool(|_| MetalSceneRenderer::new(gpu.device, initial_target));
        let mut renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) => {
                let _ = frames.send_blocking(Frame {
                    revision: 0,
                    canvas: None,
                    stats: (0, 0, 0),
                    image: Err(error.to_string()),
                });
                return;
            }
        };
        loop {
            let request = {
                let (lock, ready) = &*shared;
                let mut inbox = lock.lock().unwrap();
                while inbox.latest.is_none() && !inbox.closed {
                    inbox = ready.wait(inbox).unwrap();
                }
                if inbox.closed {
                    break;
                }
                inbox.latest.take().unwrap()
            };
            // Background Metal calls need a pool per frame; NSApplication's pool
            // does not drain autoreleased objects created on this thread.
            let frame = objc2::rc::autoreleasepool(|_| {
                renderer
                    .draw_transformed(
                        &gpu.queue,
                        request.target,
                        &request.scene,
                        request.canvas.transform,
                    )
                    .map_err(|e| e.to_string())
            });
            if frames
                .send_blocking(Frame {
                    revision: request.revision,
                    canvas: Some(request.canvas),
                    stats: renderer.stats(),
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
