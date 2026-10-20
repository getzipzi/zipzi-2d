use super::super::{document_fixture, setup_with_document};
use editor_core::{NewParameter, TimelineChannelId, TimelineCommand, TimelineDisplayMode};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px,
};

#[gpui_kit::test]
fn parameter_ranges_normalize_independently_and_axis_ticks_pan_with_curves(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) = document_fixture::animation_document().unwrap();
    let parameter = document.parameters()[0].id.clone();
    let mut draft = document.parameter_draft(&parameter).unwrap();
    draft.minimum = -30.;
    draft.maximum = 30.;
    document
        .update_parameter(&parameter, draft, document.parameter_model_revision())
        .unwrap();
    let second = document
        .create_parameter(NewParameter {
            name: "Wide range".into(),
            runtime_id: "ParamWide".into(),
            minimum: 0.,
            maximum: 100.,
            ..Default::default()
        })
        .unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            let projection = view.editor.timeline_projection().unwrap();
            let track = projection.tracks.values().next().unwrap();
            for (key, value) in [
                (track.keys[0].reference.clone(), -30.),
                (track.keys[1].reference.clone(), 30.),
            ] {
                let time = projection.key(&key).unwrap().time;
                view.editor
                    .execute_timeline(document, TimelineCommand::SetKey { key, time, value })
                    .unwrap();
            }
            let channel = TimelineChannelId::Parameter(second.clone());
            for (time, value) in [(0., 0.), (1., 100.)] {
                view.editor.seek_timeline(document, time).unwrap();
                view.editor
                    .execute_timeline(
                        document,
                        TimelineCommand::WriteValue {
                            channel: channel.clone(),
                            track_id: None,
                            selected_key: None,
                            value,
                        },
                    )
                    .unwrap();
            }
            let projection = view.editor.timeline_projection().unwrap();
            let rows: Vec<_> = projection
                .rows
                .iter()
                .filter(|row| row.track_id.is_some())
                .map(|row| row.id.clone())
                .collect();
            view.editor
                .update_timeline_view(document, |state| {
                    state.channels = rows.into_iter().collect();
                    state.mode = TimelineDisplayMode::Curves;
                    state.time_range = (0., 2.5);
                })
                .unwrap();
            view.redraw(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    let (keys, original, history) = cx
        .update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let dock = harness.view.read(cx).dock.as_ref().unwrap();
            let timeline = dock.timeline.read(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let keys: Vec<_> = projection
                .tracks
                .values()
                .map(|track| {
                    (
                        track.keys[0].reference.clone(),
                        track.keys[1].reference.clone(),
                    )
                })
                .collect();
            assert_eq!(keys.len(), 2);
            for index in [0, 1] {
                let (a, b) = if index == 0 {
                    (&keys[0].0, &keys[1].0)
                } else {
                    (&keys[0].1, &keys[1].1)
                };
                assert!(
                    (timeline.key_position(a).unwrap().y - timeline.key_position(b).unwrap().y)
                        .abs()
                        < px(0.001)
                );
            }
            let ticks = timeline.axis_ticks();
            assert!(
                ticks
                    .iter()
                    .find(|tick| tick.0 == 0.)
                    .unwrap()
                    .1
                    .starts_with("0%")
            );
            assert!(
                ticks
                    .iter()
                    .find(|tick| tick.0 == 1.)
                    .unwrap()
                    .1
                    .starts_with("100%")
            );
            let document = harness.view.read(cx).editor.document().unwrap();
            (
                keys,
                document
                    .motion_definition(&projection.clip_id)
                    .unwrap()
                    .clone(),
                document.history_lengths(),
            )
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        super::tool(window, "timeline-normalized", cx);
        window.render_frame(cx);
        let timeline = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx);
        assert!(
            (timeline.key_position(&keys[0].1).unwrap().y
                - timeline.key_position(&keys[1].1).unwrap().y)
                .abs()
                > px(20.)
        );
        super::tool(window, "timeline-normalized", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    // Focusing a selected row changes ruler units, never the peer curves/camera.
    for (channel, bound) in [
        (TimelineChannelId::Parameter(parameter.clone()), "30"),
        (TimelineChannelId::Parameter(second.clone()), "100"),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let row = projection
                .rows
                .iter()
                .find(|row| row.channel.as_ref() == Some(&channel) && row.track_id.is_some())
                .unwrap();
            let timeline = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx);
            let before = timeline.key_position(&keys[0].1).unwrap();
            let grid = timeline.axis_ticks();
            window.click(format!("timeline-row-{:?}", row.id), cx);
            window.render_frame(cx);
            let view = harness.view.read(cx);
            let state = view.editor.timeline_view().unwrap();
            assert_eq!(state.channels.len(), 2);
            assert_eq!(state.curve_focus.as_ref(), Some(&channel));
            let timeline = view.dock.as_ref().unwrap().timeline.read(cx);
            assert_eq!(timeline.key_position(&keys[0].1).unwrap(), before);
            let ticks = timeline.axis_ticks();
            assert_ne!(
                ticks.iter().map(|tick| &tick.1).collect::<Vec<_>>(),
                grid.iter().map(|tick| &tick.1).collect::<Vec<_>>()
            );
            assert!(ticks.iter().all(|tick| !tick.1.contains('%')));
            assert!(
                ticks
                    .iter()
                    .find(|tick| tick.0 == 1.)
                    .unwrap()
                    .1
                    .starts_with(bound)
            );
            assert!(ticks.iter().filter(|tick| !tick.1.is_empty()).count() >= 5);
        })
        .unwrap();
        cx.run_until_parked();
    }
    // Curve and key clicks focus the corresponding ruler without hiding peers.
    cx.update_window(harness.window, |_, window, cx| {
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let first = keys
            .iter()
            .find(|keys| {
                projection.tracks[&keys.0.track_id].channel
                    == TimelineChannelId::Parameter(parameter.clone())
            })
            .unwrap();
        let second_keys = keys
            .iter()
            .find(|keys| {
                projection.tracks[&keys.0.track_id].channel
                    == TimelineChannelId::Parameter(second.clone())
            })
            .unwrap();
        let timeline = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx);
        let a = timeline.key_position(&first.0).unwrap();
        let b = timeline.key_position(&first.1).unwrap();
        let on_curve = a + (b - a) * 0.4;
        let on_key = timeline.key_position(&second_keys.1).unwrap();
        for (position, channel) in [
            (on_curve, TimelineChannelId::Parameter(parameter.clone())),
            (on_key, TimelineChannelId::Parameter(second.clone())),
        ] {
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
            window.render_frame(cx);
            let view = harness.view.read(cx);
            let state = view.editor.timeline_view().unwrap();
            assert_eq!(state.curve_focus.as_ref(), Some(&channel));
            assert_eq!(state.channels.len(), 2);
            assert_eq!(
                view.dock
                    .as_ref()
                    .unwrap()
                    .timeline
                    .read(cx)
                    .key_position(&first.1)
                    .unwrap(),
                b
            );
        }
    })
    .unwrap();
    cx.run_until_parked();
    // Clearing focus restores a common percentage ruler, including after a pan.
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(document, |state| state.curve_focus = None)
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    for button in [MouseButton::Left, MouseButton::Middle] {
        if button == MouseButton::Left {
            cx.update_window(harness.window, |_, window, cx| {
                super::tool(window, "timeline-pan", cx)
            })
            .unwrap();
        }
        for dy in [8., -8.] {
            cx.update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                let timeline = harness
                    .view
                    .read(cx)
                    .dock
                    .as_ref()
                    .unwrap()
                    .timeline
                    .read(cx);
                let before = timeline.key_position(&keys[0].1).unwrap();
                let tick = timeline
                    .axis_ticks()
                    .into_iter()
                    .find(|tick| tick.0 > 0. && tick.0 < 1. && !tick.1.is_empty())
                    .unwrap();
                let origin = window.find("timeline-graph").bounds().center();
                let end = origin + point(px(0.), px(dy));
                window.dispatch_event(
                    MouseDownEvent {
                        button,
                        position: origin,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    MouseMoveEvent {
                        position: end,
                        pressed_button: Some(button),
                        modifiers: Modifiers::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    MouseUpEvent {
                        button,
                        position: end,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
                let timeline = harness
                    .view
                    .read(cx)
                    .dock
                    .as_ref()
                    .unwrap()
                    .timeline
                    .read(cx);
                let after = timeline.key_position(&keys[0].1).unwrap();
                let next = timeline
                    .axis_ticks()
                    .into_iter()
                    .find(|candidate| candidate.0 == tick.0)
                    .unwrap();
                assert!((after.y - before.y - px(dy)).abs() < px(0.01));
                assert!((next.2 - tick.2 - dy).abs() < 0.01);
                assert_eq!(
                    tick.1, next.1,
                    "panning must move a tick, rather than change its value"
                );
            })
            .unwrap();
        }
        if button == MouseButton::Left {
            cx.update_window(harness.window, |_, window, cx| {
                super::tool(window, "timeline-pan", cx)
            })
            .unwrap();
        }
    }
    cx.update_window(harness.window, |_, window, cx| {
        let timeline = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx);
        let before = timeline.key_position(&keys[0].1).unwrap();
        let position = window.find("timeline-graph").bounds().center();
        window.dispatch_event(
            ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(8.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        let view = harness.view.read(cx);
        let after = view
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&keys[0].1)
            .unwrap();
        assert!((after.y - before.y - px(8.)).abs() < px(0.01));
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), history);
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .motion_definition(&original.id)
                .unwrap(),
            &original
        );
    })
    .unwrap();
    // A single channel automatically has an actual-value ruler, with no focus.
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            let projection = view.editor.timeline_projection().unwrap();
            let row = projection
                .rows
                .iter()
                .find(|row| {
                    row.channel.as_ref() == Some(&TimelineChannelId::Parameter(parameter.clone()))
                        && row.track_id.is_some()
                })
                .unwrap();
            view.editor
                .update_timeline_view(document, |state| {
                    state.channels = [row.id.clone()].into_iter().collect();
                    state.curve_focus = None;
                })
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        let ticks = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .axis_ticks();
        assert!(ticks.iter().all(|tick| !tick.1.contains('%')));
        assert!(ticks.iter().any(|tick| tick.1.starts_with("-30")));
        assert!(ticks.iter().any(|tick| tick.1.starts_with("30")));
        assert!(ticks.iter().filter(|tick| !tick.1.is_empty()).count() >= 5);
    })
    .unwrap();
}
