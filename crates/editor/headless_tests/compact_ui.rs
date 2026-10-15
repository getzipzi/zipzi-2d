use super::setup;
use crate::{
    settings::{Settings, UiScale},
    theme,
};
use gpui_kit::component::ThemeMode;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext, prelude::*};

#[gpui_kit::test]
fn scaled_parameter_rows_keep_their_spacing_after_scrolling(cx: &mut TestAppContext) {
    use gpui_kit::{ScrollDelta, point, px};
    let (document, assets) = super::document_fixture::sample_document_with_parameters(40).unwrap();
    let ids: Vec<_> = document.parameters().iter().map(|p| p.id.clone()).collect();
    let harness = super::setup_with_document(cx, document, assets);
    for scale in [UiScale::Large, UiScale::Largest, UiScale::Standard] {
        cx.update_window(harness.window, |_, window, cx| {
            cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale).unwrap());
            theme::change(ThemeMode::Dark, cx);
            window.refresh();
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            for (delta, pair) in [(10000., &ids[..2]), (-10000., &ids[38..])] {
                window.scroll(
                    "parameters-panel",
                    ScrollDelta::Pixels(point(px(0.), px(delta))),
                    cx,
                );
                let first = window.find(format!("parameter-row-{}", pair[0])).bounds();
                let second = window.find(format!("parameter-row-{}", pair[1])).bounds();
                assert!(
                    (second.top() - first.top() - px(22. * scale.factor())).abs() <= px(0.5),
                    "cached row spacing at {scale:?}: {first:?}, {second:?}"
                );
                assert!(first.bottom() <= second.top() + px(0.5));
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn scaled_parameter_markers_stay_centered_on_their_values(cx: &mut TestAppContext) {
    use gpui_kit::px;
    let (document, assets) = super::document_fixture::sample_document_with_parameters(1).unwrap();
    let parameter = document.parameters()[0].clone();
    let harness = super::setup_with_document(cx, document, assets);
    for scale in UiScale::ALL {
        cx.update_window(harness.window, |_, window, cx| {
            cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale).unwrap());
            theme::change(ThemeMode::Dark, cx);
            window.refresh();
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let row = window.within(format!("parameter-row-{}", parameter.id));
            let track = row.find("slider-bar-container").bounds();
            for (value, expected) in [
                (parameter.minimum, track.left()),
                (parameter.maximum, track.right()),
            ] {
                let marker = row
                    .find(format!("parameter-key-{}-{value}", parameter.id))
                    .bounds();
                assert!(
                    (marker.center().x - expected).abs() <= px(0.5),
                    "marker offset at {scale:?}: {marker:?}"
                );
            }
            let thumb = row.find(("slider-thumb", 0_u32)).bounds();
            let fraction =
                (parameter.value - parameter.minimum) / (parameter.maximum - parameter.minimum);
            assert!(
                (thumb.center().x - (track.left() + track.size.width * fraction)).abs() <= px(0.5),
                "thumb offset at {scale:?}: {thumb:?} on {track:?}"
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn ui_scale_changes_controls_rows_and_chrome_together(cx: &mut TestAppContext) {
    let harness = setup(cx);
    for scale in UiScale::ALL {
        cx.update_window(harness.window, |_, window, cx| {
            cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale).unwrap());
            theme::change(ThemeMode::Dark, cx);
            window.refresh();
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let factor = scale.factor();
            for (id, expected) in [
                ("workspace-switcher", 24. * factor),
                ("app-status-bar", 19.25 * factor),
                ("object-property-name", 22. * factor),
            ] {
                let height = window.find(id).bounds().size.height.as_f32();
                assert!(
                    (height - expected).abs() <= 1.,
                    "{id}: {height} at {factor}"
                );
            }
            let dock = harness.view.read(cx).dock.as_ref().unwrap();
            let tree = dock
                .area
                .read(cx)
                .layout(gpui_kit::component::dock::DockPlacement::Center)
                .unwrap();
            let node = tree
                .find_panel_node(gpui_kit::component::dock::PanelId::from(
                    dock.mesh_tree.entity_id(),
                ))
                .unwrap();
            let header = window
                .find(format!("dock-header-{}", node.as_u64()))
                .bounds();
            let selector = window
                .find(format!("dock-selector-{}", node.as_u64()))
                .bounds();
            for control in [
                selector,
                window.find("mesh-tree-search").bounds(),
                window.find("mesh-tree-filter").bounds(),
            ] {
                assert!(
                    (control.size.height.as_f32() - 20. * factor).abs() <= 0.5,
                    "header control height: {control:?}"
                );
                assert!(
                    (control.center().y - header.center().y).abs() <= gpui_kit::px(0.5),
                    "header control alignment: {control:?} in {header:?}"
                );
            }
            let search = window.find("mesh-tree-search").bounds();
            assert!(
                (header.right() - search.right() - (search.top() - header.top())).abs()
                    <= gpui_kit::px(0.5),
                "tree header right inset differs from top inset: {search:?} in {header:?}"
            );
            let canvas_node = tree
                .find_panel_node(gpui_kit::component::dock::PanelId::from(
                    dock.active_canvas().entity_id(),
                ))
                .unwrap();
            let canvas_header = window
                .find(format!("dock-header-{}", canvas_node.as_u64()))
                .bounds();
            let zoom = window.find("canvas-zoom-controls").bounds();
            assert!(
                (canvas_header.right() - zoom.right() - (zoom.top() - canvas_header.top())).abs()
                    <= gpui_kit::px(0.5),
                "canvas header right inset differs from top inset: {zoom:?} in {canvas_header:?}"
            );
            for id in ["app-status-message"] {
                let left = window.find(id).bounds().left();
                assert!(
                    (left - selector.left()).abs() <= gpui_kit::px(0.5),
                    "{id} content inset differs from panel header"
                );
            }
            let canvas = window.find("canvas-body").bounds();
            // Only the panel frame supplies spacing below the title bar.
            let expected_gap = crate::ui::compact::scaled(crate::ui::compact::PANEL_GAP, cx);
            let titlebar = window.find("workspace-titlebar").bounds();
            let switcher = window.find("workspace-switcher").bounds();
            assert!((switcher.center().x - titlebar.center().x).abs() <= gpui_kit::px(0.5));
            let top_gap = header.top() - titlebar.bottom();
            assert!(
                (top_gap - expected_gap / 2.).abs() <= gpui_kit::px(0.5),
                "titlebar gap {top_gap:?} differs from {:?}",
                expected_gap / 2.
            );
            let message = window.find("app-status-message").bounds();
            let status_gap = message.top() - canvas.bottom() - gpui_kit::px(1.);
            assert!(
                (status_gap - expected_gap).abs() <= gpui_kit::px(0.5),
                "statusbar gap {status_gap:?} differs from {expected_gap:?}"
            );
            let area = window.find("dock-area").bounds();
            assert!(area.contains(&canvas.origin));
            assert!(canvas.right() <= area.right());
            assert!(canvas.bottom() <= area.bottom());
            // Theme changes retain the selected object and read-only properties.
            assert!(window.try_find("object-property-input").is_none());
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn property_groups_collapse_without_editing_the_document(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(window.find("object-property-name").visible());
        window.click("property-group-", cx);
        window.render_frame(cx);
        assert!(window.try_find("object-property-name").is_none());
        window.click("property-group-", cx);
        window.render_frame(cx);
        assert!(window.find("object-property-name").visible());
        let doc = harness.view.read(cx).editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(!doc.modified());
    })
    .unwrap();
}

#[gpui_kit::test]
fn moving_timeline_to_a_side_group_releases_the_empty_bottom_dock(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let area = dock.area.clone();
        let timeline = PanelId::from(dock.timeline.entity_id());
        let target = area
            .read(cx)
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(PanelId::from(dock.animation_info.entity_id()))
            .unwrap();
        area.update(cx, |area, cx| {
            area.move_panel(
                timeline,
                InsertTarget::Tabs {
                    node: target,
                    ix: None,
                    activate: true,
                },
                window,
                cx,
            )
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let area = &harness.view.read(cx).dock.as_ref().unwrap().area;
        assert!(!area.read(cx).has_dock(DockPlacement::Bottom));
        assert!(window.find("animation-timeline").visible());
    })
    .unwrap();
}
