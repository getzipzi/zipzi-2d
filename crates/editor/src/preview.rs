//! Immutable Metal frames produced on GPUI's device and command queue.
use editor_core::SceneSnapshot;
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use gpui_kit::{IntoElement, Styled, Window, frame_trace};
use metal::MTLPixelFormat;
use std::sync::{Arc, Condvar, Mutex};

pub type Image = metal::Texture;
#[derive(Clone)]
pub struct Presented {
    pub image: Image,
    pub canvas: editor_core::CanvasFrame,
}

pub struct Request {
    pub trace_id: u64,
    pub revision: u64,
    pub canvas: Option<editor_core::CanvasFrame>,
    pub animation: Option<crate::animation::Presentation>,
    pub transform: editor_core::ViewportTransform,
    pub scene: SceneSnapshot,
    pub target: MetalTargetConfig,
}

pub struct Frame {
    pub trace_id: u64,
    pub revision: u64,
    pub canvas: Option<editor_core::CanvasFrame>,
    pub animation: Option<crate::animation::Presentation>,
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
        frame_trace::emit(request.trace_id, "enqueued", &[]);
        if let Some(old) = inbox.latest.replace(request) {
            frame_trace::emit(old.trace_id, "coalesced", &[]);
        }
        ready.notify_one();
        Ok(())
    }
}
impl Drop for RequestSender {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        if let Ok(mut inbox) = lock.lock() {
            inbox.closed = true;
            if let Some(old) = inbox.latest.take() {
                frame_trace::emit(old.trace_id, "cancelled", &[]);
            }
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
                    trace_id: 0,
                    revision: 0,
                    canvas: None,
                    animation: None,
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
            let trace_id = request.trace_id;
            frame_trace::emit(trace_id, "worker_begin", &[]);
            // Background Metal calls need a pool per frame; NSApplication's pool
            // does not drain autoreleased objects created on this thread.
            let frame = objc2::rc::autoreleasepool(|_| {
                renderer
                    .submit_transformed_traced(
                        &gpu.queue,
                        request.target,
                        &request.scene,
                        request.transform,
                        |stage| frame_trace::emit(trace_id, stage, &[]),
                    )
                    .map_err(|e| e.to_string())
            });
            let (image, completion) = match frame {
                Ok(submitted) => {
                    let stats = renderer.last_frame_stats();
                    frame_trace::emit(
                        trace_id,
                        "published",
                        &[
                            (
                                "texture",
                                submitted.image.as_ref() as *const metal::TextureRef as u64,
                            ),
                            ("draw_calls", stats.draw_calls as u64),
                            ("render_passes", stats.render_passes as u64),
                            ("mask_redraws", stats.masks as u64),
                            ("mask_hits", stats.mask_cache_hits as u64),
                            ("buffer_uploads", stats.buffer_uploads as u64),
                            ("color_attachment_bytes", stats.color_attachment_bytes),
                        ],
                    );
                    (Ok(submitted.image), Some(submitted.command))
                }
                Err(error) => {
                    frame_trace::emit(trace_id, "render_error", &[]);
                    (Err(error), None)
                }
            };
            if frames
                .send_blocking(Frame {
                    trace_id,
                    revision: request.revision,
                    canvas: request.canvas,
                    animation: request.animation,
                    stats: renderer.stats(),
                    image,
                })
                .is_err()
            {
                break;
            }
            frame_trace::emit(trace_id, "delivery_sent", &[]);
            // One producer command in flight. Requests arriving while the GPU
            // works collapse into Inbox::latest; discarded images no longer
            // leave an unbounded queue of obsolete GPU work ahead of the UI.
            // Publish before waiting: GPUI can sample on this same queue.
            if let Some(command) = completion {
                command.wait_until_completed();
                if trace_id != 0 {
                    frame_trace::emit(
                        trace_id,
                        "preview_complete",
                        &[("status", command.status() as u64)],
                    );
                    let object = unsafe {
                        &*(command.as_ref() as *const metal::CommandBufferRef
                            as *const objc2::runtime::AnyObject)
                    };
                    let start: f64 = unsafe { objc2::msg_send![object, GPUStartTime] };
                    let end: f64 = unsafe { objc2::msg_send![object, GPUEndTime] };
                    if start > 0. && end >= start {
                        frame_trace::emit_at(
                            trace_id,
                            "preview_gpu_start",
                            (start * 1e9) as u64,
                            &[],
                        );
                        frame_trace::emit_at(trace_id, "preview_gpu_end", (end * 1e9) as u64, &[]);
                    }
                }
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
