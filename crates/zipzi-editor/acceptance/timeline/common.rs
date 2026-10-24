use super::super::harness::{Harness, update};
use anyhow::{Context as _, ensure};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point,
};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use zipzi_core::document::MotionClip;
use zipzi_editor_core::TimelineCommand;

pub(crate) fn pump(h: &mut Harness) -> anyhow::Result<()> {
    h.cx.run_until_parked();
    h.cx.update_window(h.window, |_, window, cx| {
        window.draw(cx).clear(cx);
    })?;
    Ok(())
}

pub(crate) fn current(h: &Harness) -> bool {
    h.view.read_with(&h.cx, |view, _| {
        let a = view.editor.animation().unwrap();
        assert!(a.error.is_none(), "{:?}", a.error);
        assert!(view.render_error().is_none(), "{:?}", view.render_error());
        view.animation_frame().is_some_and(|frame| {
            let id = frame.state.identity;
            view.editor
                .can_display_animation_viewport(frame.state.viewport)
                && view.editor.animation_viewport_revision()
                    == Some(frame.state.viewport.viewport_revision)
                && id.time == a.time
                && id.candidate == a.candidate
                && id.model_generation == a.model_generation
                && id.preview_generation == a.preview_generation
                && id.preview_mode == a.preview_mode
                && a.presented
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(p, &frame.state.snapshot))
        })
    })
}

pub(crate) fn drain(h: &mut Harness) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        pump(h)?;
        if current(h) {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "current Timeline revision never reached the Metal canvas"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(crate) fn stats(values: &[f64]) -> Value {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let percentile = |p: f64| v[((v.len() - 1) as f64 * p).ceil() as usize];
    json!({"samples":v.len(),"p50_ms":percentile(0.5),"p95_ms":percentile(0.95),"max_ms":v.last()})
}

pub(crate) fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}

pub(crate) fn select(h: &mut Harness, group: &str) -> anyhow::Result<()> {
    update(h, |view, _, cx| -> anyhow::Result<()> {
        let selection = view
            .editor
            .document()
            .unwrap()
            .animation_catalog()
            .motions
            .iter()
            .find(|m| m.group.as_deref() == Some(group))
            .context("missing variant")?
            .selection
            .clone();
        view.editor
            .select_motion(view.editor.active_id().unwrap(), selection)?;
        view.redraw(cx);
        Ok(())
    })??;
    drain(h)
}

pub(crate) fn clip(h: &Harness) -> MotionClip {
    h.view.read_with(&h.cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .motion_definition(&view.editor.timeline_projection().unwrap().clip_id)
            .unwrap()
            .as_ref()
            .clone()
    })
}

pub(crate) fn edit(h: &mut Harness, c: TimelineCommand) -> anyhow::Result<()> {
    update(h, |view, _, cx| -> anyhow::Result<()> {
        view.editor
            .execute_timeline(view.editor.active_id().unwrap(), c)?;
        view.redraw(cx);
        Ok(())
    })??;
    drain(h)
}

pub(crate) fn pointer(h: &mut Harness, position: Point<Pixels>, kind: u8) -> anyhow::Result<()> {
    h.cx.update_window(h.window, |_, window, cx| match kind {
        0 => {
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
        }
        1 => {
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
        }
        _ => {
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        }
    })?;
    Ok(())
}
