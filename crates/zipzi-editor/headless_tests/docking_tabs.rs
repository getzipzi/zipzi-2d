//! Exercise editor-owned tab chrome through the dock's real callbacks.
use super::setup;
use gpui_kit::component::dock::{DockLayout, DockPlacement, PaneRef, PanelId, panel_handle};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, px};
use zipzi_editor_core::PanelKind;

fn selector(
    area: &gpui_kit::Entity<gpui_kit::component::dock::DockArea>,
    panel: PanelId,
    cx: &gpui_kit::App,
) -> String {
    let area = area.read(cx);
    let node = [
        DockPlacement::Center,
        DockPlacement::Left,
        DockPlacement::Right,
        DockPlacement::Bottom,
    ]
    .into_iter()
    .find_map(|placement| area.layout(placement)?.find_panel_node(panel))
    .unwrap();
    format!("dock-selector-{}", node.as_u64())
}

fn right_click_header_padding(window: &mut gpui_kit::Window, id: String, cx: &mut gpui_kit::App) {
    let bounds = window.find(id.clone()).bounds();
    window.click_with_options(
        id,
        gpui_kit::test::ClickOptions::new()
            .with_offset(gpui_kit::point(
                bounds.size.width - px(5.),
                bounds.size.height / 2.,
            ))
            .with_button(gpui_kit::MouseButton::Right),
        cx,
    );
}

