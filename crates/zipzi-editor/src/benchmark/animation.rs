//! Native window playback: the application's display callbacks, GPU workers and compositor.
use super::*;

pub(super) async fn run(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
) -> anyhow::Result<()> {
    let drag = std::env::var("ZIPZI_BENCH_ANIMATION_DRAG").ok();
    ensure!(
        !drag
            .as_deref()
            .is_some_and(|kind| kind.starts_with("timeline_"))
            || cfg!(feature = "test-support"),
        "Timeline navigation probes require test-support"
    );
    ensure!(
        drag.as_deref().is_none_or(|v| matches!(
            v,
            "canvas"
                | "number"
                | "timeline_scroll"
                | "timeline_zoom"
                | "timeline_seek"
                | "timeline_key"
        )),
        "invalid animation drag workload"
    );
    let baseline = view.update_in(cx, |view, window, cx| -> anyhow::Result<_> {
        view.run_command(zipzi_editor_core::CommandId::AnimationWorkspace, window, cx);
        let resource = view
            .editor
            .document()
            .unwrap()
            .animation_catalog()
            .motions
            .iter()
            .find(|m| {
                m.playable()
                    && m.looping
                    && std::env::var("ZIPZI_BENCH_TIMELINE_VARIANT").map_or(true, |variant| {
                        m.group.as_deref() == Some(format!("Timeline{variant}").as_str())
                    })
            })
            .cloned()
            .context("native benchmark needs a looping motion")?;
        view.editor
            .select_motion(view.editor.active_id().unwrap(), resource.selection)?;
        view.editor.set_animation_preview_mode(
            view.editor.active_id().unwrap(),
            if drag.is_some() {
                zipzi_editor_core::AnimationPreviewMode::Editing
            } else {
                zipzi_editor_core::AnimationPreviewMode::Result
            },
        )?;
        view.redraw(cx);
        Ok((
            view.editor.document().unwrap().history_lengths(),
            view.editor.document().unwrap().modified(),
        ))
    })??;
    let executor = cx.background_executor().clone();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        executor.timer(Duration::from_millis(10)).await;
        if view.update_in(cx, |v, _, _| {
            v.editor.animation().unwrap().presented.is_some()
        })? {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "animation baseline not presented"
        );
    }
    if let Some(drag) = drag {
        #[cfg(feature = "test-support")]
        if drag.starts_with("timeline_") {
            return run_timeline_navigation(view, cx, config, &drag).await;
        }
        return run_drag(view, cx, config, &drag).await;
    }
    view.update_in(cx, |view, window, cx| {
        view.run_command(zipzi_editor_core::CommandId::PlayPause, window, cx)
    })?;
    executor.timer(Duration::from_millis(500)).await;
    let marker = frame_trace::next_id();
    frame_trace::emit(marker, "animation_workload", &[("model", 0), ("phase", 1)]);
    let deadline =
        Instant::now() + Duration::from_secs_f64(config.samples as f64 / config.hz as f64);
    while Instant::now() < deadline {
        executor.timer(Duration::from_millis(100)).await;
        view.update_in(cx, |view, window, _| -> anyhow::Result<()> {
            ensure!(
                window.is_visible() && window.is_window_active(),
                "animation benchmark window lost visibility or focus"
            );
            let a = view.editor.animation().unwrap();
            ensure!(
                a.playing && a.error.is_none(),
                "animation stopped: {:?}",
                a.error
            );
            Ok(())
        })??;
    }
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", 0), ("phase", 0)],
    );
    view.update_in(cx, |view, window, cx| -> anyhow::Result<()> {
        view.run_command(zipzi_editor_core::CommandId::PlayPause, window, cx);
        ensure!(
            (
                view.editor.document().unwrap().history_lengths(),
                view.editor.document().unwrap().modified()
            ) == baseline,
            "playback changed authoring state"
        );
        Ok(())
    })??;
    executor.timer(Duration::from_millis(200)).await;
    Ok(())
}

