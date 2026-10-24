use super::super::harness::{Harness, update};
use super::common::{drain, elapsed, pointer, pump, select, stats};
use anyhow::{Context as _, ensure};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ScrollDelta, ScrollWheelEvent, Window, point, px, test::TestWindowExt,
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use zipzi_editor_core::{AnimationPreviewMode, PanelKind, TimelineDisplayMode, WorkspaceKind};

pub(crate) fn canvas_navigation(h: &mut Harness, samples: usize) -> anyhow::Result<Value> {
    select(h, "TimelineNatural")?;
    let mut report = serde_json::Map::new();
    for (name, mode) in [
        ("editing", AnimationPreviewMode::Editing),
        ("result", AnimationPreviewMode::Result),
    ] {
        update(h, |v, _, cx| -> anyhow::Result<()> {
            let document = v.editor.active_id().unwrap();
            v.editor.set_animation_preview_mode(document, mode)?;
            v.editor.seek_timeline(document, 0.5)?;
            v.redraw(cx);
            Ok(())
        })??;
        drain(h)?;
        let before = h.capture()?;
        let before_canvas = h.canvas_image(&before)?;
        let (snapshot, history, pan) = h.view.read_with(&h.cx, |v, _| {
            (
                v.editor.animation().unwrap().presented.clone().unwrap(),
                v.editor.document().unwrap().history_lengths(),
                v.editor.view().pan,
            )
        });
        let origin =
            h.cx.update_window(h.window, |_, w, _| w.find("canvas-body").bounds().center())?;
        h.cx.update_window(h.window, |_, w, cx| {
            w.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Middle,
                    position: origin,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
        })?;
        pump(h)?;
        let timeline_renders = h.view.read_with(&h.cx, |v, cx| {
            v.dock.as_ref().unwrap().timeline.read(cx).renders()
        });
        let mut latency = Vec::new();
        let mut dispatch = Vec::new();
        let mut reused = 0;
        let position = |i: usize| origin + point(px(16. + (i % 16) as f32 * 2.), px(12.));
        for i in 0..samples {
            let start = Instant::now();
            h.cx.update_window(h.window, |_, w, cx| {
                w.dispatch_event(
                    MouseMoveEvent {
                        position: position(i),
                        pressed_button: Some(MouseButton::Middle),
                        modifiers: Modifiers::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
            })?;
            dispatch.push(elapsed(start));
            drain(h)?;
            latency.push(elapsed(start));
            reused += h.view.read_with(&h.cx, |v, _| {
                Arc::ptr_eq(
                    &snapshot,
                    v.editor.animation().unwrap().presented.as_ref().unwrap(),
                ) as usize
            });
        }
        ensure!(
            reused == samples,
            "viewport navigation re-evaluated the pose"
        );
        let after_renders = h.view.read_with(&h.cx, |v, cx| {
            v.dock.as_ref().unwrap().timeline.read(cx).renders()
        });
        ensure!(
            after_renders == timeline_renders,
            "canvas navigation rebuilt Timeline controls"
        );
        h.cx.update_window(h.window, |_, w, cx| {
            w.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Middle,
                    position: position(samples - 1),
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })?;
        let after = h.capture()?;
        ensure!(
            h.canvas_image(&after)? != before_canvas,
            "pan did not move displayed pixels"
        );
        h.view.read_with(&h.cx, |v, _| -> anyhow::Result<()> {
            ensure!(v.editor.view().pan != pan, "native middle drag did not pan");
            ensure!(
                v.editor.animation().unwrap().presented.as_deref() == Some(snapshot.as_ref()),
                "pan changed the animation pose"
            );
            ensure!(
                v.editor.document().unwrap().history_lengths() == history,
                "pan changed document history"
            );
            Ok(())
        })?;
        report.insert(name.into(), json!({"input_dispatch":stats(&dispatch), "input_to_canvas":stats(&latency), "reused_snapshots":reused, "timeline_renders":after_renders - timeline_renders}));
    }
    Ok(Value::Object(report))
}

pub(crate) fn canvas_dock_resize(
    h: &mut Harness,
    samples: usize,
    output: &Path,
) -> anyhow::Result<Value> {
    select(h, "TimelineNatural")?;
    update(h, |v, _, cx| {
        v.editor
            .update_timeline_view(v.editor.active_id().unwrap(), |view| {
                view.search.clear();
                view.scroll = 0.;
                view.mode = TimelineDisplayMode::Overview;
            })
            .unwrap();
        v.redraw(cx);
    })?;
    let mut report = serde_json::Map::new();
    for (name, workspace, properties) in [
        ("modeling", WorkspaceKind::Modeling, false),
        ("animation_properties", WorkspaceKind::Animation, true),
    ] {
        update(h, |v, w, cx| {
            v.switch_workspace(workspace, w, cx);
            if workspace == WorkspaceKind::Animation {
                v.dock
                    .as_ref()
                    .unwrap()
                    .show(PanelKind::Properties, None, w, cx);
            } else {
                v.editor
                    .execute(zipzi_editor_core::EditorCommand::SelectObjects(Vec::new()))
                    .unwrap();
            }
            v.redraw(cx);
        })?;
        if workspace == WorkspaceKind::Animation {
            drain(h)?;
        } else {
            h.capture()?;
        }
        pump(h)?;
        if properties {
            h.cx.update_window(h.window, |_, w, cx| {
                for _ in 0..3 {
                    w.scroll(
                        "object-properties",
                        ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
                        cx,
                    );
                    w.render_frame(cx);
                }
                w.click("property-group-$details", cx);
                w.render_frame(cx);
                w.scroll(
                    "object-properties",
                    ScrollDelta::Pixels(point(px(0.), px(100_000.))),
                    cx,
                );
            })?;
            pump(h)?;
        }
        let (origin, width) = h.cx.update_window(h.window, |_, w, _| {
            let b = w.find("canvas-body").bounds();
            let x = w
                .try_find("resize-handle-right")
                .map_or(b.right(), |handle| handle.bounds().center().x);
            (point(x, b.top() + px(60.)), b.size.width)
        })?;
        pointer(h, origin, 0)?;
        pump(h)?;
        let owner_notifications = std::rc::Rc::new(std::cell::Cell::new(0u64));
        let count = owner_notifications.clone();
        let _subscription =
            h.cx.update(|cx| cx.observe(&h.view, move |_, _| count.set(count.get() + 1)));
        let (timeline_renders, property_cache) = h.view.read_with(&h.cx, |v, cx| {
            let dock = v.dock.as_ref().unwrap();
            (
                dock.timeline.read(cx).renders(),
                dock.properties.read(cx).properties_cache_stats(),
            )
        });
        let mut dispatch = Vec::new();
        let mut draw = Vec::new();
        let mut complete = Vec::new();
        let pose = h.view.read_with(&h.cx, |v, _| {
            v.editor.animation().unwrap().presented.clone()
        });
        let position = |i: usize| origin + point(px(-12. - (i % 20) as f32 * 3.), px(0.));
        for i in 0..samples {
            let start = Instant::now();
            pointer(h, position(i), 1)?;
            dispatch.push(elapsed(start));
            pump(h)?;
            draw.push(elapsed(start));
            if workspace == WorkspaceKind::Animation {
                drain(h)?;
            } else {
                let deadline = Instant::now() + Duration::from_secs(10);
                while !h.view.read_with(&h.cx, |v, _| {
                    v.modeling_frame()
                        .as_ref()
                        .is_some_and(|f| v.editor.accepts_preview(f.canvas.version.request))
                }) {
                    pump(h)?;
                    ensure!(
                        Instant::now() < deadline,
                        "modeling resize never reached canvas"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            complete.push(elapsed(start));
        }
        let (after_renders, after_cache) = h.view.read_with(&h.cx, |v, cx| {
            let dock = v.dock.as_ref().unwrap();
            (
                dock.timeline.read(cx).renders(),
                dock.properties.read(cx).properties_cache_stats(),
            )
        });
        if properties {
            ensure!(
                property_cache == after_cache,
                "resize rebuilt property data"
            );
            ensure!(
                h.view.read_with(&h.cx, |v, _| Arc::ptr_eq(
                    pose.as_ref().unwrap(),
                    v.editor.animation().unwrap().presented.as_ref().unwrap()
                )),
                "resize re-evaluated the animation pose"
            );
        }
        let notifications = owner_notifications.get();
        pointer(h, position(samples - 1), 2)?;
        pump(h)?;
        let after = h.cx.update_window(h.window, |_, w, _| {
            w.find("canvas-body").bounds().size.width
        })?;
        ensure!(
            after != width,
            "native drag missed the right dock resize handle in {name}"
        );
        if properties {
            h.capture()?
                .save(output.join("dock-properties-resized.png"))?;
            let last = h
                .view
                .read_with(&h.cx, |v, cx| {
                    v.dock.as_ref().unwrap().properties.read(cx).last_row_id()
                })
                .context("Properties has no last row")?;
            let last_visible = |window: &Window| {
                let bounds = window.find("object-properties").bounds();
                window.try_find(last.clone()).is_some_and(|row| {
                    row.bounds().top() >= bounds.top() && row.bounds().bottom() <= bounds.bottom()
                })
            };
            // Motion details no longer repeat the entire timeline. Small
            // inspectors may already fit; larger lists still have to reach
            // their final row after a real dock resize and native scrolling.
            for _ in 0..4 {
                h.cx.update_window(h.window, |_, w, cx| {
                    w.dispatch_event(
                        ScrollWheelEvent {
                            position: w.find("object-properties").bounds().center(),
                            delta: ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
                            ..Default::default()
                        }
                        .to_platform_input(),
                        cx,
                    );
                })?;
                pump(h)?;
                if h.cx.update_window(h.window, |_, w, _| last_visible(w))? {
                    break;
                }
            }
            ensure!(
                h.cx.update_window(h.window, |_, w, _| last_visible(w))?,
                "last property row was not reachable by scrolling"
            );
            h.capture()?
                .save(output.join("dock-properties-bottom.png"))?;
        }
        report.insert(name.into(), json!({"input_dispatch":stats(&dispatch), "input_and_draw":stats(&draw), "input_to_canvas":stats(&complete), "owner_notifications":notifications, "timeline_renders":after_renders - timeline_renders, "property_cache_builds":after_cache.0 - property_cache.0, "property_rows":after_cache.1}));
    }
    Ok(Value::Object(report))
}
