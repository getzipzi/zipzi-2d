//! A fresh native window's first model frame, joined through its preview label.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

static ENABLED: AtomicBool = AtomicBool::new(false);
static TRACKER: OnceLock<Mutex<Tracker>> = OnceLock::new();

pub(super) fn init(automatic: bool) -> anyhow::Result<()> {
    let active = match std::env::var("ZIPZI_BENCH_FIRST_FRAME") {
        Err(std::env::VarError::NotPresent) => false,
        Ok(value) if value == "1" => true,
        _ => anyhow::bail!("ZIPZI_BENCH_FIRST_FRAME must be 1 or unset"),
    };
    ensure!(
        !active || automatic,
        "first-frame mode requires ZIPZI_WINDOW_BENCH=1"
    );
    ENABLED.store(active, Ordering::Relaxed);
    Ok(())
}

pub(super) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Presented {
    at_ns: u64,
    composition: u64,
    preview: u64,
}

#[derive(Default)]
struct Tracker {
    accepted: BTreeSet<u64>,
    compositions: BTreeMap<u64, u64>,
    preview_gpu_ends: BTreeSet<u64>,
    compose_gpu_ends: BTreeSet<u64>,
    redraw: Option<(u64, u64)>,
    first: Option<Presented>,
}

impl Tracker {
    fn observe(&mut self, event: &frame_trace::Event) {
        match event.stage {
            "accepted" => {
                self.accepted.insert(event.id);
            }
            "compose_surface" => {
                if let Some((_, preview)) = event.fields.iter().find(|(key, _)| *key == "preview") {
                    if *preview != 0 && self.accepted.contains(preview) {
                        self.compositions.insert(event.id, *preview);
                    }
                }
            }
            "preview_gpu_end" => {
                self.preview_gpu_ends.insert(event.id);
            }
            "compose_gpu_end" => {
                self.compose_gpu_ends.insert(event.id);
            }
            "presented" => {
                if let Some(&preview) = self.compositions.get(&event.id) {
                    if self.first.is_none_or(|first| event.at_ns < first.at_ns) {
                        self.first = Some(Presented {
                            at_ns: event.at_ns,
                            composition: event.id,
                            preview,
                        });
                    }
                }
            }
            "present_dropped" => {
                if let Some(&preview) = self.compositions.get(&event.id) {
                    self.redraw = Some((event.id, preview));
                }
            }
            _ => {}
        }
    }

    fn ready(&self) -> Option<Presented> {
        self.first.filter(|first| {
            self.preview_gpu_ends.contains(&first.preview)
                && self.compose_gpu_ends.contains(&first.composition)
        })
    }
}

pub(super) fn observe(event: &frame_trace::Event) {
    if enabled() {
        TRACKER
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .observe(event);
    }
}

