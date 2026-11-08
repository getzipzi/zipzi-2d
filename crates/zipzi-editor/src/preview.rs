//! Immutable preview frames, coalesced independently for each viewport.
use crate::worker_queue::{Coalesced, MailboxReceiver, mailbox};
use gpui_kit::frame_trace;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Condvar, Mutex},
};
use zipzi_editor_core::{DocumentId, SceneSnapshot, ViewportBinding, ViewportId};
#[cfg(target_os = "macos")]
mod metal;
#[cfg(target_os = "macos")]
pub use metal::*;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

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
    retain_documents: Option<RetainDocuments>,
    closed: bool,
}
struct RetainDocuments {
    live: HashSet<DocumentId>,
    closed: HashSet<DocumentId>,
}
enum PreviewWork {
    Render(Request),
    Retain(HashSet<DocumentId>),
}
impl Inbox {
    fn has_work(&self) -> bool {
        !self.pending.is_empty() || self.retain_documents.is_some()
    }
    fn pop_work(&mut self) -> Option<PreviewWork> {
        self.retain_documents
            .take()
            .map(|retention| PreviewWork::Retain(retention.live))
            .or_else(|| self.pending.pop().map(PreviewWork::Render))
    }
    /// A Windows preview can be held outside the queue while all presentation
    /// slots are busy. Do not requeue it after its document has closed.
    #[cfg(any(test, target_os = "windows"))]
    fn requeue_after_backpressure(&mut self, request: Request) {
        let latest = self
            .pending
            .remove(request.binding.viewport)
            .unwrap_or(request);
        if self
            .retain_documents
            .as_ref()
            .is_some_and(|retention| retention.closed.contains(&latest.binding.document))
        {
            frame_trace::emit(latest.trace_id, "cancelled", &[]);
        } else {
            self.pending.insert(latest.binding.viewport, latest);
        }
    }
}
#[derive(Default)]
struct PreviewCacheOwners(HashMap<(DocumentId, ViewportId), HashSet<String>>);
impl PreviewCacheOwners {
    fn note(&mut self, request: &Request) {
        self.0.insert(
            (request.binding.document, request.binding.viewport),
            request
                .scene
                .textures
                .iter()
                .map(zipzi_editor_render::scene_texture_key)
                .collect(),
        );
    }
    fn retain(&mut self, live: &HashSet<DocumentId>) -> HashSet<String> {
        self.0.retain(|(id, _), _| live.contains(id));
        self.0
            .values()
            .flat_map(|keys| keys.iter().cloned())
            .collect()
    }
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
    pub fn retain_documents(&self, live: HashSet<DocumentId>, closed: DocumentId) {
        let (lock, ready) = &*self.shared;
        if let Ok(mut inbox) = lock.lock() {
            let mut pending = Vec::new();
            while let Some(request) = inbox.pending.pop() {
                if live.contains(&request.binding.document) {
                    pending.push(request);
                } else {
                    frame_trace::emit(request.trace_id, "cancelled", &[]);
                }
            }
            for request in pending {
                inbox.pending.insert(request.binding.viewport, request);
            }
            let mut closed_documents = inbox
                .retain_documents
                .take()
                .map_or_else(HashSet::new, |retention| retention.closed);
            closed_documents.insert(closed);
            inbox.retain_documents = Some(RetainDocuments {
                live,
                closed: closed_documents,
            });
            ready.notify_one();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(document: DocumentId, viewport: ViewportId) -> Request {
        let frame: Arc<zipzi_core::DrawableFrame> = Arc::default();
        Request {
            binding: ViewportBinding {
                viewport,
                kind: zipzi_editor_core::ViewportKind::Modeling,
                document,
                generation: 0,
            },
            trace_id: 0,
            revision: 0,
            canvas: None,
            animation: None,
            transform: zipzi_editor_core::ViewportTransform::new(
                zipzi_editor_core::ViewportMetrics {
                    width: 1.,
                    height: 1.,
                    scale_factor: 1.,
                },
                zipzi_editor_core::ViewState::default(),
            )
            .unwrap(),
            scene: SceneSnapshot {
                model_opacity: 1.,
                runtime_frame: frame.clone(),
                viewport_frame: frame,
                textures: Arc::from([]),
            },
            target: Target {
                width: 1,
                height: 1,
            },
        }
    }

    #[test]
    fn backpressured_closed_document_does_not_return_after_retention() {
        let mut editor = zipzi_editor_core::Editor::new().unwrap();
        let closed = editor.active_id().unwrap();
        let live = editor.new_document().unwrap();
        let viewport = ViewportId::new();
        let sender = RequestSender::deferred();
        sender.send(request(closed, viewport)).unwrap();
        let in_flight = sender.take_pending().unwrap();
        sender.retain_documents(HashSet::from([live]), closed);

        let mut inbox = sender.shared.0.lock().unwrap();
        inbox.requeue_after_backpressure(in_flight);
        assert!(matches!(inbox.pop_work(), Some(PreviewWork::Retain(_))));
        assert!(inbox.pop_work().is_none());
    }

    #[test]
    fn backpressure_keeps_a_request_sent_after_retention() {
        let mut editor = zipzi_editor_core::Editor::new().unwrap();
        let closed = editor.active_id().unwrap();
        let viewport = ViewportId::new();
        let sender = RequestSender::deferred();
        sender.send(request(closed, viewport)).unwrap();
        let in_flight = sender.take_pending().unwrap();
        assert!(editor.close_document(closed, false).unwrap());
        sender.retain_documents(HashSet::new(), closed);
        let opened_later = editor.new_document().unwrap();
        sender.send(request(opened_later, viewport)).unwrap();

        let mut inbox = sender.shared.0.lock().unwrap();
        inbox.requeue_after_backpressure(in_flight);
        assert!(matches!(inbox.pop_work(), Some(PreviewWork::Retain(_))));
        assert!(
            matches!(inbox.pop_work(), Some(PreviewWork::Render(next)) if next.binding.document == opened_later)
        );
    }

    #[test]
    fn closing_a_document_keeps_only_textures_used_by_other_documents() {
        let mut editor = zipzi_editor_core::Editor::new().unwrap();
        let first = editor.active_id().unwrap();
        let second = editor.new_document().unwrap();
        let mut owners = PreviewCacheOwners::default();
        owners.0.insert(
            (first, ViewportId::new()),
            HashSet::from(["first".into(), "shared".into()]),
        );
        owners.0.insert(
            (second, ViewportId::new()),
            HashSet::from(["second".into(), "shared".into()]),
        );
        assert_eq!(
            owners.retain(&HashSet::from([second])),
            HashSet::from(["second".into(), "shared".into()])
        );
        assert!(owners.retain(&HashSet::new()).is_empty());
    }
}
