use super::slide_number_input::{press, release, slide};
use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Focusable, InputEvent as _, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px,
};
use std::time::Duration;
use zipzi_editor_core::{AnimationPreviewMode, CommandId, TimelineCommand, TimelineFilter};

fn animation(cx: &mut TestAppContext) -> Harness {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click("animation-preview-mode", cx);
        window.click("animation-edit-preview", cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness
}

#[gpui_kit::test]
fn shift_wheel_and_hand_and_middle_drag_pan_without_asset_edits(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(document, |state| state.time_range = (0.5, 1.))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        let position = window.find("timeline-graph").bounds().center();
        for delta in [point(px(24.), px(0.)), point(px(0.), px(24.))] {
            let before = harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .time_range;
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(delta),
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            let after = harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .time_range;
            assert!(after.0 < before.0);
            assert!((after.1 - after.0 - 0.5).abs() < 1e-9);
        }
        super::timeline::toggle_pan_tool(&harness, window, cx);
        window.render_frame(cx);
        let before = harness
            .view
            .read(cx)
            .editor
            .timeline_view()
            .unwrap()
            .time_range
            .0;
        let origin = window.find("timeline-graph").bounds().center();
        press(window, origin, cx);
        slide(window, origin - point(px(40.), px(0.)), false, cx);
        release(window, origin - point(px(40.), px(0.)), cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .time_range
                .0
                > before
        );
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        super::timeline::toggle_pan_tool(&harness, window, cx);
        window.render_frame(cx);
        let before = harness
            .view
            .read(cx)
            .editor
            .timeline_view()
            .unwrap()
            .time_range
            .0;
        window.dispatch_event(
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
        window.dispatch_event(
            MouseMoveEvent {
                position: origin - point(px(40.), px(0.)),
                pressed_button: Some(MouseButton::Middle),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Middle,
                position: origin - point(px(40.), px(0.)),
                modifiers: Modifiers::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .time_range
                .0
                > before
        );
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn transport_buttons_and_shortcuts_share_endpoints_and_key_navigation(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let focus = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .focus_handle(cx);
        window.focus(&focus, cx);
        crate::keybindings::reset(cx).unwrap();
        window.hover("timeline-graph", cx);
        window.press("shift-right", cx);
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            2.
        );
        window.press("shift-left", cx);
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            0.
        );
        window.press("cmd-right", cx);
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            2.
        );
        window.press("cmd-left", cx);
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            0.
        );
        window.click("timeline-end", cx);
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            2.
        );
        window.click("timeline-start", cx);
        crate::headless_tests::timeline::tool(window, "timeline-next-frame", cx);
        assert!(
            (harness.view.read(cx).editor.animation().unwrap().edit_time - 1. / 60.).abs() < 1e-6
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        crate::headless_tests::timeline::tool(window, "timeline-previous-frame", cx);
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            0.
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
        assert!(window.try_find("timeline-motion").is_none());
        assert!(window.try_find("timeline-new-motion").is_none());
        assert!(window.find("animation-new-motion").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn edge_pan_keeps_key_candidate_alive_and_commits_one_undo_step(cx: &mut TestAppContext) {
    let harness = animation(cx);
    let (key, pointer, before) = cx
        .update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                let document = view.editor.active_id().unwrap();
                view.editor
                    .update_timeline_view(document, |state| {
                        state.time_range = (0., 0.5);
                        state.snap = false;
                    })
                    .unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
            let p = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key = p.tracks.values().next().unwrap().keys[0].reference.clone();
            let origin = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .key_position(&key)
                .unwrap()
                + point(px(2.), px(0.));
            let bounds = window.find("timeline-graph").bounds();
            let pointer = point(bounds.right() - px(2.), origin.y);
            press(window, origin, cx);
            slide(window, pointer, false, cx);
            let edit = harness.view.read(cx).editor.timeline_edit().unwrap();
            let before = (
                edit.identity.edit,
                edit.candidate.clip().tracks[0].initial.time,
            );
            window.dispatch_event(
                ScrollWheelEvent {
                    position: pointer,
                    delta: ScrollDelta::Pixels(point(px(-24.), px(0.))),
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            (key, pointer, before)
        })
        .unwrap();
    cx.run_until_parked();
    for _ in 0..8 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    harness.view.read_with(cx, |view, _| {
        let edit = view
            .editor
            .timeline_edit()
            .expect("pan must preserve the candidate");
        assert_eq!(edit.identity.edit, before.0);
        assert!(edit.candidate.clip().tracks[0].initial.time > before.1);
        assert!(view.editor.timeline_view().unwrap().time_range.0 > 0.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        release(window, pointer, cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (1, 0)
        );
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .time,
            0.
        );
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_millis(64));
    cx.run_until_parked();
    harness
        .view
        .read_with(cx, |view, _| assert!(view.editor.timeline_edit().is_none()));
}

#[gpui_kit::test]
fn escape_stops_edge_pan_and_restores_asset_after_late_pointer_input(cx: &mut TestAppContext) {
    let harness = animation(cx);
    let pointer = cx
        .update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                let document = view.editor.active_id().unwrap();
                view.editor
                    .update_timeline_view(document, |view| {
                        view.time_range = (0., 0.5);
                        view.snap = false;
                    })
                    .unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
            let p = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key = &p.tracks.values().next().unwrap().keys[0].reference;
            let origin = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .key_position(key)
                .unwrap()
                + point(px(2.), px(0.));
            let pointer = point(
                window.find("timeline-graph").bounds().right() - px(2.),
                origin.y,
            );
            press(window, origin, cx);
            slide(window, pointer, false, cx);
            pointer
        })
        .unwrap();
    cx.run_until_parked();
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    cx.update_window(harness.window, |_, window, cx| {
        assert!(harness.view.read(cx).editor.timeline_edit().is_some());
        window.press("escape", cx);
        slide(window, pointer, false, cx);
        release(window, pointer, cx);
    })
    .unwrap();
    let range = harness.view.read_with(cx, |view, _| {
        view.editor.timeline_view().unwrap().time_range
    });
    cx.executor().advance_clock(Duration::from_millis(160));
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.timeline_view().unwrap().time_range, range);
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(
            view.editor
                .timeline_projection()
                .unwrap()
                .tracks
                .values()
                .next()
                .unwrap()
                .keys[0]
                .time,
            0.
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn marquee_edge_pan_keeps_offscreen_keys_and_scrolls_rows(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(24).unwrap();
    let harness = setup_with_document(cx, document, assets);
    let pointer = cx
        .update_window(harness.window, |_, window, cx| {
            window.click("workspace-animation", cx);
            harness.view.update(cx, |view, cx| {
                let d = view.editor.active_id().unwrap();
                view.editor.create_timeline_motion(d).unwrap();
                view.editor.seek_timeline(d, 0.25).unwrap();
                let parameters = view.editor.document().unwrap().parameters();
                for parameter in parameters {
                    view.editor
                        .execute_timeline(
                            d,
                            TimelineCommand::WriteValue {
                                channel: zipzi_editor_core::TimelineChannelId::Parameter(
                                    parameter.id,
                                ),
                                track_id: None,
                                selected_key: None,
                                value: 0.,
                            },
                        )
                        .unwrap();
                }
                view.editor
                    .update_timeline_view(d, |state| {
                        state.time_range = (0., 0.5);
                        state.filter = TimelineFilter::All;
                        state.keys.clear();
                        state.channels.clear();
                        state.reveal_row = None;
                        state.scroll = 0.;
                    })
                    .unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
            let bounds = window.find("timeline-graph").bounds();
            let origin = bounds.origin + point(px(8.), px(2.));
            let pointer = point(bounds.right() - px(2.), bounds.bottom() - px(2.));
            press(window, origin, cx);
            slide(window, pointer, false, cx);
            pointer
        })
        .unwrap();
    let (initial, history) = harness.view.read_with(cx, |view, _| {
        (
            view.editor.timeline_view().unwrap().keys.clone(),
            view.editor.document().unwrap().history_lengths(),
        )
    });
    assert!(!initial.is_empty());
    cx.run_until_parked();
    for _ in 0..60 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    harness.view.read_with(cx, |view, _| {
        let state = view.editor.timeline_view().unwrap();
        assert!(state.time_range.0 > 0.25, "{:?}", state.time_range);
        assert!(state.scroll > 0.);
        assert!(state.keys.is_superset(&initial));
        assert!(state.keys.len() > initial.len());
        assert_eq!(view.editor.document().unwrap().history_lengths(), history);
    });
    cx.update_window(harness.window, |_, window, cx| release(window, pointer, cx))
        .unwrap();
    let range = harness.view.read_with(cx, |view, _| {
        view.editor.timeline_view().unwrap().time_range
    });
    cx.executor().advance_clock(Duration::from_millis(64));
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.timeline_view().unwrap().time_range, range)
    });
}

#[gpui_kit::test]
fn search_clear_restores_channels_and_expression_selection_keeps_preview_mode(
    cx: &mut TestAppContext,
) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-search", cx);
        window.input("nothing matches", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        let rows = harness
            .view
            .read(cx)
            .editor
            .timeline_projection()
            .unwrap()
            .rows
            .len();
        assert!(harness.view.read(cx).editor.timeline_rows().unwrap().len() < rows);
        window.within("timeline-search").click("clean", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        let rows = harness
            .view
            .read(cx)
            .editor
            .timeline_projection()
            .unwrap()
            .visible_rows(&Default::default())
            .len();
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .search
                .is_empty()
        );
        assert_eq!(
            harness.view.read(cx).editor.timeline_rows().unwrap().len(),
            rows
        );
        window.click(("animation-expression", 0usize), cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .preview_mode,
            AnimationPreviewMode::Editing
        );
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .expression
                .is_some()
        );
        window.click("animation-preview-mode", cx);
        window.click("animation-result-preview", cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .preview_mode,
            AnimationPreviewMode::Result
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}
