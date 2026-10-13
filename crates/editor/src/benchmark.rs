//! Opt-in window benchmark and nonblocking JSONL trace writer.
//! Synthetic events enter GPUI dispatch; they do not measure OS input delivery.
use super::*;
mod animation;
mod deformation;
use anyhow::{Context as _, ensure};
use std::{
    cell::Cell,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Writer {
    sender: mpsc::SyncSender<Option<frame_trace::Event>>,
    thread: Mutex<Option<std::thread::JoinHandle<std::io::Result<()>>>>,
}
static WRITER: OnceLock<Writer> = OnceLock::new();
static STOPPED: AtomicBool = AtomicBool::new(false);
static LOST: AtomicU64 = AtomicU64::new(0);
static IN_SINK: AtomicU64 = AtomicU64::new(0);
// Explicit trace-mode path switches retain the historical GPUI baselines.
static METAL_MESH_OVERLAY: AtomicBool = AtomicBool::new(true);
static BATCH_MESH_EDGES: AtomicBool = AtomicBool::new(true);
static HIDE_MESH_EDGES: AtomicBool = AtomicBool::new(false);
static CONFIG: OnceLock<Config> = OnceLock::new();
thread_local! { static INJECTING_POINTER: Cell<bool> = const { Cell::new(false) }; }
struct Config {
    model: PathBuf,
    samples: usize,
    warmup: usize,
    hz: u32,
    parameters_only: bool,
    deformations: bool,
    animation: bool,
    vertices: usize,
    mesh: Option<String>,
    warp: Option<String>,
    rotation: Option<String>,
}

pub fn init() -> anyhow::Result<()> {
    let automatic = std::env::var("KASANE_WINDOW_BENCH").is_ok_and(|s| s == "1");
    let Some(path) = std::env::var_os("KASANE_FRAME_TRACE") else {
        ensure!(
            !automatic,
            "KASANE_WINDOW_BENCH requires KASANE_FRAME_TRACE"
        );
        return Ok(());
    };
    for (name, valid) in [
        ("GPUI_TRACE_DRAWABLES", ["2", "3"]),
        ("GPUI_TRACE_DISPLAY_SYNC", ["0", "1"]),
        ("GPUI_TRACE_OFFSCREEN", ["0", "1"]),
        ("KASANE_TRACE_HIDE_MESH_EDGES", ["0", "1"]),
        ("KASANE_TRACE_BATCH_MESH_EDGES", ["0", "1"]),
    ] {
        if let Ok(value) = std::env::var(name) {
            ensure!(valid.contains(&value.as_str()), "invalid {name}");
        }
    }
    METAL_MESH_OVERLAY.store(
        std::env::var_os("KASANE_TRACE_BATCH_MESH_EDGES").is_none(),
        Ordering::Relaxed,
    );
    HIDE_MESH_EDGES.store(
        std::env::var("KASANE_TRACE_HIDE_MESH_EDGES").is_ok_and(|v| v == "1"),
        Ordering::Relaxed,
    );
    BATCH_MESH_EDGES.store(
        !std::env::var("KASANE_TRACE_BATCH_MESH_EDGES").is_ok_and(|v| v == "0"),
        Ordering::Relaxed,
    );
    if automatic {
        let number = |name: &str, fallback: usize| -> anyhow::Result<usize> {
            Ok(std::env::var(name)
                .map(|s| s.parse())
                .unwrap_or(Ok(fallback))?)
        };
        let samples = number("KASANE_BENCH_SAMPLES", 300)?;
        let warmup = number("KASANE_BENCH_WARMUP", 30)?;
        let hz = number("KASANE_BENCH_HZ", 120)?;
        ensure!(
            (1..=100_000).contains(&samples) && warmup <= 10_000,
            "invalid sample count"
        );
        ensure!((1..=1000).contains(&hz), "invalid benchmark frequency");
        let model = PathBuf::from(
            std::env::var_os("KASANE_BENCH_MODEL").context("KASANE_BENCH_MODEL is required")?,
        )
        .canonicalize()?;
        let parameters_only = std::env::var("KASANE_BENCH_PARAMETERS").is_ok_and(|s| s == "1");
        let deformations = std::env::var("KASANE_BENCH_DEFORMATIONS").is_ok_and(|s| s == "1");
        let animation = std::env::var("KASANE_BENCH_ANIMATION").is_ok_and(|s| s == "1");
        ensure!(
            (parameters_only as u8 + deformations as u8 + animation as u8) <= 1,
            "choose one benchmark workload"
        );
        let vertices = number("KASANE_BENCH_VERTICES", 8)?;
        ensure!(
            (2..=10_000).contains(&vertices),
            "invalid multi-vertex count"
        );
        CONFIG
            .set(Config {
                model,
                samples,
                warmup,
                hz: hz as u32,
                parameters_only,
                deformations,
                animation,
                vertices,
                mesh: std::env::var("KASANE_BENCH_MESH").ok(),
                warp: std::env::var("KASANE_BENCH_WARP").ok(),
                rotation: std::env::var("KASANE_BENCH_ROTATION").ok(),
            })
            .ok();
    }
    // Refuse to overwrite an earlier run's measurements.
    let file = File::options().write(true).create_new(true).open(path)?;
    let (sender, receiver) = mpsc::sync_channel::<Option<frame_trace::Event>>(65_536);
    let thread = std::thread::spawn(move || -> std::io::Result<()> {
        let mut out = BufWriter::new(file);
        let config = CONFIG.get();
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "type": "metadata", "schema": 1, "clock": "mach_host_nanoseconds",
                "debug_assertions": cfg!(debug_assertions), "automatic": automatic,
                "gpui_backend": "gpui-fast",
                "view_retention": std::env::var("GPUI_VIEW_RETENTION").map_or(true, |v| v != "0"),
                "scroll_layers": std::env::var("GPUI_SCROLL_LAYERS").map_or(true, |v| v != "0"),
                "model": config.map(|c| c.model.display().to_string()),
                "samples": config.map(|c| c.samples), "warmup": config.map(|c| c.warmup),
                "parameters_only": config.map(|c| c.parameters_only),
                "workloads": config.map(|c| if c.animation { vec![9] } else if c.deformations { vec![5, 6, 7, 8] }
                    else if c.parameters_only { vec![4] } else { vec![1, 2, 3] }),
                "multi_vertices": config.map(|c| c.vertices),
                "input_hz": config.map(|c| c.hz), "pid": std::process::id(),
                "diagnostics": {
                    "drawables": std::env::var("GPUI_TRACE_DRAWABLES").ok(),
                    "display_sync": std::env::var("GPUI_TRACE_DISPLAY_SYNC").ok(),
                    "offscreen": std::env::var("GPUI_TRACE_OFFSCREEN").ok(),
                    "default_worker_qos": std::env::var("KASANE_TRACE_DEFAULT_WORKER_QOS").is_ok_and(|v| v == "1"),
                    "hide_mesh_edges": HIDE_MESH_EDGES.load(Ordering::Relaxed),
                    "batch_mesh_edges": BATCH_MESH_EDGES.load(Ordering::Relaxed),
                    "metal_mesh_overlay": METAL_MESH_OVERLAY.load(Ordering::Relaxed),
                },
            })
        )?;
        loop {
            match receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(Some(event)) => {
                    let fields: serde_json::Map<_, _> = event
                        .fields
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value.into()))
                        .collect();
                    writeln!(
                        out,
                        "{}",
                        serde_json::json!({
                            "type": "event", "at_ns": event.at_ns, "id": event.id,
                            "stage": event.stage, "fields": fields,
                        })
                    )?;
                }
                Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => out.flush()?,
            }
        }
        writeln!(
            out,
            "{}",
            serde_json::json!({"type": "end", "lost_events": LOST.load(Ordering::Relaxed)})
        )?;
        out.flush()
    });
    WRITER
        .set(Writer {
            sender,
            thread: Mutex::new(Some(thread)),
        })
        .ok();
    ensure!(
        frame_trace::install(|event| {
            IN_SINK.fetch_add(1, Ordering::SeqCst);
            if !STOPPED.load(Ordering::SeqCst) {
                if let Some(writer) = WRITER.get() {
                    if writer.sender.try_send(Some(event)).is_err() {
                        LOST.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            IN_SINK.fetch_sub(1, Ordering::SeqCst);
        }),
        "frame trace sink already installed"
    );
    Ok(())
}

pub fn metal_mesh_overlay() -> bool {
    METAL_MESH_OVERLAY.load(Ordering::Relaxed)
}

pub fn batch_mesh_edges() -> bool {
    BATCH_MESH_EDGES.load(Ordering::Relaxed)
}

pub fn hide_mesh_edges() -> bool {
    HIDE_MESH_EDGES.load(Ordering::Relaxed)
}

// AppKit may deliver an unpressed hover move between synthetic drag samples.
// Let scripted pointer events own the canvas during an automatic benchmark.
// Manual tracing and ordinary editor sessions keep all native mouse behavior.
pub fn ignore_native_mouse_move() -> bool {
    CONFIG.get().is_some() && !INJECTING_POINTER.get()
}

fn dispatch_pointer(
    window: &mut Window,
    event: PlatformInput,
    cx: &mut App,
) -> DispatchEventResult {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            INJECTING_POINTER.set(self.0);
        }
    }
    let _restore = Restore(INJECTING_POINTER.replace(true));
    window.dispatch_event(event, cx)
}

