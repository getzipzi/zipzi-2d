//! Exercise editor-owned tab chrome through the dock's real callbacks.
use super::setup;
use crate::panels::DocumentPanelKind;
use gpui_kit::component::dock::{DockLayout, DockPlacement, PaneRef, PanelId, panel_handle};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, px};

#[gpui_kit::test]
fn compact_dock_tabs_select_and_close_the_context_target(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, mesh, draw_order) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            PanelId::from(dock.mesh_tree.entity_id()),
            PanelId::from(dock.document_panels[&DocumentPanelKind::DrawOrder].entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        let tab = format!("dock-tab-{draw_order:?}");
        assert!(window.find(tab.clone()).bounds().size.height <= px(28.));
        assert!(
            window
                .try_find(format!("dock-tab-close-{mesh:?}"))
                .is_none()
        );
        assert!(
            window
                .try_find(format!("dock-tab-close-{draw_order:?}"))
                .is_none()
        );
        window.click(tab, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let selected = |cx: &mut TestAppContext| {
        area.read_with(cx, |area, _| {
            let tree = area.layout(DockPlacement::Center).unwrap();
            let node = tree
                .find_node(tree.find_panel_node(draw_order).unwrap())
                .unwrap();
            let PaneRef::Tabs { panels, active_ix } = node.kind() else {
                panic!("expected tabs")
            };
            panels[active_ix]
        })
    };
    assert_eq!(selected(cx), draw_order);
    cx.update_window(harness.window, |_, window, cx| {
        // The inactive tab's context menu must close it by identity.
        window.right_click(format!("dock-tab-{mesh:?}"), cx);
    })
    .unwrap();
    assert_eq!(selected(cx), draw_order);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    assert_eq!(selected(cx), draw_order);
    area.read_with(cx, |area, _| {
        assert!(area.panel(mesh).is_none());
        assert!(area.panel(draw_order).is_some());
    });
}

#[gpui_kit::test]
fn selecting_overflowed_dock_tab_reveals_it(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let (area, last) = harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let mut layout = DockLayout::tabs();
            for kind in DocumentPanelKind::ALL {
                layout = layout.panel_view(panel_handle(dock.document_panels[&kind].clone()), cx);
            }
            let layout = DockLayout::h_split().child(layout, Some(px(240.))).child(
                DockLayout::tabs().panel_view(panel_handle(dock.canvas.clone()), cx),
                None,
            );
            dock.area
                .update(cx, |area, cx| area.set_center(layout, window, cx));
            (
                dock.area.clone(),
                PanelId::from(dock.document_panels[&DocumentPanelKind::Resources].entity_id()),
            )
        });
        window.render_frame(cx);
        let initial_tab = window.find(format!("dock-tab-{last:?}")).bounds();
        let viewport = window.find("editor-dock-tabs-scroll").bounds();
        assert!(
            initial_tab.right() > viewport.right(),
            "the test must overflow the strip"
        );
        area.update(cx, |area, cx| area.select_panel(last, window, cx));
        window.render_frame(cx);
        window.render_frame(cx);
        let tab = window.find(format!("dock-tab-{last:?}")).bounds();
        let viewport = window.find("editor-dock-tabs-scroll").bounds();
        assert!(tab.left() >= viewport.left());
        assert!(tab.right() <= viewport.right());
        assert_eq!(
            window.find("editor-dock-tab-bar").bounds().size.height,
            px(28.)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn compact_tabs_drag_between_groups(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, source, target) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            PanelId::from(dock.document_panels[&DocumentPanelKind::DrawOrder].entity_id()),
            PanelId::from(dock.properties.entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        let source_bounds = window.find(format!("dock-tab-{source:?}")).bounds();
        let target_bounds = window.find(format!("dock-tab-{target:?}")).bounds();
        window.drag(
            source_bounds.origin + gpui_kit::point(px(12.), px(12.)),
            target_bounds.center(),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    area.read_with(cx, |area, _| {
        let tree = area.layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(source), tree.find_panel_node(target));
    });
}

#[gpui_kit::test]
fn collapsed_bottom_tab_reopens_the_dock(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let (area, resources) = harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let resources = dock.document_panels[&DocumentPanelKind::Resources].clone();
            let layout = DockLayout::tabs()
                .panel_view(panel_handle(resources.clone()), cx)
                .panel_view(
                    panel_handle(dock.document_panels[&DocumentPanelKind::Physics].clone()),
                    cx,
                );
            dock.area.update(cx, |area, cx| {
                area.set_dock(DockPlacement::Bottom, layout, window, cx);
                area.toggle_dock(DockPlacement::Bottom, window, cx);
            });
            (dock.area.clone(), PanelId::from(resources.entity_id()))
        });
        window.render_frame(cx);
        assert!(!area.read(cx).is_dock_open(DockPlacement::Bottom));
        window.click(format!("dock-tab-{resources:?}"), cx);
        window.render_frame(cx);
        assert!(area.read(cx).is_dock_open(DockPlacement::Bottom));
    })
    .unwrap();
}
