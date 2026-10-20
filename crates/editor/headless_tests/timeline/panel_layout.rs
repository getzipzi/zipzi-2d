use super::super::slide_number_input::{press, release, slide};
use super::super::{document_fixture, setup_with_document};
use editor_core::CommandId;
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
            let info = PanelId::from(dock.properties.entity_id());
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