/// Visible-window counterpart of the existing headless Timeline navigation
/// probes. Uses the same production pointer handlers, without screenshot waits.
#[cfg(feature = "test-support")]
async fn run_timeline_navigation(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
    kind: &str,
) -> anyhow::Result<()> {
    use gpui_kit::test::TestWindowExt;
    use zipzi_editor_core::{TimelineDisplayMode, TimelineRowId};
    let key = view.update_in(cx, |v, _, cx| -> anyhow::Result<_> {
        let doc = v.editor.active_id().unwrap();
        let projection = v
            .editor
            .timeline_projection()
            .context("Timeline projection missing")?;
        let chosen = projection
            .channels
            .iter()
            .filter(|c| c.editable)
            .find_map(|channel| {
                let track = projection.tracks.get(channel.tracks.last()?)?;
                let key = track
                    .keys
                    .iter()
                    .find(|k| k.time > 0.)
                    .or_else(|| track.keys.first())?;
                Some((
                    channel.runtime_id.clone(),
                    channel.id.clone(),
                    track.id.clone(),
                    key.clone(),
                ))
            })
            .context("no editable Timeline key")?;
        v.editor.update_timeline_view(doc, |state| {
            state.scroll = 0.;
            state.mode = if kind == "timeline_key" {
                TimelineDisplayMode::Curves
            } else {
                TimelineDisplayMode::Overview
            };
            if kind == "timeline_key" {
                state.search = chosen.0.clone();
                state.channels = [
                    TimelineRowId::Track(chosen.2.clone()),
                    TimelineRowId::Channel(chosen.1.clone()),
                ]
                .into();
                state.time_range = ((chosen.3.time - 0.1).max(0.), chosen.3.time + 0.1);
                state.snap = false;
            }
        })?;
        if kind == "timeline_key" {
            v.editor.seek_timeline(doc, chosen.3.time as f32)?;
        }
        v.redraw(cx);
        Ok(chosen.3.reference)
    })??;
    let executor = cx.background_executor().clone();
    executor.timer(Duration::from_millis(600)).await;
    let (bounds, history, original) =
        view.update_in(cx, |v, window, cx| -> anyhow::Result<_> {
            let bounds = window
                .find(if kind == "timeline_seek" {
                    "animation-seek"
                } else {
                    "timeline-graph"
                })
                .bounds();
            let projection = v.editor.timeline_projection().unwrap();
            let original = v
                .editor
                .document()
                .unwrap()
                .motion_definition(&projection.clip_id)
                .context("missing motion definition")?;
            if kind == "timeline_key" {
                ensure!(
                    v.dock
                        .as_ref()
                        .unwrap()
                        .timeline
                        .read(cx)
                        .key_position(&key)
                        .is_some(),
                    "Timeline key is not visible"
                );
            }
            Ok((
                bounds,
                v.editor.document().unwrap().history_lengths(),
                original,
            ))
        })??;
    let origin = if kind == "timeline_key" {
        view.update_in(cx, |v, _, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .key_position(&key)
        })?
        .unwrap()
    } else {
        bounds.center()
    };
    let dragging = matches!(kind, "timeline_seek" | "timeline_key");
    if dragging {
        cx.update(|window, cx| {
            dispatch_pointer(
                window,
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: origin,
                    click_count: 1,
                    modifiers: Default::default(),
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            )
        })?;
    }
    let mode = match kind {
        "timeline_scroll" => 2,
        "timeline_zoom" => 3,
        "timeline_seek" => 4,
        _ => 5,
    };
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", mode), ("phase", 1)],
    );
    let mut changed = 0u64;
    let interval = Duration::from_secs_f64(1. / config.hz as f64);
    let mut deadline = Instant::now() + interval;
    for sample in 0..config.samples {
        executor
            .timer(deadline.saturating_duration_since(Instant::now()))
            .await;
        let input = frame_trace::InputScope::begin(120 + mode);
        frame_trace::emit(
            input.id(),
            "timeline_tick",
            &[("sample", sample as u64), ("kind", mode)],
        );
        let before = view.update_in(cx, |v, _, _| {
            (
                v.editor.timeline_view().cloned(),
                v.editor.animation().unwrap().time,
                v.editor.timeline_edit().map(|edit| edit.identity.revision),
            )
        })?;
        cx.update(|window, cx| -> anyhow::Result<()> {
            ensure!(
                window.is_visible() && window.is_window_active(),
                "Timeline benchmark lost focus"
            );
            if dragging {
                let position = if kind == "timeline_key" {
                    origin + point(px(0.), px(-8. - (sample as f32 * 0.17).sin() * 5.))
                } else {
                    point(
                        bounds.left()
                            + bounds.size.width * (0.5 + (sample as f32 * 0.13).sin() * 0.35),
                        origin.y,
                    )
                };
                dispatch_pointer(
                    window,
                    MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
            } else {
                dispatch_pointer(
                    window,
                    ScrollWheelEvent {
                        position: bounds.center(),
                        delta: ScrollDelta::Lines(point(
                            0.,
                            if sample % 40 < 20 { -2. } else { 2. },
                        )),
                        modifiers: if kind == "timeline_zoom" {
                            Modifiers::secondary_key()
                        } else {
                            Modifiers::default()
                        },
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
            }
            Ok(())
        })??;
        changed += view.update_in(cx, |v, _, _| -> anyhow::Result<u64> {
            ensure!(
                v.edit_error.is_none(),
                "Timeline input failed: {:?}",
                v.edit_error
            );
            let after = (
                v.editor.timeline_view().cloned(),
                v.editor.animation().unwrap().time,
                v.editor.timeline_edit().map(|edit| edit.identity.revision),
            );
            Ok(u64::from(before != after))
        })??;
        drop(input);
        deadline = navigation_deadline(deadline, interval, Instant::now());
    }
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", mode), ("phase", 0)],
    );
    if kind == "timeline_key" {
        cx.update(|window, cx| {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            )
        })?;
    }
    if dragging {
        cx.update(|window, cx| {
            dispatch_pointer(
                window,
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: origin,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            )
        })?;
    }
    view.update_in(cx, |v, _, _| -> anyhow::Result<()> {
        ensure!(changed > 0, "{kind} was a no-op");
        ensure!(
            v.editor.timeline_edit().is_none(),
            "Timeline candidate was not cancelled"
        );
        ensure!(
            v.editor.document().unwrap().history_lengths() == history,
            "Timeline probe changed history"
        );
        let projection = v.editor.timeline_projection().unwrap();
        ensure!(
            v.editor
                .document()
                .unwrap()
                .motion_definition(&projection.clip_id)
                .context("missing motion definition")?
                .as_ref()
                == original.as_ref(),
            "Timeline probe changed authored curves"
        );
        Ok(())
    })??;
    frame_trace::emit(
        frame_trace::next_id(),
        "timeline_checked",
        &[
            ("kind", mode),
            ("changed_samples", changed),
            ("samples", config.samples as u64),
        ],
    );
    executor.timer(Duration::from_millis(300)).await;
    Ok(())
}

