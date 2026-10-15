//! Native window playback: the application's display callbacks, GPU workers and compositor.
use super::*;

pub(super) async fn run(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
) -> anyhow::Result<()> {
    let drag = std::env::var("KASANE_BENCH_ANIMATION_DRAG").ok();
    ensure!(
        drag.as_deref()
            .is_none_or(|v| matches!(v, "canvas" | "number")),
        "invalid animation drag workload"
    );
    let baseline = view.update_in(cx, |view, window, cx| -> anyhow::Result<_> {
        view.run_command(editor_core::CommandId::AnimationWorkspace, window, cx);
        let resource = view
            .editor
            .document()
            .unwrap()
            .animation_catalog()
            .motions
            .iter()
            .find(|m| m.playable() && m.looping)
            .cloned()
            .context("native benchmark needs a looping motion")?;
        view.editor
            .select_motion(view.editor.active_id().unwrap(), resource.selection)?;
        view.editor.set_animation_preview_mode(
            view.editor.active_id().unwrap(),
            if drag.is_some() {
                editor_core::AnimationPreviewMode::Editing
            } else {
                editor_core::AnimationPreviewMode::Result
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
        return run_drag(view, cx, config, &drag).await;
    }
    view.update_in(cx, |view, window, cx| {
        view.run_command(editor_core::CommandId::PlayPause, window, cx)
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
        view.run_command(editor_core::CommandId::PlayPause, window, cx);
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
                .or_else(|| dock.active_canvas().read(cx).benchmark_center())
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
                number.ignore_mouse_moves_when(Some(ignore_native_mouse_move))
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
    for sample in 0..config.samples {
        deadline += interval;
        executor
            .timer(deadline.saturating_duration_since(Instant::now()))
            .await;
        let _input = frame_trace::InputScope::begin(110 + mode);
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
