use super::super::slide_number_input::{press, release, slide};
use super::super::{document_fixture, setup_with_document};
use editor_core::{CommandId, TimelineDisplayMode};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point, px};

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
