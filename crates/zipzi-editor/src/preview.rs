//! Immutable preview frames, coalesced independently for each viewport.
use crate::worker_queue::{Coalesced, MailboxReceiver, mailbox};
use gpui_kit::frame_trace;
use std::sync::{Arc, Condvar, Mutex};
use zipzi_editor_core::{SceneSnapshot, ViewportBinding, ViewportId};
#[cfg(target_os = "macos")]
mod metal;
#[cfg(target_os = "macos")]
pub use metal::*;
#[cfg(target_os = "windows")]
mod readback;
#[cfg(target_os = "windows")]
pub use readback::*;

#[derive(Clone, Copy)]
pub struct Target {
    pub width: u32,
    pub height: u32,
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
    pub target: Target,
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
    Option<Device>,
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