#[cfg(any(feature = "test-support", test))]
fn navigation_deadline(previous: Instant, interval: Duration, now: Instant) -> Instant {
    let scheduled = previous + interval;
    if scheduled < now {
        now + interval
    } else {
        scheduled
    }
}

#[cfg(test)]
mod navigation_clock_tests {
    use super::navigation_deadline;
    use std::time::{Duration, Instant};
    #[test]
    fn late_input_yields_before_the_next_dispatch() {
        let previous = Instant::now();
        let interval = Duration::from_millis(8);
        let late = previous + Duration::from_millis(30);
        assert!(navigation_deadline(previous, interval, late) > late);
        let on_time = previous + Duration::from_millis(2);
        assert_eq!(
            navigation_deadline(previous, interval, on_time),
            previous + interval
        );
    }
}

async fn run_drag(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
    kind: &str,
) -> anyhow::Result<()> {
    let executor = cx.background_executor().clone();
    executor.timer(Duration::from_millis(500)).await;
    let (origin, number, baseline, initial_view) =
        view.update_in(cx, |v, _, cx| -> anyhow::Result<_> {
            let dock = v.dock.as_ref().unwrap();
            let number = if kind == "number" {
                Some(
                    dock.timeline
                        .read(cx)
                        .benchmark_number(
                            v.editor
                                .timeline_projection()
                                .context("no Timeline projection")?
                                .as_ref(),
                            cx,
                        )
                        .context("number control has no visible bounds")?,
                )
            } else {
                None
            };
            let origin = number
                .as_ref()
                .map(|(origin, _)| *origin)
                .or_else(|| dock.active_viewport().read(cx).benchmark_center())
                .context("drag control has no visible bounds")?;
            Ok((
                origin,
                number.map(|(_, number)| number),
                v.editor.document().unwrap().history_lengths(),
                v.editor.view(),
            ))
        })??;
    let button = if kind == "number" {
        MouseButton::Left
    } else {
        MouseButton::Middle
    };
    cx.update(|window, cx| {
        if let Some(number) = &number {
            number.update(cx, |number, _| {
                number.ignore_mouse_moves_when(Some(ignore_native_pointer_event))
            });
        }
        dispatch_pointer(
            window,
            MouseDownEvent {
                button,
                position: origin,
                click_count: 1,
                modifiers: Default::default(),
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        )
    })?;
    let mode = if kind == "number" { 1 } else { 0 };
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", mode), ("phase", 1)],
    );
    let interval = Duration::from_secs_f64(1. / config.hz as f64);
    let mut deadline = Instant::now();
    #[cfg(feature = "bench-instrumentation")]
    let detail = std::env::var("ZIPZI_BENCH_ANIMATION_DETAIL").is_ok_and(|v| v == "1");
    for sample in 0..config.samples {
        deadline += interval;
        executor
            .timer(deadline.saturating_duration_since(Instant::now()))
            .await;
        let _input = frame_trace::InputScope::begin(110 + mode);
        #[cfg(feature = "bench-instrumentation")]
        let capture = detail.then(zipzi_core::benchmark::Capture::start);
        cx.update(|window, cx| -> anyhow::Result<()> {
            ensure!(
                window.is_visible() && window.is_window_active(),
                "drag benchmark lost focus"
            );
            frame_trace::emit(
                frame_trace::next_id(),
                "drag_sample",
                &[("sample", sample as u64), ("kind", mode)],
            );
            dispatch_pointer(
                window,
                MouseMoveEvent {
                    position: origin + point(px((sample as f32 * 0.23).sin() * 60. + 8.), px(0.)),
                    pressed_button: Some(button),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            if let Some(number) = &number {
                ensure!(
                    number.read(cx).is_sliding(),
                    "number drag was interrupted at sample {sample}"
                );
            }
            Ok(())
        })??;
        #[cfg(feature = "bench-instrumentation")]
        if let Some(capture) = capture {
            for (stage, measurement) in capture.finish() {
                frame_trace::emit(
                    frame_trace::next_id(),
                    stage,
                    &[
                        ("sample", sample as u64),
                        ("calls", measurement.calls),
                        ("inclusive_ns", measurement.inclusive_ns),
                        ("self_ns", measurement.self_ns),
                    ],
                );
            }
        }
        // Do not inject a catch-up burst after a slow UI frame.
        deadline = deadline.max(Instant::now());
    }
    view.update_in(cx, |v, _, _| -> anyhow::Result<()> {
        if kind == "number" {
            ensure!(
                v.editor
                    .timeline_edit()
                    .is_some_and(|e| e.identity.revision > 1),
                "number drag was a no-op"
            );
        } else {
            ensure!(
                v.editor.view().pan != initial_view.pan,
                "canvas drag was a no-op"
            );
        }
        Ok(())
    })??;
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", mode), ("phase", 0)],
    );
    // Cancel before mouse-up so the benchmark never commits a number edit.
    if kind == "number" {
        cx.update(|window, cx| {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
        })?;
    }
    cx.update(|window, cx| {
        dispatch_pointer(
            window,
            MouseUpEvent {
                button,
                position: origin,
                click_count: 1,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        )
    })?;
    view.update_in(cx, |v, _, _| -> anyhow::Result<()> {
        ensure!(
            v.editor.timeline_edit().is_none(),
            "drag candidate was not cancelled"
        );
        ensure!(
            v.editor.document().unwrap().history_lengths() == baseline,
            "drag changed history"
        );
        Ok(())
    })??;
    if let Some(number) = &number {
        cx.update(|_, cx| number.update(cx, |number, _| number.ignore_mouse_moves_when(None)))?;
    }
    executor.timer(Duration::from_millis(300)).await;
    Ok(())
}