#[gpui_kit::test]
fn panel_selector_switches_and_context_menu_closes_only_the_active_panel(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, mesh, draw_order) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            PanelId::from(dock.part_tree.entity_id()),
            PanelId::from(dock.document_panels[&PanelKind::DrawOrder].entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click(selector(&area, mesh, cx), cx);
        window.render_frame(cx);
        assert!(window.find(format!("dock-select-{draw_order:?}")).visible());
        // Selection never offers creation, split, zoom or close actions.
        let group = area.read(cx).layout(DockPlacement::Center).unwrap();
        let node = group.find_panel_node(mesh).unwrap();
        let PaneRef::Tabs { panels, .. } = group.find_node(node).unwrap().kind() else {
            panic!("tabs")
        };
        assert!(window.within("popup-menu").try_find(panels.len()).is_none());
        window.click(format!("dock-select-{draw_order:?}"), cx);
        window.render_frame(cx);
        assert!(window.find(format!("dock-tab-{draw_order:?}")).visible());
        assert!(window.try_find(format!("dock-tab-{mesh:?}")).is_none());
        assert!(window.try_find("parts-panel-toolbar").is_none());
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.right_click(selector(&area, draw_order, cx), cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click(format!("dock-close-{draw_order:?}"), cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(area.read(cx).panel(draw_order).is_none());
        assert!(area.read(cx).panel(mesh).is_some());
        assert!(window.find("parts-panel-toolbar").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn single_panel_has_a_padded_draggable_tab_and_context_actions_without_a_selector(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    let (area, panel) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            PanelId::from(dock.properties.entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let layout = DockLayout::h_split()
            .child(
                DockLayout::tabs().panel_view(panel_handle(dock.properties.clone()), cx),
                Some(px(240.)),
            )
            .child(
                DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                None,
            );
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
        window.render_frame(cx);
        let selector_id = selector(&area, panel, cx);
        let header_id = selector_id.replace("dock-selector-", "dock-header-");
        let tab = format!("dock-tab-{panel:?}");
        assert!(window.try_find(selector_id.clone()).is_none());
        assert!(
            window
                .try_find(selector_id.replace("dock-selector-", "dock-menu-"))
                .is_none()
        );
        let header = window.find(header_id.clone()).bounds();
        let tab_bounds = window.find(tab.clone()).bounds();
        assert_eq!(tab_bounds.left() - header.left(), px(3.));
        assert!(tab_bounds.right() <= header.right() - px(3.));
        window.click(tab.clone(), cx);
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_none());
        right_click_header_padding(window, header_id, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(format!("dock-close-{panel:?}")).visible());
        // Creation is reachable through the tab context menu even with one tab.
        window.within("popup-menu").hover(0usize, cx);
        window.render_frame(cx);
        assert!(window.try_find("submenu").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn selector_keeps_every_grouped_panel_reachable_without_a_tab_strip(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let (area, last) = harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let mut layout = DockLayout::tabs();
            for kind in PanelKind::DOCUMENT_PANELS {
                layout = layout.panel_view(panel_handle(dock.document_panels[&kind].clone()), cx);
            }
            let layout = DockLayout::h_split().child(layout, Some(px(240.))).child(
                DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                None,
            );
            dock.area
                .update(cx, |area, cx| area.set_center(layout, window, cx));
            (
                dock.area.clone(),
                PanelId::from(dock.document_panels[&PanelKind::Resources].entity_id()),
            )
        });
        window.render_frame(cx);
        assert!(window.try_find("editor-dock-tabs-scroll").is_none());
        window.click(selector(&area, last, cx), cx);
        window.render_frame(cx);
        for kind in PanelKind::DOCUMENT_PANELS {
            let id = PanelId::from(
                harness.view.read(cx).dock.as_ref().unwrap().document_panels[&kind].entity_id(),
            );
            assert!(window.find(format!("dock-select-{id:?}")).visible());
        }
        window.click(format!("dock-select-{last:?}"), cx);
        window.render_frame(cx);
        assert!(window.find(format!("dock-tab-{last:?}")).visible());
        let header_id = selector(&area, last, cx).replace("dock-selector-", "dock-header-");
        assert_eq!(window.find(header_id).bounds().size.height, px(26.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn active_panel_drags_from_tab_and_full_header_between_groups(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, source, target, mesh) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            PanelId::from(dock.document_panels[&PanelKind::DrawOrder].entity_id()),
            PanelId::from(dock.properties.entity_id()),
            PanelId::from(dock.part_tree.entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| area.select_panel(source, window, cx));
        window.render_frame(cx);
        let source_bounds = window.find(format!("dock-tab-{source:?}")).bounds();
        let target_bounds = window.find(format!("dock-tab-{target:?}")).bounds();
        window.drag(source_bounds.center(), target_bounds.center(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    area.read_with(cx, |area, _| {
        let tree = area.layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(source), tree.find_panel_node(target));
    });
    cx.update_window(harness.window, |_, window, cx| {
        let header = window
            .find(selector(&area, source, cx).replace("dock-selector-", "dock-header-"))
            .bounds();
        let target = window.find(format!("dock-tab-{mesh:?}")).bounds();
        window.drag(header.center(), target.center(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    area.read_with(cx, |area, _| {
        let tree = area.layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(source), tree.find_panel_node(mesh));
        assert_ne!(tree.find_panel_node(source), tree.find_panel_node(target));
    });
}

#[gpui_kit::test]
fn selecting_header_search_text_does_not_drag_the_panel(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("parts-tree-search", cx);
        window.input("select this text", cx);
        let search = window.find("parts-tree-search").bounds();
        let from = gpui_kit::point(search.left() + px(45.), search.center().y);
        let to = from + gpui_kit::point(px(35.), px(0.));
        super::slide_number_input::press(window, from, cx);
        super::slide_number_input::slide(window, to, false, cx);
        window.render_frame(cx);
        assert!(!cx.has_active_drag(), "text selection started a dock drag");
        super::slide_number_input::release(window, to, cx);
        assert_eq!(window.find("parts-tree-search").focused(), Some(true));
        for id in ["parts-tree-filter", "btn_zoom_out"] {
            let from = window.find(id).bounds().center();
            let to = from + gpui_kit::point(px(35.), px(0.));
            super::slide_number_input::press(window, from, cx);
            super::slide_number_input::slide(window, to, false, cx);
            window.render_frame(cx);
            assert!(!cx.has_active_drag(), "{id} started a dock drag");
            super::slide_number_input::release(window, to, cx);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_drags_from_header_background_and_keeps_zoom_controls(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, canvas, mesh) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            dock.viewport_panel_id(),
            PanelId::from(dock.part_tree.entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        let header = window
            .find(selector(&area, canvas, cx).replace("dock-selector-", "dock-header-"))
            .bounds();
        let target = window.find(format!("dock-tab-{mesh:?}")).bounds();
        let from = gpui_kit::point(header.right() - px(140.), header.center().y);
        super::slide_number_input::press(window, from, cx);
        super::slide_number_input::slide(
            window,
            from - gpui_kit::point(px(20.), px(0.)),
            false,
            cx,
        );
        window.render_frame(cx);
        assert!(
            cx.has_active_drag(),
            "Canvas header did not initiate a drag: {header:?}, from {from:?}"
        );
        super::slide_number_input::slide(window, target.center(), false, cx);
        window.render_frame(cx);
        super::slide_number_input::release(window, target.center(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    // Dock moves arrive through TabGroupEvent after the input dispatch returns.
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(canvas), tree.find_panel_node(mesh));
        assert!(window.find("canvas-zoom-controls").visible());
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn collapsed_bottom_selector_reopens_the_selected_panel(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let (area, resources, pose) = harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let resources = dock.document_panels[&PanelKind::Resources].clone();
            let pose = dock.document_panels[&PanelKind::Pose].clone();
            let layout = DockLayout::tabs()
                .panel_view(panel_handle(resources.clone()), cx)
                .panel_view(panel_handle(pose.clone()), cx);
            dock.area.update(cx, |area, cx| {
                area.set_dock(DockPlacement::Bottom, layout, window, cx);
                area.toggle_dock(DockPlacement::Bottom, window, cx);
            });
            (
                dock.area.clone(),
                PanelId::from(resources.entity_id()),
                PanelId::from(pose.entity_id()),
            )
        });
        window.render_frame(cx);
        assert!(!area.read(cx).is_dock_open(DockPlacement::Bottom));
        window.click(selector(&area, resources, cx), cx);
        window.render_frame(cx);
        window.click(format!("dock-select-{pose:?}"), cx);
        window.render_frame(cx);
        assert!(area.read(cx).is_dock_open(DockPlacement::Bottom));
        assert!(window.find(format!("dock-tab-{pose:?}")).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn animation_add_menu_places_panel_in_the_clicked_group_and_restores_it(cx: &mut TestAppContext) {
    use zipzi_editor_core::{CommandId, PanelKind, WorkspaceKind};
    let harness = setup(cx);
    let (area, motions, physics, group) = cx
        .update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationWorkspace, window, cx);
            });
            window.render_frame(cx);
            window.simulate_next_frame(cx);
            let (area, motions, physics) = harness.view.read_with(cx, |view, _| {
                let dock = view.dock.as_ref().unwrap();
                (
                    dock.area.clone(),
                    PanelId::from(dock.animation_motions.entity_id()),
                    PanelId::from(dock.animation_inspectors[&PanelKind::AnimationPose].entity_id()),
                )
            });
            let group = area
                .read(cx)
                .layout(DockPlacement::Center)
                .unwrap()
                .find_panel_node(motions)
                .unwrap();
            assert!(
                window
                    .try_find(format!("dock-add-panel-{}", group.as_u64()))
                    .is_none()
            );
            assert!(
                window
                    .try_find(format!("dock-menu-{}", group.as_u64()))
                    .is_none()
            );
            right_click_header_padding(window, format!("dock-header-{}", group.as_u64()), cx);
            (area, motions, physics, group)
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        // Tab context menu > Add Panel > first unopened Animation panel.
        window.within("popup-menu").hover(0usize, cx);
        window.render_frame(cx);
        window.within("submenu").click(0usize, cx);
        window.render_frame(cx);
        let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(physics), Some(group));
        let PaneRef::Tabs { panels, active_ix } = tree.find_node(group).unwrap().kind() else {
            panic!("tabs")
        };
        assert_eq!(panels[active_ix], physics);
        assert!(window.find("AnimationPose").visible());
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::ModelingWorkspace, window, cx);
            view.run_command(CommandId::AnimationWorkspace, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(physics), tree.find_panel_node(motions));
        crate::docking::validate_layout(&area.read(cx).dump(cx), WorkspaceKind::Animation).unwrap();
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn targeted_panel_splits_preserve_direction_and_reject_stale_or_foreign_targets(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{base::Placement, component::dock::InsertTarget};
    use zipzi_editor_core::{CommandId, PanelKind};
    let harness = setup(cx);
    for placement in [Placement::Right, Placement::Bottom] {
        cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationWorkspace, window, cx);
                view.run_command(CommandId::ResetLayout, window, cx);
            });
            window.render_frame(cx);
            window.simulate_next_frame(cx);
            let (area, info, physics) = harness.view.read_with(cx, |view, _| {
                let dock = view.dock.as_ref().unwrap();
                (
                    dock.area.clone(),
                    PanelId::from(dock.properties.entity_id()),
                    PanelId::from(dock.animation_inspectors[&PanelKind::AnimationPose].entity_id()),
                )
            });
            let node = area
                .read(cx)
                .layout(DockPlacement::Center)
                .unwrap()
                .find_panel_node(info)
                .unwrap();
            harness.view.update(cx, |view, cx| {
                let dock = view.dock.as_ref().unwrap();
                let target = InsertTarget::Split {
                    node,
                    placement,
                    size: None,
                };
                dock.show(PanelKind::AnimationPose, Some(target), window, cx);
                dock.show(PanelKind::AnimationPose, Some(target), window, cx);
                dock.show(PanelKind::PartsTree, Some(target), window, cx);
            });
            window.render_frame(cx);
            let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
            assert!(tree.find_panel_node(physics).is_some());
            assert_ne!(tree.find_panel_node(physics), tree.find_panel_node(info));
            assert_eq!(tree.panels().filter(|panel| *panel == physics).count(), 1);
            let info_bounds = window.find("properties-panel").bounds();
            let physics_bounds = window.find("AnimationPose").bounds();
            match placement {
                Placement::Right => assert!(physics_bounds.left() >= info_bounds.right()),
                Placement::Bottom => assert!(physics_bounds.top() >= info_bounds.bottom()),
                _ => unreachable!(),
            }
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::ResetLayout, window, cx);
                let dock = view.dock.as_ref().unwrap();
                dock.show(
                    PanelKind::AnimationPose,
                    Some(InsertTarget::Tabs {
                        node,
                        ix: None,
                        activate: true,
                    }),
                    window,
                    cx,
                );
                assert!(
                    !dock.contains(PanelKind::AnimationPose, cx),
                    "a stale menu target must not fall back elsewhere"
                );
                let canvas = dock.viewport_panel_id();
                let node = dock
                    .area
                    .read(cx)
                    .layout(DockPlacement::Center)
                    .unwrap()
                    .find_panel_node(canvas)
                    .unwrap();
                dock.show(
                    PanelKind::AnimationPose,
                    Some(InsertTarget::Tabs {
                        node,
                        ix: None,
                        activate: true,
                    }),
                    window,
                    cx,
                );
                assert!(
                    dock.contains(PanelKind::AnimationPose, cx),
                    "canvas accepts grouped panels"
                );
            });
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn adding_to_collapsed_dock_reveals_the_new_panel(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::InsertTarget;
    use zipzi_editor_core::{CommandId, PanelKind};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            dock.area.update(cx, |area, cx| {
                area.remove_panel(dock.properties.clone(), window, cx);
                area.set_dock(
                    DockPlacement::Bottom,
                    DockLayout::tabs().panel_view(panel_handle(dock.properties.clone()), cx),
                    window,
                    cx,
                );
                area.toggle_dock(DockPlacement::Bottom, window, cx);
            });
            let node = dock
                .area
                .read(cx)
                .layout(DockPlacement::Bottom)
                .unwrap()
                .find_panel_node(PanelId::from(dock.properties.entity_id()))
                .unwrap();
            dock.show(
                PanelKind::AnimationPose,
                Some(InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                }),
                window,
                cx,
            );
            assert!(dock.area.read(cx).is_dock_open(DockPlacement::Bottom));
        });
        window.render_frame(cx);
        assert!(window.find("AnimationPose").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn workspace_restoration_keeps_each_windows_own_panel_instances(cx: &mut TestAppContext) {
    use zipzi_editor_core::CommandId;
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::ModelingWorkspace, window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    let (_other_window, other) = cx.update(|cx| {
        gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| crate::EditorView::new(window, cx))
        })
        .unwrap()
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::ModelingWorkspace, window, cx);
        });
        window.render_frame(cx);
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let foreign = other.read(cx).dock.as_ref().unwrap();
        for (own, foreign) in [
            (dock.viewport_panel_id(), foreign.viewport_panel_id()),
            (
                PanelId::from(dock.part_tree.entity_id()),
                PanelId::from(foreign.part_tree.entity_id()),
            ),
        ] {
            assert!(dock.area.read(cx).panel(own).is_some());
            assert!(dock.area.read(cx).panel(PanelId::from(foreign)).is_none());
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn global_add_menu_splits_beside_canvas_in_animation(cx: &mut TestAppContext) {
    use zipzi_editor_core::CommandId;
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        assert!(window.try_find("workspace-add-panel").is_none());
        window.click("workspace-layout-menu", cx);
        window.render_frame(cx);
        window.within("popup-menu").hover(0usize, cx);
        window.render_frame(cx);
        window.within("submenu").click(0usize, cx);
        window.render_frame(cx);
        let canvas = window.find("canvas-body").bounds();
        let panel = window.find("AnimationPose").bounds();
        assert!(panel.left() >= canvas.right());
        assert!(panel.size.width > px(0.));
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("workspace-layout-menu", cx);
        window.render_frame(cx);
        window.click("workspace-reset-layout", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("AnimationPose").is_none());
        assert!(window.find("properties-panel").visible());
        assert!(window.find("AnimationMotions").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn adding_before_next_frame_is_saved_when_leaving_workspace(cx: &mut TestAppContext) {
    use zipzi_editor_core::{CommandId, PanelKind};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx);
            view.run_command(CommandId::ModelingWorkspace, window, cx);
        });
        let saved = cx
            .global::<crate::settings::Settings>()
            .store
            .read::<crate::docking::LayoutConfig>("layout-animation.json")
            .unwrap()
            .unwrap();
        fn contains(node: &gpui_kit::component::dock::PanelState) -> bool {
            node.panel_name == "AnimationPhysics" || node.children.iter().any(contains)
        }
        assert!(contains(&saved.layout.center));
    })
    .unwrap();
}

