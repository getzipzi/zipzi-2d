use super::slide_number_input::{press, release, slide};
use super::{document_fixture, setup_with_document};
use editor_core::{CommandId, TimelineDisplayMode, TimelineKeyRef, TimelineRowId};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point, px};

#[gpui_kit::test]
fn timeline_panel_close_reopen_and_collapse_cancel_live_edits(cx: &mut TestAppContext) {
    use editor_core::PanelKind;
    use gpui_kit::component::dock::{DockPlacement, PanelId};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let (area, panel) = cx
        .update_window(harness.window, |_, window, cx| {
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let dock = harness.view.read(cx).dock.as_ref().unwrap();
            let panel = PanelId::from(dock.timeline.entity_id());
            assert!(
                dock.area
                    .read(cx)
                    .layout(DockPlacement::Bottom)
                    .unwrap()
                    .find_panel_node(panel)
                    .is_some()
            );
            (dock.area.clone(), panel)
        })
        .unwrap();
    for close in [true, false] {
        let origin = cx
            .update_window(harness.window, |_, window, cx| {
                let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
                let key = projection.tracks.values().next().unwrap().keys[0]
                    .reference
                    .clone();
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
                press(window, origin, cx);
                slide(window, origin + point(px(40.), px(0.)), false, cx);
                assert!(harness.view.read(cx).editor.timeline_edit().is_some());
                if close {
                    harness.view.update(cx, |view, cx| {
                        view.run_command(CommandId::AnimationTimeline, window, cx)
                    });
                } else {
                    area.update(cx, |area, cx| {
                        area.toggle_dock(DockPlacement::Bottom, window, cx)
                    });
                }
                window.render_frame(cx);
                origin
            })
            .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |view, cx| {
            assert!(view.editor.timeline_edit().is_none());
            assert!(
                !view
                    .dock
                    .as_ref()
                    .unwrap()
                    .timeline
                    .read(cx)
                    .pointer_active()
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        cx.update_window(harness.window, |_, window, cx| {
            // The old pointer release cannot publish the canceled candidate.
            release(window, origin + point(px(40.), px(0.)), cx);
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationTimeline, window, cx)
            });
            window.render_frame(cx);
            let dock = harness.view.read(cx).dock.as_ref().unwrap();
            assert!(dock.contains(PanelKind::AnimationTimeline, cx));
            assert_eq!(PanelId::from(dock.timeline.entity_id()), panel);
            assert!(area.read(cx).is_dock_open(DockPlacement::Bottom));
            assert!(window.find("animation-timeline").visible());
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
}

#[gpui_kit::test]
fn timeline_panel_can_move_share_tabs_and_restore_its_workspace_layout(cx: &mut TestAppContext) {
    use editor_core::WorkspaceKind;
    use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let (area, panel, info) = cx
        .update_window(harness.window, |_, window, cx| {
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.simulate_next_frame(cx);
            let dock = harness.view.read(cx).dock.as_ref().unwrap();
            let panel = PanelId::from(dock.timeline.entity_id());
            let info = PanelId::from(dock.animation_info.entity_id());
            let area = dock.area.clone();
            let node = area
                .read(cx)
                .layout(DockPlacement::Center)
                .unwrap()
                .find_panel_node(info)
                .unwrap();
            area.update(cx, |area, cx| {
                area.move_panel(
                    panel,
                    InsertTarget::Tabs {
                        node,
                        ix: None,
                        activate: true,
                    },
                    window,
                    cx,
                );
            });
            window.render_frame(cx);
            assert!(window.find("animation-timeline").visible());
            crate::docking::validate_layout(&area.read(cx).dump(cx), WorkspaceKind::Animation)
                .unwrap();
            (area, panel, info)
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| area.select_panel(info, window, cx));
        window.render_frame(cx);
        assert!(window.try_find("animation-timeline").is_none());
        area.update(cx, |area, cx| area.select_panel(panel, window, cx));
        window.render_frame(cx);
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        assert!(window.try_find("animation-timeline").is_none());
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(panel), tree.find_panel_node(info));
        assert!(tree.find_panel_node(panel).is_some());
        assert!(window.find("animation-timeline").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn overview_drag_is_one_transaction_and_escape_restores_the_asset(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let key = projection.tracks.values().next().unwrap().keys[0]
            .reference
            .clone();
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
        let graph = window.find("timeline-graph").bounds();
        press(window, origin, cx);
        slide(
            window,
            origin + point(graph.size.width / 4., px(0.)),
            false,
            cx,
        );
        let view = harness.view.read(cx);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert_eq!(
            view.editor.timeline_edit().unwrap().candidate.clip().tracks[0]
                .initial
                .time,
            0.5
        );
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .motion_definition(&key.clip_id)
                .unwrap()
                .tracks[0]
                .initial
                .time,
            0.
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
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
        // A late move after cancellation must not restart the gesture.
        slide(
            window,
            origin + point(graph.size.width / 3., px(0.)),
            false,
            cx,
        );
        release(window, origin, cx);
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
        press(window, origin, cx);
        slide(
            window,
            origin + point(graph.size.width / 4., px(0.)),
            false,
            cx,
        );
        release(window, origin + point(graph.size.width / 4., px(0.)), cx);
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
}

#[gpui_kit::test]
fn empty_motion_inline_value_creates_one_key_at_the_head_and_can_cancel(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click("animation-new-motion", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            assert!(projection.tracks.is_empty());
            let row = projection
                .rows
                .iter()
                .find(|row| {
                    matches!(
                        row.channel,
                        Some(editor_core::TimelineChannelId::Parameter(_))
                    )
                })
                .unwrap();
            let field = format!("timeline-number-RowValue({:?})", row.id);
            harness.view.update(cx, |view, cx| {
                let id = view.editor.active_id().unwrap();
                view.editor.seek_timeline(id, 0.5).unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
            let origin = window.find(field).bounds().center();
            press(window, origin, cx);
            slide(window, origin + point(px(80.), px(0.)), false, cx);
            origin
        })
        .unwrap();
    harness.view.read_with(cx, |view, _| {
        let candidate = view.editor.timeline_edit().unwrap().candidate.clip();
        assert_eq!(candidate.tracks.len(), 1);
        assert_eq!(candidate.tracks[0].initial.time, 0.5);
        assert!(candidate.tracks[0].segments.is_empty());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks
                .is_empty()
        );
        slide(window, origin + point(px(120.), px(0.)), false, cx);
        release(window, origin, cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        press(window, origin, cx);
        slide(window, origin + point(px(80.), px(0.)), false, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        release(window, origin, cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        assert_eq!(view.editor.timeline_projection().unwrap().tracks.len(), 1);
    });
}

#[gpui_kit::test]
fn curve_view_draws_handles_and_restricted_handle_drag_only_changes_value(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let track = projection.tracks.values().next().unwrap();
        let first = track.keys[0].reference.clone();
        let end = track.keys[1].reference.clone();
        let origin = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&first)
            .unwrap()
            + point(px(2.), px(0.));
        press(window, origin, cx);
        release(window, origin, cx);
        window.render_frame(cx);
        window.click("timeline-interpolation-bezier", cx);
        window.render_frame(cx);
        window.click("timeline-view", cx);
        window.render_frame(cx);
        let handles = harness
            .view
            .read(cx)
            .editor
            .timeline_projection()
            .unwrap()
            .display_handles(&end)
            .unwrap();
        assert!(!handles.horizontal_editable);
        let origin = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .handle_position(&end)
            .unwrap();
        press(window, origin, cx);
        slide(window, origin + point(px(35.), px(-15.)), false, cx);
        release(window, origin, cx);
        window.render_frame(cx);
        let next = harness
            .view
            .read(cx)
            .editor
            .timeline_projection()
            .unwrap()
            .display_handles(&end)
            .unwrap();
        assert_eq!(handles.control1.time, next.control1.time);
        assert_ne!(handles.control1.value, next.control1.value);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (2, 0)
        );
        let selection = harness
            .view
            .read(cx)
            .editor
            .timeline_view()
            .unwrap()
            .keys
            .clone();
        window.click("timeline-view", cx);
        window.render_frame(cx);
        assert_eq!(
            harness.view.read(cx).editor.timeline_view().unwrap().keys,
            selection
        );
        assert_eq!(
            harness.view.read(cx).editor.timeline_view().unwrap().mode,
            TimelineDisplayMode::Overview
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn keyboard_copy_paste_delete_is_scoped_and_number_text_has_priority(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let (key, pasted) = cx
        .update_window(harness.window, |_, window, cx| {
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key = projection.tracks.values().next().unwrap().keys[0]
                .reference
                .clone();
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
            press(window, origin, cx);
            release(window, origin, cx);
            window.render_frame(cx);
            crate::keybindings::reset(cx).unwrap();
            window.press("cmd-c", cx);
            for _ in 0..30 {
                window.press("right", cx);
            }
            window.press("cmd-v", cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let track = &projection.tracks[&key.track_id];
            assert_eq!(track.keys.len(), 3);
            assert!((track.keys[1].time - 0.5).abs() < 1e-5);
            let pasted: TimelineKeyRef = track.keys[1].reference.clone();
            (key, pasted)
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let field = format!("timeline-number-KeyValue({pasted:?})");
        window.click(field.clone(), cx);
        window.press("cmd-a", cx);
        window.input("0.25", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(harness.view.read(cx).editor.timeline_edit().is_some());
        window.press("backspace", cx); // text, not a key deletion
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks[&key.track_id]
                .keys
                .len(),
            3
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        window.press("backspace", cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks[&key.track_id]
                .keys
                .len(),
            2
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (2, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn event_editing_resize_and_view_controls_preserve_history_boundaries(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let before = window.find("animation-timeline").bounds().size.height;
        let area = harness.view.read(cx).dock.as_ref().unwrap().area.clone();
        area.update(cx, |area, cx| {
            use gpui_kit::component::dock::DockPlacement;
            let size = area.dock_size(DockPlacement::Bottom).unwrap();
            area.set_dock_size(DockPlacement::Bottom, size + px(45.), window, cx);
        });
        window.render_frame(cx);
        assert!(window.find("animation-timeline").bounds().size.height > before);
        window.click("timeline-seconds", cx);
        window.click("timeline-fit", cx);
        window.click("timeline-snap", cx);
        window.render_frame(cx);
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
        window.click("timeline-add-event", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.events.len(), 1);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .channels
                .first(),
            Some(&TimelineRowId::Events)
        );
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
        window.click("timeline-delete", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, _window, cx| {
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .events
                .is_empty()
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (2, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn seek_reuses_static_curve_geometry_and_marquee_selects_real_keys(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let key = projection.tracks.values().next().unwrap().keys[0]
            .reference
            .clone();
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
        press(window, origin, cx);
        release(window, origin, cx);
        window.render_frame(cx);
        window.click("timeline-view", cx);
        window.render_frame(cx);
        let builds = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .scene_builds();
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            view.editor.seek_timeline(d, 0.5).unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .scene_builds(),
            builds
        );
        window.click("timeline-view", cx);
        window.render_frame(cx);
        let origin = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&key)
            .unwrap();
        let start = origin + point(px(35.), px(-10.));
        let end = origin + point(px(-1.), px(10.));
        press(window, start, cx);
        slide(window, end, false, cx);
        release(window, end, cx);
        window.render_frame(cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .keys
                .contains(&key)
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

#[gpui_kit::test]
fn event_text_enter_commits_once_and_same_time_events_remain_distinct(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        window.click("timeline-add-event", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-event-value", cx);
        window.input("hello", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.timeline_projection().unwrap().events[0].value,
            "hello"
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            cx,
        );
        window.click("timeline-add-event", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.events.len(), 2);
        assert_eq!(projection.events[0].time, projection.events[1].time);
        assert_ne!(projection.events[0].id, projection.events[1].id);
        window.click("timeline-event-list", cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (3, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn explicit_time_scale_previews_and_enter_commits_all_times_together(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        window.click("timeline-properties", cx);
        window.render_frame(cx);
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-number-Scale", cx);
        window.press("cmd-a", cx);
        window.input("2", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let clip = view.editor.timeline_edit().unwrap().candidate.clip();
        assert_eq!(clip.duration, 4.);
        assert_eq!(clip.tracks[0].segments[0].end().time, 4.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert_eq!(view.editor.timeline_projection().unwrap().duration, 4.);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(10000.))),
            cx,
        );
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-90.))),
            cx,
        );
        window.click("timeline-fade-default-FadeIn", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .motion_definition(&projection.clip_id)
                .unwrap()
                .fade_in,
            None
        );
        window.click("timeline-fade-custom-FadeIn", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.duration, 4.);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .motion_definition(&projection.clip_id)
                .unwrap()
                .fade_in,
            Some(0.)
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (3, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn marquee_batch_drag_preserves_spacing_and_has_one_undo_entry(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            let projection = view.editor.timeline_projection().unwrap();
            let channel = projection.tracks.values().next().unwrap().channel.clone();
            let d = view.editor.active_id().unwrap();
            for time in [0.5, 0.75] {
                view.editor.seek_timeline(d, time).unwrap();
                view.editor
                    .execute_timeline(
                        d,
                        editor_core::TimelineCommand::InsertKey {
                            channel: channel.clone(),
                            track_id: None,
                            selected_key: None,
                        },
                    )
                    .unwrap();
            }
            view.redraw(cx);
        });
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let track = projection.tracks.values().next().unwrap();
        let a = track.keys[1].reference.clone();
        let b = track.keys[2].reference.clone();
        let start = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&a)
            .unwrap()
            - point(px(12.), px(9.));
        let end = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&b)
            .unwrap()
            + point(px(12.), px(9.));
        press(window, start, cx);
        slide(window, end, false, cx);
        release(window, end, cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .keys
                .len(),
            2
        );
        let origin = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&a)
            .unwrap();
        let end = origin
            + point(
                window.find("timeline-graph").bounds().size.width / 8.,
                px(0.),
            );
        press(window, origin, cx);
        slide(window, end, false, cx);
        release(window, end, cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.key(&a).unwrap().time, 0.75);
        assert_eq!(projection.key(&b).unwrap().time, 1.);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (3, 0)
        );
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.key(&a).unwrap().time, 0.5);
        assert_eq!(projection.key(&b).unwrap().time, 0.75);
    })
    .unwrap();
}

#[gpui_kit::test]
fn cached_tracks_are_not_repainted_for_clock_only_updates(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
    })
    .unwrap();
    let paints = cx
        .update_window(harness.window, |_, window, cx| {
            window.draw(cx).clear(cx);
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .scene_paints()
        })
        .unwrap();
    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            view.editor.seek_timeline(d, 0.5).unwrap();
            view.redraw(cx);
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        // Normal draw, unlike render_frame's intentional full-window refresh.
        window.draw(cx).clear(cx);
        assert_eq!(
            window.find("timeline-overlay").bounds(),
            window.find("timeline-graph").bounds(),
            "Playhead and marquee must cover the graph, not flow below the cached tracks"
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .scene_paints(),
            paints
        );
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            0.5
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn committing_on_blur_keeps_the_next_field_focused_and_uses_the_new_asset(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let key = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key = projection.tracks.values().next().unwrap().keys[0]
                .reference
                .clone();
            let position = harness
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
            press(window, position, cx);
            release(window, position, cx);
            window.render_frame(cx);
            key
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("timeline-number-KeyValue({key:?})"), cx);
        window.press("cmd-a", cx);
        window.input("0.25", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("timeline-number-KeyTime({key:?})"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-a", cx);
        window.input("0.5", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let projection = view.editor.timeline_projection().unwrap();
        assert_eq!(projection.key(&key).unwrap().time, 0.5);
        assert_eq!(projection.key(&key).unwrap().value, 0.25);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
}

#[gpui_kit::test]
fn invalid_inspector_text_does_not_commit_the_last_valid_intermediate_value(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let (key, original) = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key_data = &projection.tracks.values().next().unwrap().keys[0];
            let key = key_data.reference.clone();
            let original = key_data.value;
            let position = harness
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
            press(window, position, cx);
            release(window, position, cx);
            window.render_frame(cx);
            (key, original)
        })
        .unwrap();
    for invalid in ["-", "999999"] {
        cx.update_window(harness.window, |_, window, cx| {
            window.click(format!("timeline-number-KeyValue({key:?})"), cx);
            window.press("cmd-a", cx);
            window.input("0.25", cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.press("cmd-a", cx);
            window.input(invalid, cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.timeline_edit().is_none());
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
            assert_eq!(
                view.editor
                    .timeline_projection()
                    .unwrap()
                    .key(&key)
                    .unwrap()
                    .value,
                original
            );
        });
    }
}

#[gpui_kit::test]
fn modal_cancels_a_candidate_and_late_pointer_events_do_not_restart_it(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let key = projection.tracks.values().next().unwrap().keys[0]
            .reference
            .clone();
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
        let end = origin
            + point(
                window.find("timeline-graph").bounds().size.width / 4.,
                px(0.),
            );
        press(window, origin, cx);
        slide(window, end, false, cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_some());
        harness.view.update(cx, |view, cx| {
            view.modal = true;
            view.redraw(cx);
        });
        window.render_frame(cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
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
        harness.view.update(cx, |view, cx| {
            view.modal = false;
            view.redraw(cx);
        });
        window.render_frame(cx);
        slide(window, end, false, cx);
        release(window, end, cx);
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