pub fn finish() {
    if STOPPED.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(writer) = WRITER.get() {
        // Finish only after callbacks that observed the open sink have enqueued.
        while IN_SINK.load(Ordering::SeqCst) != 0 {
            std::thread::yield_now();
        }
        let _ = writer.sender.send(None);
        if let Some(thread) = writer.thread.lock().unwrap().take() {
            match thread.join() {
                Ok(Ok(())) => {}
                error => eprintln!("frame trace writer failed: {error:?}"),
            }
        }
    }
}

pub fn start(view: &mut EditorView, window: &mut Window, cx: &mut Context<EditorView>) {
    let Some(config) = CONFIG.get() else {
        return;
    };
    let request = if config.model.to_string_lossy().ends_with(".model3.json") {
        LoadRequest::Model3(config.model.clone())
    } else {
        LoadRequest::Open(config.model.clone())
    };
    let task = match view.editor.prepare_load(request) {
        Ok(Some(task)) => task,
        other => {
            eprintln!(
                "benchmark could not prepare model: {}",
                other
                    .err()
                    .map_or_else(|| "no load task".into(), |e| e.to_string())
            );
            frame_trace::emit(frame_trace::next_id(), "benchmark_failed", &[]);
            cx.defer(|cx| {
                finish();
                cx.quit();
            });
            return;
        }
    };
    cx.spawn_in(window, async move |view, cx| {
        let result: anyhow::Result<()> = async {
            let executor = cx.background_executor().clone();
            let completion = executor.spawn(async move { task.run() }).await;
            view.update_in(cx, |view, window, cx| -> anyhow::Result<()> {
                view.editor.finish_load(completion)?;
                if let Some(layer) = view
                    .editor
                    .layers()
                    .into_iter()
                    .find(|l| l.visible && l.vertices > 0)
                {
                    view.editor.execute(EditorCommand::Select(layer.id))?;
                }
                view.refresh_document(window, cx);
                window.activate_window();
                cx.activate(true);
                Ok(())
            })??;
            // Await a usable frame, not just import completion. First uploads are cold work.
            let ready_deadline = Instant::now() + Duration::from_secs(30);
            loop {
                executor.timer(Duration::from_millis(25)).await;
                if view.update_in(cx, |v, _, _| v.frame.is_some())? {
                    break;
                }
                ensure!(
                    Instant::now() < ready_deadline,
                    "timed out waiting for first preview"
                );
            }
            executor.timer(Duration::from_millis(500)).await;
            let modes = if config.animation {
                animation::run(&view, cx, config).await?;
                1..=0u64
            } else if config.deformations {
                5..=8u64
            } else if config.parameters_only {
                4..=4u64
            } else {
                1..=3u64
            };
            for mode in modes {
                let center = view
                    .update_in(cx, |view, window, cx| {
                        view.editor.cancel_gesture();
                        view.editor.reset_view();
                        view.redraw(cx);
                        view.dock
                            .as_ref()
                            .unwrap()
                            .canvas
                            .read(cx)
                            .benchmark_center()
                            .map(|center| (center, window.is_visible()))
                    })?
                    .context("canvas has no measured bounds")?;
                ensure!(center.1, "benchmark window is hidden");
                let drag = if mode >= 5 {
                    // Pick from the restored model and viewport actually displayed.
                    let deadline = Instant::now() + Duration::from_secs(30);
                    while !view.update_in(cx, |view, _, _| deformation::frame_ready(view))? {
                        ensure!(
                            Instant::now() < deadline,
                            "timed out waiting for deformation fixture"
                        );
                        executor.timer(Duration::from_millis(25)).await;
                    }
                    Some(view.update_in(cx, |view, _, cx| deformation::prepare(view, mode, cx))??)
                } else {
                    None
                };
                let slider = if mode == 4 {
                    Some(
                        view.update_in(cx, |view, _, cx| {
                            view.dock
                                .as_ref()
                                .unwrap()
                                .parameters
                                .read(cx)
                                .benchmark_slider(cx)
                        })?
                        .context("parameters panel has no visible slider")?,
                    )
                } else {
                    None
                };
                // Press the actual thumb: a fixed fraction can hit a keyframe
                // marker, which selects a value without starting a drag.
                let center = slider.map_or(center.0, |(_, thumb)| thumb);
                let center = drag.as_ref().map_or(center, |drag| drag.start);
                executor.timer(Duration::from_millis(250)).await;
                if let Some(drag) = &drag {
                    cx.update(|window, cx| {
                        deformation::select_components(&view, drag, window, cx)
                    })??;
                }
                if mode == 1 || mode == 4 || mode >= 5 {
                    cx.update(|window, cx| {
                        dispatch_pointer(
                            window,
                            MouseDownEvent {
                                button: if mode >= 4 {
                                    MouseButton::Left
                                } else {
                                    MouseButton::Middle
                                },
                                position: center,
                                click_count: 1,
                                modifiers: Default::default(),
                                first_mouse: false,
                            }
                            .to_platform_input(),
                            cx,
                        )
                    })?;
                }
                let interval = Duration::from_secs_f64(1. / config.hz as f64);
                let mut deadline = Instant::now() + interval;
                let mut last_position = center;
                for sample in 0..config.samples + config.warmup {
                    executor
                        .timer(deadline.saturating_duration_since(Instant::now()))
                        .await;
                    let late_ns = Instant::now()
                        .saturating_duration_since(deadline)
                        .as_nanos() as u64;
                    // Slider subscriptions run when App::update flushes effects, after
                    // dispatch_event returns. Keep attribution alive through that flush.
                    let evaluations = if drag.is_some() {
                        view.update_in(cx, |view, _, _| view.editor.evaluation_count())?
                    } else {
                        0
                    };
                    let input = frame_trace::InputScope::begin(100 + mode);
                    cx.update(|window, cx| -> anyhow::Result<()> {
                        frame_trace::emit(
                            input.id(),
                            "benchmark_tick",
                            &[
                                ("mode", mode),
                                ("sample", sample as u64),
                                ("warmup", u64::from(sample < config.warmup)),
                                ("timer_late_ns", late_ns),
                            ],
                        );
                        ensure!(
                            window.is_visible() && window.is_window_active(),
                            "benchmark window lost visibility or focus"
                        );
                        match mode {
                            1 => {
                                dispatch_pointer(
                                    window,
                                    MouseMoveEvent {
                                        position: center
                                            + point(
                                                px((sample as f32 * 0.13).sin() * 60.),
                                                px((sample as f32 * 0.07).cos() * 30.),
                                            ),
                                        pressed_button: Some(MouseButton::Middle),
                                        modifiers: Default::default(),
                                    }
                                    .to_platform_input(),
                                    cx,
                                );
                            }
                            2 => {
                                dispatch_pointer(
                                    window,
                                    ScrollWheelEvent {
                                        position: center,
                                        delta: ScrollDelta::Lines(point(
                                            0.,
                                            if sample % 40 < 20 { 1. } else { -1. },
                                        )),
                                        ..Default::default()
                                    }
                                    .to_platform_input(),
                                    cx,
                                );
                            }
                            4 => {
                                let (bounds, _) = slider.unwrap();
                                dispatch_pointer(
                                    window,
                                    MouseMoveEvent {
                                        position: point(
                                            bounds.left()
                                                + bounds.size.width
                                                    * (0.5 + (sample as f32 * 0.13).sin() * 0.35),
                                            center.y,
                                        ),
                                        pressed_button: Some(MouseButton::Left),
                                        modifiers: Default::default(),
                                    }
                                    .to_platform_input(),
                                    cx,
                                );
                            }
                            5..=8 => {
                                last_position = drag.as_ref().unwrap().position(sample);
                                dispatch_pointer(
                                    window,
                                    MouseMoveEvent {
                                        position: last_position,
                                        pressed_button: Some(MouseButton::Left),
                                        modifiers: Default::default(),
                                    }
                                    .to_platform_input(),
                                    cx,
                                );
                            }
                            _ => {
                                view.update(cx, |view, cx| -> anyhow::Result<()> {
                                    // Real SDK edit/evaluation; bypasses only the slider widget.
                                    view.editor.execute(EditorCommand::PreviewOpacity(
                                        0.5 + (sample % 50) as f32 * 0.01,
                                    ))?;
                                    view.redraw_canvas(cx);
                                    Ok(())
                                })??;
                            }
                        }
                        Ok(())
                    })??;
                    drop(input);
                    if drag.is_some() {
                        view.update_in(cx, |view, _, _| -> anyhow::Result<()> {
                            ensure!(
                                view.edit_error.is_none(),
                                "canvas edit failed: {:?}",
                                view.edit_error
                            );
                            ensure!(
                                view.editor.gesture_active()
                                    && view.editor.document().is_some_and(|d| d.has_pending_edit())
                                    && view.editor.evaluation_count() > evaluations,
                                "mode {mode} sample {sample}: drag produced no model evaluation"
                            );
                            Ok(())
                        })??;
                    }
                    // Never inject a burst of catch-up events after a late wake-up.
                    deadline += interval;
                    if deadline < Instant::now() {
                        deadline = Instant::now() + interval;
                    }
                }
                if mode == 1 || mode == 4 || mode >= 5 {
                    cx.update(|window, cx| {
                        dispatch_pointer(
                            window,
                            MouseUpEvent {
                                button: if mode >= 4 {
                                    MouseButton::Left
                                } else {
                                    MouseButton::Middle
                                },
                                position: last_position,
                                ..Default::default()
                            }
                            .to_platform_input(),
                            cx,
                        )
                    })?;
                }
                executor.timer(Duration::from_millis(500)).await;
                if drag.is_some() {
                    view.update_in(cx, |view, _, cx| -> anyhow::Result<()> {
                        ensure!(view.edit_error.is_none(), "could not commit canvas gesture");
                        view.editor.execute(EditorCommand::Undo)?;
                        view.redraw(cx);
                        Ok(())
                    })??;
                }
            }
            executor.timer(Duration::from_secs(2)).await;
            frame_trace::emit(frame_trace::next_id(), "benchmark_done", &[]);
            Ok(())
        }
        .await;
        if let Err(error) = result {
            eprintln!("window benchmark failed: {error:#}");
            frame_trace::emit(frame_trace::next_id(), "benchmark_failed", &[]);
        }
        finish();
        let _ = cx.update(|_, cx| cx.quit());
    })
    .detach();
}