#[gpui_kit::test]
fn workspace_menu_recovers_a_layout_without_any_panels(cx: &mut TestAppContext) {
    use zipzi_editor_core::{CommandId, PanelKind};
    let harness = setup(cx);
    for workspace in [CommandId::ModelingWorkspace, CommandId::AnimationWorkspace] {
        cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.run_command(workspace, window, cx);
                view.dock.as_ref().unwrap().area.update(cx, |area, cx| {
                    area.set_center(DockLayout::tabs(), window, cx);
                });
            });
            window.render_frame(cx);
            assert!(!harness.view.read(cx).dock.as_ref().unwrap().contains(
                PanelKind::primary_viewport(harness.view.read(cx).editor.workspace()),
                cx
            ));
            window.click("workspace-layout-menu", cx);
            window.render_frame(cx);
            // Recovery remains reachable beside the appearance settings.
            window.click("workspace-reset-layout", cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let dock = harness.view.read(cx).dock.as_ref().unwrap();
            assert!(dock.contains(
                PanelKind::primary_viewport(harness.view.read(cx).editor.workspace()),
                cx
            ));
            assert!(dock.contains(
                if workspace == CommandId::AnimationWorkspace {
                    PanelKind::Properties
                } else {
                    PanelKind::Properties
                },
                cx
            ));
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn first_row_owns_tree_controls_and_preserves_search_when_switching(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, mesh, order) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            PanelId::from(dock.part_tree.entity_id()),
            PanelId::from(dock.document_panels[&PanelKind::DrawOrder].entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        let header = window
            .find(selector(&area, mesh, cx).replace("dock-selector-", "dock-header-"))
            .bounds();
        for id in ["parts-tree-search", "parts-tree-filter"] {
            let control = window.find(id).bounds();
            assert!(header.contains(&control.origin));
            assert!(control.bottom() <= header.bottom());
        }
        window.click("parts-tree-search", cx);
        window.input("kept", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| area.select_panel(order, window, cx));
        window.render_frame(cx);
        assert!(window.try_find("parts-tree-search").is_none());
        area.update(cx, |area, cx| area.select_panel(mesh, window, cx));
        window.render_frame(cx);
        assert_eq!(window.find("parts-tree-search").value(), Some("kept"));
        window.click("parts-tree-clear", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("parts-tree-search").value(), Some(""));
        assert!(window.try_find("parts-tree-clear").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn narrow_panel_keeps_selector_and_tools_accessible(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, mesh) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (dock.area.clone(), PanelId::from(dock.part_tree.entity_id()))
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let layout = DockLayout::h_split()
                .child(
                    DockLayout::tabs().panel_view(panel_handle(dock.part_tree.clone()), cx),
                    Some(px(100.)),
                )
                .child(
                    DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                    None,
                );
            area.update(cx, |area, cx| area.set_center(layout, window, cx));
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let header_id = selector(&area, mesh, cx).replace("dock-selector-", "dock-header-");
        let header = window.find(header_id.clone()).bounds();
        let tools = window.within(header_id).find("panel-header-tools").bounds();
        assert!(header.contains(&tools.origin));
        assert!(tools.right() <= header.right());
        assert!(tools.bottom() <= header.bottom());
        window.click("panel-header-tools", cx);
        window.render_frame(cx);
        assert!(window.find("parts-tree-search").visible());
        window.click("parts-tree-search", cx);
        window.input("popup", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("parts-tree-search").value(), Some("popup"));
        window.press("escape", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_uses_the_same_selector_and_can_share_and_restore_a_panel_group(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::InsertTarget;
    let harness = setup(cx);
    let (area, canvas, mesh) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            dock.viewport_panel_id(),
            PanelId::from(dock.part_tree.entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| {
            let node = area
                .layout(DockPlacement::Center)
                .unwrap()
                .find_panel_node(mesh)
                .unwrap();
            area.move_panel(
                canvas,
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
        assert!(window.find(selector(&area, canvas, cx)).visible());
        assert!(window.find("canvas-toolbar").visible());
        assert!(window.try_find("parts-tree-search").is_none());
        crate::docking::validate_layout(
            &area.read(cx).dump(cx),
            zipzi_editor_core::WorkspaceKind::Modeling,
        )
        .unwrap();
        window.click(selector(&area, canvas, cx), cx);
        window.render_frame(cx);
        window.click(format!("dock-select-{mesh:?}"), cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("canvas-toolbar").is_none());
        assert!(window.find("parts-tree-search").visible());
        window.click(selector(&area, mesh, cx), cx);
        window.render_frame(cx);
        window.click(format!("dock-select-{canvas:?}"), cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("canvas-toolbar").visible());
        assert!(harness.view.read(cx).edit_error.is_none());
        assert_eq!(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .viewport_panel_id(),
            canvas
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_in_a_side_dock_remains_a_valid_target_for_new_panels(cx: &mut TestAppContext) {
    use zipzi_editor_core::{PanelKind, WorkspaceKind};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            dock.area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::tabs().panel_view(panel_handle(dock.part_tree.clone()), cx),
                    window,
                    cx,
                );
                area.set_dock(
                    DockPlacement::Right,
                    DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                    window,
                    cx,
                );
            });
            assert!(dock.canvas_node(cx).is_some());
            dock.show(PanelKind::Keyforms, None, window, cx);
            let area = dock.area.read(cx);
            let right = area.layout(DockPlacement::Right).unwrap();
            assert!(right.find_panel_node(dock.viewport_panel_id()).is_some());
            let keyforms = PanelId::from(dock.document_panels[&PanelKind::Keyforms].entity_id());
            assert!(right.find_panel_node(keyforms).is_some());
            crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Modeling).unwrap();
        });
        window.render_frame(cx);
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn properties_lock_uses_the_left_header_toolbar_and_updates_without_owner_redraw(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    let (area, panel) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            PanelId::from(dock.properties.entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let layout = DockLayout::h_split()
            .child(
                DockLayout::tabs().panel_view(panel_handle(dock.properties.clone()), cx),
                Some(px(240.)),
            )
            .child(
                DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                None,
            );
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let header_id = selector(&area, panel, cx).replace("dock-selector-", "dock-header-");
        let header = window.find(header_id.clone()).bounds();
        let tab = window.find(format!("dock-tab-{panel:?}")).bounds();
        let button = window
            .within(header_id)
            .within("properties-panel-toolbar")
            .find("property-inspection-lock");
        assert!(button.bounds().left() >= tab.right());
        assert!(button.bounds().right() < header.center().x);
        assert_eq!(
            button.bounds().size.height,
            crate::ui::compact::scaled(crate::ui::compact::HEADER_CONTROL, cx)
        );
        assert_eq!(button.checked(), Some(false));
        window.click("property-inspection-lock", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("property-inspection-lock").checked(),
            Some(true)
        );
        // Mouse activation preserves the editor's current focus. The button
        // remains reachable through the shared toolbar's keyboard traversal.
        for _ in 0..64 {
            window.focus_next(cx);
            window.render_frame(cx);
            if window.find("property-inspection-lock").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(
            window.find("property-inspection-lock").focused(),
            Some(true)
        );
        window.press("space", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("property-inspection-lock").checked(),
            Some(false)
        );
    })
    .unwrap();
}