pub(super) fn start(
    view: &mut EditorView,
    window: &mut Window,
    cx: &mut Context<EditorView>,
    config: &'static Config,
) {
    let load = frame_trace::next_id();
    frame_trace::emit(load, "load_begin", &[]);
    let request = if config.model.to_string_lossy().ends_with(".model3.json") {
        LoadRequest::Model3(config.model.clone())
    } else if config.model.extension().is_some_and(|s| s == "psd") {
        LoadRequest::Psd(config.model.clone())
    } else if config.model.extension().is_some_and(|s| s == "cmo3") {
        LoadRequest::Cubism(config.model.clone())
    } else {
        LoadRequest::Open(config.model.clone())
    };
    let task = (|| -> anyhow::Result<_> {
        ensure!(
            view.editor.document_count() == 0,
            "first-frame mode requires an empty workspace"
        );
        view.editor
            .prepare_load(request)?
            .context("no fresh load task")
    })();
    let task = match task {
        Ok(task) => task,
        Err(error) => {
            eprintln!("first-frame benchmark failed: {error:#}");
            frame_trace::emit(load, "benchmark_failed", &[]);
            cx.defer(|cx| {
                finish();
                cx.quit();
            });
            return;
        }
    };
    frame_trace::emit(load, "load_admitted", &[]);
    let runtime = view.editor.runtime_host();
    let host = view.host.clone();
    cx.spawn_in(window, async move |view, cx| {
        let result: anyhow::Result<()> = async {
            let executor = cx.background_executor().clone();
            let mut completion = executor
                .spawn(async move {
                    frame_trace::emit(load, "load_worker_begin", &[]);
                    let completion = task.run();
                    frame_trace::emit(load, "load_worker_end", &[]);
                    completion
                })
                .await;
            completion.publish(&runtime);
            if let Some(host) = &host {
                host.completed();
            }
            view.update_in(cx, |view, window, cx| -> anyhow::Result<()> {
                frame_trace::emit(load, "load_attach_begin", &[]);
                let notices = view.editor.finish_load(completion)?;
                // Preserve the ordinary empty selection; selecting a mesh adds
                // cold overlay work unrelated to the normal opening path.
                view.refresh_document(window, cx);
                frame_trace::emit(load, "load_attached", &[("notices", notices.len() as u64)]);
                window.activate_window();
                cx.activate(true);
                Ok(())
            })??;
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                executor.timer(Duration::from_millis(10)).await;
                let (first, redraw) = {
                    let mut tracker = TRACKER.get_or_init(Default::default).lock().unwrap();
                    (tracker.ready(), tracker.redraw.take())
                };
                if first.is_none() {
                    if let Some((composition, preview)) = redraw {
                        frame_trace::emit(
                            load,
                            "first_frame_redraw",
                            &[("composition", composition), ("preview", preview)],
                        );
                        // Redraw the retained image after a dropped drawable.
                        // Do not invalidate/evaluate/render another model frame.
                        view.update_in(cx, |_, window, _| window.refresh())?;
                    }
                }
                view.update_in(cx, |v, _, _| -> anyhow::Result<()> {
                    ensure!(
                        v.render_error().is_none(),
                        "viewport failed: {:?}",
                        v.render_error()
                    );
                    Ok(())
                })??;
                if let Some(first) = first {
                    ensure!(
                        view.update_in(cx, |v, window, _| window.is_visible()
                            && v.modeling_frame().is_some())?,
                        "first model frame is not visible"
                    );
                    frame_trace::emit_at(
                        load,
                        "first_frame_presented",
                        first.at_ns,
                        &[
                            ("preview", first.preview),
                            ("composition", first.composition),
                        ],
                    );
                    // Allow completion callbacks to reach the trace before exit;
                    // the presentation timestamp, not this timer, ends timing.
                    executor.timer(Duration::from_millis(100)).await;
                    frame_trace::emit(load, "benchmark_done", &[]);
                    return Ok(());
                }
                ensure!(
                    Instant::now() < deadline,
                    "timed out waiting for a presented model frame"
                );
            }
        }
        .await;
        if let Err(error) = result {
            eprintln!("first-frame benchmark failed: {error:#}");
            frame_trace::emit(load, "benchmark_failed", &[]);
        }
        finish();
        let _ = cx.update(|_, cx| cx.quit());
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::{Presented, Tracker, frame_trace};

    fn event(id: u64, stage: &'static str, at_ns: u64, preview: u64) -> frame_trace::Event {
        frame_trace::Event {
            id,
            stage,
            at_ns,
            fields: vec![("preview", preview)],
        }
    }

    #[test]
    fn first_frame_requires_accepted_surface_gpu_completion_and_presentation() {
        let mut tracker = Tracker::default();
        tracker.observe(&event(1, "compose_surface", 1, 0));
        tracker.observe(&event(1, "presented", 2, 0)); // Placeholder window.
        tracker.observe(&event(2, "compose_surface", 3, 9)); // Unaccepted preview.
        tracker.observe(&event(2, "presented", 4, 0));
        assert!(tracker.first.is_none());
        tracker.observe(&event(9, "accepted", 5, 0));
        tracker.observe(&event(3, "compose_surface", 6, 9));
        tracker.observe(&event(3, "present_dropped", 7, 0));
        assert!(tracker.first.is_none());
        assert_eq!(tracker.redraw.take(), Some((3, 9)));
        tracker.observe(&event(4, "compose_surface", 8, 9));
        tracker.observe(&event(4, "presented", 10, 0));
        assert!(tracker.ready().is_none());
        tracker.observe(&event(9, "preview_gpu_end", 6, 0));
        assert!(tracker.ready().is_none());
        tracker.observe(&event(4, "compose_gpu_end", 9, 0));
        assert_eq!(
            tracker.ready(),
            Some(Presented {
                at_ns: 10,
                composition: 4,
                preview: 9
            })
        );
    }

    #[test]
    fn out_of_order_callbacks_use_the_earliest_actual_presentation() {
        let mut tracker = Tracker::default();
        tracker.observe(&event(9, "accepted", 1, 0));
        for id in [2, 3] {
            tracker.observe(&event(id, "compose_surface", 2, 9));
        }
        tracker.observe(&event(3, "presented", 20, 0));
        tracker.observe(&event(2, "presented", 10, 0));
        tracker.observe(&event(9, "preview_gpu_end", 3, 0));
        tracker.observe(&event(3, "compose_gpu_end", 4, 0));
        assert!(tracker.ready().is_none()); // Wait for the first composition.
        tracker.observe(&event(2, "compose_gpu_end", 4, 0));
        assert_eq!(tracker.ready().unwrap().composition, 2);
    }
}
