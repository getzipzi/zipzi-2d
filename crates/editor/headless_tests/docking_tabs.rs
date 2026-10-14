//! Exercise editor-owned tab chrome through the dock's real callbacks.
use super::setup;
use crate::panels::DocumentPanelKind;
use gpui_kit::component::dock::{DockLayout, DockPlacement, PaneRef, PanelId, panel_handle};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, px};

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

#[gpui_kit::test]
fn panel_selector_switches_and_context_menu_closes_only_the_active_panel(cx: &mut TestAppContext) {
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
        window.click(selector(&area, mesh, cx), cx);
        window.render_frame(cx);
        assert!(window.find(format!("dock-select-{draw_order:?}")).visible());
        window.click(format!("dock-select-{draw_order:?}"), cx);
        window.render_frame(cx);
        assert!(window.find(format!("dock-tab-{draw_order:?}")).visible());
        assert!(window.try_find(format!("dock-tab-{mesh:?}")).is_none());
        assert!(window.try_find("mesh-panel-toolbar").is_none());
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
        assert!(window.find("mesh-panel-toolbar").visible());
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
            for kind in DocumentPanelKind::ALL {
                layout = layout.panel_view(panel_handle(dock.document_panels[&kind].clone()), cx);
            }
            let layout = DockLayout::h_split().child(layout, Some(px(240.))).child(
                DockLayout::tabs().panel_view(panel_handle(dock.active_canvas().clone()), cx),
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
        assert!(window.try_find("editor-dock-tabs-scroll").is_none());
        window.click(selector(&area, last, cx), cx);
        window.render_frame(cx);
        for kind in DocumentPanelKind::ALL {
            let id = PanelId::from(
                harness.view.read(cx).dock.as_ref().unwrap().document_panels[&kind].entity_id(),
            );
            assert!(window.find(format!("dock-select-{id:?}")).visible());
        }
        window.click(format!("dock-select-{last:?}"), cx);
        window.render_frame(cx);
        assert!(window.find(format!("dock-tab-{last:?}")).visible());
        let header_id = selector(&area, last, cx).replace("dock-selector-", "dock-header-");
        assert_eq!(window.find(header_id).bounds().size.height, px(28.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn active_panel_selector_drags_between_groups(cx: &mut TestAppContext) {
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
}

#[gpui_kit::test]
fn collapsed_bottom_selector_reopens_the_selected_panel(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let (area, resources, physics) = harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let resources = dock.document_panels[&DocumentPanelKind::Resources].clone();
            let physics = dock.document_panels[&DocumentPanelKind::Physics].clone();
            let layout = DockLayout::tabs()
                .panel_view(panel_handle(resources.clone()), cx)
                .panel_view(panel_handle(physics.clone()), cx);
            dock.area.update(cx, |area, cx| {
                area.set_dock(DockPlacement::Bottom, layout, window, cx);
                area.toggle_dock(DockPlacement::Bottom, window, cx);
            });
            (
                dock.area.clone(),
                PanelId::from(resources.entity_id()),
                PanelId::from(physics.entity_id()),
            )
        });
        window.render_frame(cx);
        assert!(!area.read(cx).is_dock_open(DockPlacement::Bottom));
        window.click(selector(&area, resources, cx), cx);
        window.render_frame(cx);
        window.click(format!("dock-select-{physics:?}"), cx);
        window.render_frame(cx);
        assert!(area.read(cx).is_dock_open(DockPlacement::Bottom));
        assert!(window.find(format!("dock-tab-{physics:?}")).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn animation_add_menu_places_panel_in_the_clicked_group_and_restores_it(cx: &mut TestAppContext) {
    use editor_core::{CommandId, WorkspaceKind};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::AnimationParameters, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        let (area, motions, parameters) = harness.view.read_with(cx, |view, _| {
            let dock = view.dock.as_ref().unwrap();
            (
                dock.area.clone(),
                PanelId::from(dock.animation_motions.entity_id()),
                PanelId::from(dock.animation_parameters.entity_id()),
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
        window.click(format!("dock-menu-{}", group.as_u64()), cx);
        window.render_frame(cx);
        // More > Add Panel > the first unopened Animation panel, Parameters.
        window.within("popup-menu").hover(0usize, cx);
        window.render_frame(cx);
        window.within("submenu").click(0usize, cx);
        window.render_frame(cx);
        let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
        assert_eq!(tree.find_panel_node(parameters), Some(group));
        let PaneRef::Tabs { panels, active_ix } = tree.find_node(group).unwrap().kind() else {
            panic!("tabs")
        };
        assert_eq!(panels[active_ix], parameters);
        assert!(window.find("AnimationParameters").visible());
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::ModelingWorkspace, window, cx);
            view.run_command(CommandId::AnimationWorkspace, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
        assert_eq!(
            tree.find_panel_node(parameters),
            tree.find_panel_node(motions)
        );
        crate::docking::validate_layout(&area.read(cx).dump(cx), WorkspaceKind::Animation).unwrap();
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn targeted_panel_splits_preserve_direction_and_reject_stale_or_foreign_targets(
    cx: &mut TestAppContext,
) {
    use editor_core::{CommandId, PanelKind};
    use gpui_kit::{base::Placement, component::dock::InsertTarget};
    let harness = setup(cx);
    for placement in [Placement::Right, Placement::Bottom] {
        cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationWorkspace, window, cx);
                view.run_command(CommandId::ResetLayout, window, cx);
                view.run_command(CommandId::AnimationParameters, window, cx);
            });
            window.render_frame(cx);
            window.simulate_next_frame(cx);
            let (area, info, parameters) = harness.view.read_with(cx, |view, _| {
                let dock = view.dock.as_ref().unwrap();
                (
                    dock.area.clone(),
                    PanelId::from(dock.animation_info.entity_id()),
                    PanelId::from(dock.animation_parameters.entity_id()),
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
                dock.show(PanelKind::AnimationParameters, Some(target), window, cx);
                dock.show(PanelKind::AnimationParameters, Some(target), window, cx);
                dock.show(PanelKind::MeshTree, Some(target), window, cx);
            });
            window.render_frame(cx);
            let tree = area.read(cx).layout(DockPlacement::Center).unwrap();
            assert!(tree.find_panel_node(parameters).is_some());
            assert_ne!(tree.find_panel_node(parameters), tree.find_panel_node(info));
            assert_eq!(
                tree.panels().filter(|panel| *panel == parameters).count(),
                1
            );
            let info_bounds = window.find("AnimationInfo").bounds();
            let params_bounds = window.find("AnimationParameters").bounds();
            match placement {
                Placement::Right => assert!(params_bounds.left() >= info_bounds.right()),
                Placement::Bottom => assert!(params_bounds.top() >= info_bounds.bottom()),
                _ => unreachable!(),
            }
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::ResetLayout, window, cx);
                view.run_command(CommandId::AnimationParameters, window, cx);
                let dock = view.dock.as_ref().unwrap();
                dock.show(
                    PanelKind::AnimationParameters,
                    Some(InsertTarget::Tabs {
                        node,
                        ix: None,
                        activate: true,
                    }),
                    window,
                    cx,
                );
                assert!(
                    !dock.contains(PanelKind::AnimationParameters, cx),
                    "a stale menu target must not fall back elsewhere"
                );
                let canvas = PanelId::from(dock.active_canvas().entity_id());
                let node = dock
                    .area
                    .read(cx)
                    .layout(DockPlacement::Center)
                    .unwrap()
                    .find_panel_node(canvas)
                    .unwrap();
                dock.show(
                    PanelKind::AnimationParameters,
                    Some(InsertTarget::Tabs {
                        node,
                        ix: None,
                        activate: true,
                    }),
                    window,
                    cx,
                );
                assert!(
                    dock.contains(PanelKind::AnimationParameters, cx),
                    "canvas accepts grouped panels"
                );
            });
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn adding_to_collapsed_dock_reveals_the_new_panel(cx: &mut TestAppContext) {
    use editor_core::{CommandId, PanelKind};
    use gpui_kit::component::dock::InsertTarget;
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::AnimationParameters, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            dock.area.update(cx, |area, cx| {
                area.remove_panel(dock.animation_info.clone(), window, cx);
                area.set_dock(
                    DockPlacement::Bottom,
                    DockLayout::tabs().panel_view(panel_handle(dock.animation_info.clone()), cx),
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
                .find_panel_node(PanelId::from(dock.animation_info.entity_id()))
                .unwrap();
            dock.show(
                PanelKind::AnimationParameters,
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
        assert!(window.find("AnimationParameters").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn workspace_restoration_keeps_each_windows_own_panel_instances(cx: &mut TestAppContext) {
    use editor_core::CommandId;
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
            (
                dock.active_canvas().entity_id(),
                foreign.active_canvas().entity_id(),
            ),
            (dock.mesh_tree.entity_id(), foreign.mesh_tree.entity_id()),
        ] {
            assert!(dock.area.read(cx).panel(PanelId::from(own)).is_some());
            assert!(dock.area.read(cx).panel(PanelId::from(foreign)).is_none());
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn global_add_menu_splits_beside_canvas_in_animation(cx: &mut TestAppContext) {
    use editor_core::CommandId;
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::AnimationParameters, window, cx);
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
        let panel = window.find("AnimationParameters").bounds();
        assert!(panel.left() >= canvas.right());
        assert!(panel.size.width > px(0.));
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("workspace-layout-menu", cx);
        window.render_frame(cx);
        // Right, Below, separator, Reset Layout.
        window.within("popup-menu").click(3usize, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("AnimationParameters").visible());
        assert!(window.find("AnimationMotions").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn adding_before_next_frame_is_saved_when_leaving_workspace(cx: &mut TestAppContext) {
    use editor_core::{CommandId, PanelKind};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::AnimationParameters, window, cx);
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationParameters, None, window, cx);
            view.run_command(CommandId::ModelingWorkspace, window, cx);
        });
        let saved = cx
            .global::<crate::settings::Settings>()
            .store
            .read::<crate::docking::LayoutConfig>("layout-animation.json")
            .unwrap()
            .unwrap();
        fn contains(node: &gpui_kit::component::dock::PanelState) -> bool {
            node.panel_name == "AnimationParameters" || node.children.iter().any(contains)
        }
        assert!(contains(&saved.layout.center));
    })
    .unwrap();
}

#[gpui_kit::test]
fn workspace_menu_recovers_a_layout_without_any_panels(cx: &mut TestAppContext) {
    use editor_core::{CommandId, PanelKind};
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
            assert!(
                !harness
                    .view
                    .read(cx)
                    .dock
                    .as_ref()
                    .unwrap()
                    .contains(PanelKind::Canvas, cx)
            );
            window.click("workspace-layout-menu", cx);
            window.render_frame(cx);
            // Without a canvas target the recovery action is the sole menu item.
            window.within("popup-menu").click(0usize, cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let dock = harness.view.read(cx).dock.as_ref().unwrap();
            assert!(dock.contains(PanelKind::Canvas, cx));
            assert!(dock.contains(
                if workspace == CommandId::AnimationWorkspace {
                    PanelKind::AnimationProperties
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
            PanelId::from(dock.mesh_tree.entity_id()),
            PanelId::from(dock.document_panels[&DocumentPanelKind::DrawOrder].entity_id()),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        let header = window
            .find(selector(&area, mesh, cx).replace("dock-selector-", "dock-header-"))
            .bounds();
        for id in ["mesh-tree-search", "mesh-tree-filter"] {
            let control = window.find(id).bounds();
            assert!(header.contains(&control.origin));
            assert!(control.bottom() <= header.bottom());
        }
        window.click("mesh-tree-search", cx);
        window.input("kept", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| area.select_panel(order, window, cx));
        window.render_frame(cx);
        assert!(window.try_find("mesh-tree-search").is_none());
        area.update(cx, |area, cx| area.select_panel(mesh, window, cx));
        window.render_frame(cx);
        assert_eq!(window.find("mesh-tree-search").value(), Some("kept"));
        window.click("mesh-tree-clear", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("mesh-tree-search").value(), Some(""));
        assert!(window.try_find("mesh-tree-clear").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn narrow_panel_keeps_selector_and_tools_accessible(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (area, mesh) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (dock.area.clone(), PanelId::from(dock.mesh_tree.entity_id()))
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let layout = DockLayout::h_split()
                .child(
                    DockLayout::tabs().panel_view(panel_handle(dock.mesh_tree.clone()), cx),
                    Some(px(100.)),
                )
                .child(
                    DockLayout::tabs().panel_view(panel_handle(dock.active_canvas().clone()), cx),
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
        assert!(window.find("mesh-tree-search").visible());
        window.click("mesh-tree-search", cx);
        window.input("popup", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("mesh-tree-search").value(), Some("popup"));
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
            PanelId::from(dock.active_canvas().entity_id()),
            PanelId::from(dock.mesh_tree.entity_id()),
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
        assert!(window.try_find("mesh-tree-search").is_none());
        crate::docking::validate_layout(
            &area.read(cx).dump(cx),
            editor_core::WorkspaceKind::Modeling,
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
        assert!(window.find("mesh-tree-search").visible());
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
            PanelId::from(
                harness
                    .view
                    .read(cx)
                    .dock
                    .as_ref()
                    .unwrap()
                    .active_canvas()
                    .entity_id()
            ),
            canvas
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_in_a_side_dock_remains_a_valid_target_for_new_panels(cx: &mut TestAppContext) {
    use editor_core::{PanelKind, WorkspaceKind};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            dock.area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::tabs().panel_view(panel_handle(dock.mesh_tree.clone()), cx),
                    window,
                    cx,
                );
                area.set_dock(
                    DockPlacement::Right,
                    DockLayout::tabs().panel_view(panel_handle(dock.active_canvas().clone()), cx),
                    window,
                    cx,
                );
            });
            assert!(dock.canvas_node(cx).is_some());
            dock.show(PanelKind::Keyforms, None, window, cx);
            let area = dock.area.read(cx);
            let right = area.layout(DockPlacement::Right).unwrap();
            assert!(
                right
                    .find_panel_node(PanelId::from(dock.active_canvas().entity_id()))
                    .is_some()
            );
            let keyforms =
                PanelId::from(dock.document_panels[&DocumentPanelKind::Keyforms].entity_id());
            assert!(right.find_panel_node(keyforms).is_some());
            crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Modeling).unwrap();
        });
        window.render_frame(cx);
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}
