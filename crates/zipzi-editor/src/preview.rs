//! Immutable Metal frames produced on GPUI's device and command queue.
use crate::worker_queue::{Coalesced, MailboxReceiver, mailbox};
use gpui_kit::{IntoElement, Styled, Window, frame_trace};
use metal::MTLPixelFormat;
use std::sync::{Arc, Condvar, Mutex};
use zipzi_editor_core::{SceneSnapshot, ViewportBinding, ViewportId};
use zipzi_editor_render::{MetalSceneRenderer, MetalTargetConfig};

pub type Image = metal::Texture;
pub type Device = metal::Device;
pub fn available() -> bool {
    true
}
#[derive(Clone)]
pub struct Presented {
    pub image: Image,
    pub canvas: zipzi_editor_core::CanvasFrame,
}

pub struct Request {
    pub binding: ViewportBinding,
    pub trace_id: u64,
    pub revision: u64,
    pub canvas: Option<zipzi_editor_core::CanvasFrame>,
    pub animation: Option<crate::animation::Presentation>,
    pub transform: zipzi_editor_core::ViewportTransform,
    pub scene: SceneSnapshot,
    pub target: MetalTargetConfig,
}

pub struct Frame {
    pub binding: Option<ViewportBinding>,
    pub trace_id: u64,
    pub revision: u64,
    pub canvas: Option<zipzi_editor_core::CanvasFrame>,
    pub animation: Option<crate::animation::Presentation>,
    pub stats: (u64, u64, u64),
    pub image: Result<Image, String>,
}
type Connection = (
    RequestSender,
    MailboxReceiver<Option<ViewportId>, Frame>,
    Option<metal::Device>,
);

#[derive(Default)]
struct Inbox {
    pending: Coalesced<ViewportId, Request>,
    closed: bool,
}
pub struct RequestSender {
    shared: Arc<(Mutex<Inbox>, Condvar)>,
}
impl RequestSender {
    /// Hold work without a worker so UI tests can deliver frames deterministically.
    #[cfg(test)]
    pub(crate) fn deferred() -> Self {
        Self {
            shared: Arc::new((Mutex::new(Inbox::default()), Condvar::new())),
        }
    }
    #[cfg(test)]
    pub(crate) fn take_pending(&self) -> Option<Request> {
        self.shared.0.lock().unwrap().pending.pop()
    }
    pub fn cancel(&self, viewport: ViewportId) {
        if let Ok(mut inbox) = self.shared.0.lock() {
            if let Some(old) = inbox.pending.remove(viewport) {
                frame_trace::emit(old.trace_id, "cancelled", &[]);
            }
        }
    }
    pub fn send(&self, request: Request) -> Result<(), ()> {
        let (lock, ready) = &*self.shared;
        let mut inbox = lock.lock().map_err(|_| ())?;
        if inbox.closed {
            return Err(());
        }
        let trace_id = request.trace_id;
        if let Some(old) = inbox.pending.insert(request.binding.viewport, request) {
            frame_trace::emit(old.trace_id, "coalesced", &[]);
        }
        frame_trace::emit(trace_id, "enqueued", &[]);
        ready.notify_one();
        Ok(())
    }
}
impl Drop for RequestSender {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        if let Ok(mut inbox) = lock.lock() {
            inbox.closed = true;
            while let Some(old) = inbox.pending.pop() {
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
    let (frames, receiver) = mailbox();
    let initial_target = MetalTargetConfig {
        width: 1,
        height: 1,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    std::thread::spawn(move || {
        crate::worker::configure(1);
        let renderer =
            objc2::rc::autoreleasepool(|_| MetalSceneRenderer::new(gpu.device, initial_target));
        let mut renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) => {
                let _ = frames.send(
                    None,
                    Frame {
                        binding: None,
                        trace_id: 0,
                        revision: 0,
                        canvas: None,
                        animation: None,
                        stats: (0, 0, 0),
                        image: Err(error.to_string()),
                    },
                );
                return;
            }
        };
        loop {
            let request = {
                let (lock, ready) = &*shared;
                let mut inbox = lock.lock().unwrap();
                while inbox.pending.is_empty() && !inbox.closed {
                    inbox = ready.wait(inbox).unwrap();
                }
                if inbox.closed {
                    break;
                }
                let request = inbox.pending.pop().unwrap();
                frame_trace::emit(request.trace_id, "dequeued", &[]);
                request
            };
            let trace_id = request.trace_id;
            frame_trace::emit(trace_id, "worker_begin", &[]);
            // Background Metal calls need a pool per frame; NSApplication's pool
            // does not drain autoreleased objects created on this thread.
            let cpu_start = crate::worker::cpu_time(trace_id);
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
            crate::worker::record_cpu_time(trace_id, "render_cpu", cpu_start);
            let (image, completion) = match frame {
                Ok(submitted) => {
                    let stats = renderer.last_frame_stats();
                    if trace_id != 0 {
                        submitted.image.set_label(&format!(
                            "{}{trace_id}",
                            frame_trace::PREVIEW_TEXTURE_LABEL_PREFIX
                        ));
                    }
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
                .send_with_replaced(
                    Some(request.binding.viewport),
                    Frame {
                        binding: Some(request.binding),
                        trace_id,
                        revision: request.revision,
                        canvas: request.canvas,
                        animation: request.animation,
                        stats: renderer.stats(),
                        image,
                    },
                    |old| {
                        frame_trace::emit(
                            old.trace_id,
                            "delivery_coalesced",
                            &[("replaced_by", trace_id)],
                        )
                    },
                )
                .is_err()
            {
                break;
            }
            frame_trace::emit(trace_id, "delivery_sent", &[]);
            // One producer command in flight. Requests arriving while the GPU
            // works coalesce independently by viewport; discarded images no longer
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
