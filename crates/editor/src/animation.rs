//! One background SDK evaluator with a bounded, replaceable request slot.
use crate::*;
use editor_core::{
    AnimationEvaluated, AnimationEvaluator, AnimationIdentity, AnimationRequest, WorkspaceKind,
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

enum Work {
    Evaluate(AnimationRequest, u64),
    Suspend,
}
#[derive(Default)]
struct Inbox {
    latest: Option<Work>,
    closed: bool,
}
pub struct AnimationWorker {
    shared: Arc<(Mutex<Inbox>, Condvar)>,
    cancellation: Arc<AtomicU64>,
    key: Option<(editor_core::DocumentId, u64, u64, u64, u64)>,
}
pub struct ResultFrame {
    pub trace_id: u64,
    pub identity: AnimationIdentity,
    pub result: anyhow::Result<AnimationEvaluated>,
}
impl AnimationWorker {
    pub fn start() -> (Self, async_channel::Receiver<ResultFrame>) {
        let shared = Arc::new((Mutex::new(Inbox::default()), Condvar::new()));
        let cancellation = Arc::new(AtomicU64::new(0));
        let worker = Self {
            shared: shared.clone(),
            cancellation: cancellation.clone(),
            key: None,
        };
        let (sender, receiver) = async_channel::bounded(1);
        std::thread::Builder::new()
            .name("kasane-animation".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let mut evaluator = AnimationEvaluator::default();
                loop {
                    let (work, version) = {
                        let (lock, ready) = &*shared;
                        let mut inbox = lock.lock().unwrap();
                        while inbox.latest.is_none() && !inbox.closed {
                            inbox = ready.wait(inbox).unwrap();
                        }
                        if inbox.closed {
                            break;
                        }
                        let work = inbox.latest.take().unwrap();
                        if let Work::Evaluate(_, trace_id) = &work {
                            frame_trace::emit(*trace_id, "animation_dequeued", &[]);
                        }
                        (work, cancellation.load(Ordering::Acquire))
                    };
                    let Work::Evaluate(request, trace_id) = work else {
                        evaluator = AnimationEvaluator::default();
                        continue;
                    };
                    let identity = request.identity;
                    frame_trace::emit(trace_id, "animation_evaluate_begin", &[]);
                    let result = evaluator.evaluate(&request, |_, _| {
                        cancellation.load(Ordering::Acquire) == version
                    });
                    frame_trace::emit(
                        trace_id,
                        "animation_evaluate_end",
                        &[("success", result.is_ok() as u64)],
                    );
                    if trace_id != 0 {
                        if let Some(stats) = evaluator.seek_cache_stats() {
                            frame_trace::emit(
                                trace_id,
                                "animation_cache",
                                &[
                                    ("bytes", stats.estimated_bytes as u64),
                                    ("budget", stats.budget_bytes as u64),
                                    ("checkpoints", stats.checkpoints as u64),
                                ],
                            );
                        }
                    }
                    if cancellation.load(Ordering::Acquire) != version {
                        frame_trace::emit(trace_id, "animation_cancelled", &[]);
                        continue;
                    }
                    if sender
                        .send_blocking(ResultFrame {
                            trace_id,
                            identity,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("start animation worker");
        (worker, receiver)
    }
    pub fn send(&mut self, request: AnimationRequest) {
        let id = request.identity;
        let key = (
            id.document,
            id.workspace_revision,
            id.model_generation,
            id.preview_generation,
            id.acceptance_floor,
        );
        let (lock, ready) = &*self.shared;
        let mut inbox = lock.lock().unwrap();
        // Publish the recipe and its cancellation epoch under the same lock.
        // Otherwise a worker could take an old slot with the new epoch and run
        // a long obsolete replay without noticing the replacement.
        if self.key != Some(key) {
            self.cancellation.fetch_add(1, Ordering::AcqRel);
            self.key = Some(key);
        }
        let trace_id = frame_trace::next_id();
        if let Some(Work::Evaluate(_, old_trace)) =
            inbox.latest.replace(Work::Evaluate(request, trace_id))
        {
            frame_trace::emit(old_trace, "animation_coalesced", &[]);
        }
        frame_trace::emit(
            trace_id,
            "animation_enqueued",
            &[
                ("request", id.request),
                ("time_us", (id.time as f64 * 1e6) as u64),
            ],
        );
        ready.notify_one();
    }
    pub fn suspend(&mut self) {
        if self.key.take().is_none() {
            return;
        }
        let (lock, ready) = &*self.shared;
        let mut inbox = lock.lock().unwrap();
        self.cancellation.fetch_add(1, Ordering::AcqRel);
        if let Some(Work::Evaluate(_, trace_id)) = inbox.latest.replace(Work::Suspend) {
            frame_trace::emit(trace_id, "animation_cancelled", &[]);
        }
        ready.notify_one();
    }
}
impl Drop for AnimationWorker {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        let mut inbox = lock.lock().unwrap();
        self.cancellation.fetch_add(1, Ordering::AcqRel);
        inbox.closed = true;
        if let Some(Work::Evaluate(_, trace_id)) = inbox.latest.take() {
            frame_trace::emit(trace_id, "animation_cancelled", &[]);
        }
        ready.notify_one();
    }
}

#[derive(Clone)]
pub struct Presentation {
    pub identity: AnimationIdentity,
    pub snapshot: Arc<kasane_sdk::MotionSnapshot>,
    pub transform: editor_core::ViewportTransform,
}
#[derive(Clone)]
pub struct Presented {
    pub image: preview::Image,
    pub state: Presentation,
}

impl EditorView {
    pub(crate) fn displayed_animation(&self) -> Option<&kasane_sdk::MotionSnapshot> {
        self.animation_frame
            .as_ref()
            .filter(|frame| self.editor.accepts_animation_image(frame.state.identity))
            .map(|frame| frame.state.snapshot.as_ref())
            .or_else(|| self.editor.animation().and_then(|a| a.presented.as_deref()))
    }
    pub(super) fn init_animation(&mut self, cx: &mut Context<Self>) {
        let (worker, results) = AnimationWorker::start();
        self.animation_worker = Some(worker);
        self._animation_results = Some(cx.spawn(async move |this, cx| {
            while let Ok(frame) = results.recv().await {
                if this
                    .update(cx, |this, cx| {
                        if !this.editor.accepts_animation(frame.identity) {
                            frame_trace::emit(frame.trace_id, "animation_rejected_cpu", &[]);
                            return;
                        }
                        match frame.result {
                            Ok(evaluated) => this.render_animation(evaluated, frame.trace_id),
                            Err(error) => {
                                this.editor.fail_animation(
                                    frame.identity,
                                    editor_core::AnimationFailure::from_error(&error),
                                );
                                this.notify_animation(cx);
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    fn render_animation(&mut self, evaluated: AnimationEvaluated, trace_id: u64) {
        let Some(transform) = editor_core::ViewportTransform::new(
            evaluated.scene.viewport_frame.canvas,
            self.editor.canvas_metrics(),
            self.editor.view(),
        ) else {
            return;
        };
        let Some((width, height)) = transform.metrics.physical_size() else {
            return;
        };
        let mut identity = evaluated.identity;
        identity.viewport_revision = self
            .editor
            .animation_viewport_revision()
            .unwrap_or(identity.viewport_revision);
        let state = Presentation {
            identity,
            snapshot: evaluated.snapshot,
            transform,
        };
        let request = preview::Request {
            trace_id,
            revision: identity.request,
            canvas: None,
            animation: Some(state),
            transform,
            scene: evaluated.scene,
            target: editor_render::MetalTargetConfig {
                width,
                height,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            },
        };
        if self
            .requests
            .as_ref()
            .is_some_and(|requests| requests.send(request).is_err())
        {
            self.editor
                .fail_animation(identity, editor_core::AnimationFailure::RendererStopped);
        }
    }
    pub(super) fn sync_animation_clock(&mut self, cx: &mut Context<Self>) {
        let playing = self.editor.workspace() == WorkspaceKind::Animation
            && self
                .editor
                .animation()
                .is_some_and(|animation| animation.playing);
        if !playing {
            self.animation_anchor = None;
            return;
        }
        if self.animation_anchor.is_none() {
            self.animation_anchor = Some((Instant::now(), self.editor.animation().unwrap().time));
        }
        if self.animation_clock_running {
            return;
        }
        self.animation_clock_running = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep = this
                    .update(cx, |this, _cx| {
                        if !this
                            .editor
                            .animation()
                            .is_some_and(|animation| animation.playing)
                            || this.editor.workspace() != WorkspaceKind::Animation
                        {
                            this.animation_clock_running = false;
                            this.animation_anchor = None;
                            return false;
                        }
                        if let Some((anchor, time)) = this.animation_anchor {
                            let target =
                                ((time + anchor.elapsed().as_secs_f32()) * 60.).floor() / 60.;
                            this.editor.advance_animation_clock(target);
                            this.request_preview();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationEvaluated, AnimationIdentity, AnimationWorker, Duration, Instant, WorkspaceKind,
    };
    #[test]
    fn worker_cancels_obsolete_recipe_and_delivers_latest_target_after_suspension()
    -> anyhow::Result<()> {
        let (document, _assets) = crate::headless_tests::document_fixture::animation_document()?;
        let mut editor = editor_core::Editor::from_document(document);
        editor.set_workspace(WorkspaceKind::Animation)?;
        editor.set_canvas_metrics(editor_core::CanvasMetrics {
            width: 256.,
            height: 256.,
            scale_factor: 1.,
        });
        let resources = editor.document().unwrap().motion_resources();
        editor.select_motion(editor.active_id().unwrap(), resources[2].selection.clone())?;
        editor.toggle_animation()?;
        editor.advance_animation_clock(10_000.);
        let (mut worker, receiver) = AnimationWorker::start();
        worker.send(editor.animation_request().unwrap());
        editor.select_motion(editor.active_id().unwrap(), resources[0].selection.clone())?;
        let request = editor.animation_request().unwrap();
        let target = request.identity;
        worker.send(request);
        let receive = |target: AnimationIdentity| -> anyhow::Result<AnimationEvaluated> {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Ok(frame) = receiver.try_recv() {
                    if frame.identity == target {
                        return frame.result;
                    }
                }
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "Latest animation target was starved by obsolete work"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        assert_eq!(receive(target)?.snapshot.time, 0.);
        // A long seek within the same recipe must also yield to a newer scrub.
        editor.invalidate_animation();
        let mut long = editor.animation_request().unwrap();
        long.identity.time = 10_000.;
        worker.send(long);
        let document = editor.active_id().unwrap();
        for time in [1.5, 0.2, 1., 0.4] {
            editor.seek_animation(document, time)?;
            worker.send(editor.animation_request().unwrap());
        }
        // Reissue the current target after another barrier to obtain its identity.
        editor.seek_animation(document, 0.4)?;
        let request = editor.animation_request().unwrap();
        let expected =
            editor_core::AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        let target = request.identity;
        worker.send(request);
        assert_eq!(receive(target)?.snapshot, expected.snapshot);
        worker.suspend();
        editor.invalidate_animation();
        let request = editor.animation_request().unwrap();
        let target = request.identity;
        worker.send(request);
        assert_eq!(receive(target)?.snapshot.time, 0.4);
        Ok(())
    }
}
