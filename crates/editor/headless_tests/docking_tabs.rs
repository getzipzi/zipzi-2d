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
                let canvas = PanelId::from(dock.canvas.entity_id());
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
                    !dock.contains(PanelKind::AnimationParameters, cx),
                    "canvas stays independent"
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
            (dock.canvas.entity_id(), foreign.canvas.entity_id()),
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
