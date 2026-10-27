use super::{document_fixture, setup, setup_with_document};
use gpui_kit::{
    AppContext as _, InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    TestAppContext, point, px, size, test::TestWindowExt,
};

fn id(value: u128) -> String {
    uuid::Uuid::from_u128(value).to_string()
}

fn begin_drag(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    from: gpui_kit::Point<gpui_kit::Pixels>,
    to: gpui_kit::Point<gpui_kit::Pixels>,
) {
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position: from,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseMoveEvent {
            position: from + point(px(8.), px(0.)),
            pressed_button: Some(MouseButton::Left),
            modifiers: Default::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
    window.dispatch_event(
        MouseMoveEvent {
            position: to,
            pressed_button: Some(MouseButton::Left),
            modifiers: Default::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
fn tree_icons_and_labels_start_dragging_without_including_row_controls(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let mesh = document.layers()[0].id.clone();
    let deformer = id(501);
    let harness = setup_with_document(cx, document, assets);
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert!(view.mesh_thumbnails.read(cx).cached_image(&mesh).is_some());
    });
    for (prefix, panel) in [
        ("parts", zipzi_editor_core::PanelKind::PartsTree),
        ("deformer", zipzi_editor_core::PanelKind::Deformers),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.dock.as_ref().unwrap().show(panel, None, window, cx);
            });
            window.render_frame(cx);
            let to = window
                .find(format!("{prefix}-tree-root-drop"))
                .bounds()
                .center();
            for (source, area) in [(&mesh, "icon"), (&deformer, "icon"), (&mesh, "label")] {
                let from = window
                    .find(format!("{prefix}-{area}-{source}"))
                    .bounds()
                    .center();
                begin_drag(window, cx, from, to);
                assert!(
                    cx.has_active_drag(),
                    "{prefix} {area} {source}: drag did not start"
                );
                window.press("escape", cx);
                window.dispatch_event(
                    MouseUpEvent {
                        button: MouseButton::Left,
                        position: to,
                        modifiers: Default::default(),
                        click_count: 1,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
                assert!(!cx.has_active_drag());
            }
            for control in [
                format!("{prefix}-vis-{mesh}"),
                format!("{prefix}-lock-{mesh}"),
                format!("tree-expand-RotationDeformer:{deformer}"),
            ] {
                let from = window
                    .within(format!("{prefix}-tree-panel"))
                    .find(control.clone())
                    .bounds()
                    .center();
                begin_drag(window, cx, from, to);
                assert!(!cx.has_active_drag(), "{prefix}: {control} started a drag");
                window.dispatch_event(
                    MouseUpEvent {
                        button: MouseButton::Left,
                        position: to,
                        modifiers: Default::default(),
                        click_count: 1,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
            }
            let history = harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths();
            let from = window
                .find(format!("{prefix}-icon-{mesh}"))
                .bounds()
                .center();
            window.drag(from, to, cx);
            window.render_frame(cx);
            harness.view.read_with(cx, |view, _| {
                let document = view.editor.document().unwrap();
                let index = document.object_index();
                let properties = &index
                    .get(&zipzi_editor_core::ObjectRef::new(
                        zipzi_editor_core::ObjectKind::Mesh,
                        &mesh,
                    ))
                    .unwrap()
                    .properties;
                assert_eq!(
                    properties[if prefix == "parts" {
                        "part_id"
                    } else {
                        "deformer_id"
                    }],
                    ""
                );
                assert_eq!(document.history_lengths(), (history.0 + 1, 0));
                assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            });
        })
        .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn motions_can_be_dragged_to_root_without_a_virtual_group(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let moved = document.motion_resources()[0].selection.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        let from = window.find(("animation-motion", 0usize)).bounds().center();
        let to = window.find("animation-motion-root-drop").bounds().center();
        window.drag(from, to, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let resources = harness
            .view
            .read(cx)
            .editor
            .document()
            .unwrap()
            .motion_resources();
        let index = resources
            .iter()
            .position(|resource| resource.selection == moved)
            .unwrap();
        assert!(resources[index].group_id.is_none());
        assert!(window.find(("animation-motion", index)).visible());
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
        assert!(
            window
                .try_find(("animation-motion-group-header", 1usize))
                .is_none()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn root_receives_parameters_without_an_ungrouped_header(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    document
        .move_parameter(
            &id(5),
            zipzi_editor_core::ParameterDestination::Group(Some(id(104))),
        )
        .unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("parameter-group-ungrouped").is_none());
        let from = window
            .find(format!("parameter-name-{}", id(3)))
            .bounds()
            .center();
        let to = window.find("parameters-root-drop").bounds().center();
        window.drag(from, to, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("parameter-group-ungrouped").is_none());
        assert!(window.find(format!("parameter-name-{}", id(3))).visible());
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .parameters()
                .iter()
                .find(|p| p.id == id(3))
                .unwrap()
                .group_id
                .is_none()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn sorting_a_long_parameter_list_keeps_scroll_position(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(60).unwrap();
    let harness = setup_with_document(cx, document, assets);
    let scroll = harness.view.read_with(cx, |view, cx| {
        view.dock
            .as_ref()
            .unwrap()
            .parameters
            .read(cx)
            .sorting_scroll_handle()
    });
    scroll.scroll_to_item_strict(20, gpui_kit::ScrollStrategy::Top);
    let before = cx
        .update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let offset = scroll.0.borrow().base_handle.offset();
            let from = window
                .find(format!("parameter-name-{}", id(22)))
                .bounds()
                .center();
            let target = window.find(format!("parameter-name-{}", id(21))).bounds();
            window.drag(from, point(target.center().x, target.top() + px(2.)), cx);
            offset
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(scroll.0.borrow().base_handle.offset(), before);
        assert!(
            window
                .find(format!("parameter-name-{}", id(22)))
                .bounds()
                .top()
                < window
                    .find(format!("parameter-name-{}", id(21)))
                    .bounds()
                    .top()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn filtered_tree_uses_stable_targets_and_document_switch_cancels_a_drag(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (first, second, original, active) = harness.view.read_with(cx, |view, _| {
        let tree = view.editor.part_tree();
        (
            tree[0].children[0].id.clone(),
            tree[0].children[1].id.clone(),
            tree,
            view.editor.active_id().unwrap(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click("parts-tree-filter", cx);
        window.click("tree-kind-Part", cx);
        window.click("parts-tree-filter", cx);
        let from = window.find(format!("parts-item-Mesh:{second}")).bounds();
        let to = window.find(format!("parts-item-Mesh:{first}")).bounds();
        window.drag(
            point(from.left() + px(80.), from.center().y),
            point(to.left() + px(80.), to.top() + px(2.)),
            cx,
        );
        assert_eq!(
            harness.view.read(cx).editor.part_tree()[0].children[0].id,
            second
        );
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(zipzi_editor_core::EditorCommand::Undo)
                .unwrap();
            view.redraw(cx);
        });
        window.click("parts-tree-clear", cx);
        window.render_frame(cx);
        begin_drag(
            window,
            cx,
            point(from.left() + px(80.), from.center().y),
            point(to.left() + px(80.), to.top() + px(2.)),
        );
        assert!(cx.has_active_drag());
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(!cx.has_active_drag());
    })
    .unwrap();
    harness.view.update(cx, |view, cx| {
        view.editor.activate(active).unwrap();
        cx.notify();
    });
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.part_tree(), original);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 1));
    });
}

#[gpui_kit::test]
fn hovering_opens_a_collapsed_parameter_group_without_creating_history(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click(format!("parameter-group-toggle-{}", id(103)), cx);
        let from = window
            .find(format!("parameter-name-{}", id(3)))
            .bounds()
            .center();
        let to = window
            .find(format!("parameter-group-{}", id(103)))
            .bounds()
            .center();
        begin_drag(window, cx, from, to);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(103)))
                .expanded(),
            Some(false)
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(103)))
                .expanded(),
            Some(true)
        );
        window.press("escape", cx);
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
fn dragging_at_the_list_edge_scrolls_and_escape_stops_the_timer(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(60).unwrap();
    let harness = setup_with_document(cx, document, assets);
    let scroll = harness.view.read_with(cx, |view, cx| {
        view.dock
            .as_ref()
            .unwrap()
            .parameters
            .read(cx)
            .sorting_scroll_handle()
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window
            .find(format!("parameter-name-{}", id(1)))
            .bounds()
            .center();
        let bounds = window.find("parameters-scroll-frame").bounds();
        begin_drag(window, cx, from, point(from.x, bounds.bottom() - px(8.)));
    })
    .unwrap();
    cx.run_until_parked();
    for _ in 0..3 {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(70));
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    assert!(scroll.0.borrow().base_handle.offset().y < px(0.));
    let stopped = cx
        .update_window(harness.window, |_, window, cx| {
            window.press("escape", cx);
            assert!(!cx.has_active_drag());
            window.render_frame(cx);
            scroll.0.borrow().base_handle.offset()
        })
        .unwrap();
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(scroll.0.borrow().base_handle.offset(), stopped);
    assert_eq!(
        harness.view.read_with(cx, |view, _| view
            .editor
            .document()
            .unwrap()
            .history_lengths()),
        (0, 0)
    );
}

#[gpui_kit::test]
fn tree_drag_reorders_siblings_once_and_undo_restores_both_rows(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.part_tree());
    let first = original[0].children[0].id.clone();
    let second = original[0].children[1].id.clone();
    cx.update_window(harness.window, |_, window, cx| {
        let from = window.find(format!("parts-item-Mesh:{second}")).bounds();
        let to = window.find(format!("parts-item-Mesh:{first}")).bounds();
        window.drag(
            point(from.left() + px(80.), from.center().y),
            point(to.left() + px(80.), to.top() + px(3.)),
            cx,
        );
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert_eq!(view.editor.part_tree()[0].children[0].id, second);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            assert_eq!(view.editor.deformer_tree()[0].children[0].id, first);
        });
        harness.view.update(cx, |view, cx| {
            let result = view.editor.execute(zipzi_editor_core::EditorCommand::Undo);
            view.complete_edit(result, cx);
        });
        window.render_frame(cx);
        assert!(
            window
                .find(format!("parts-item-Mesh:{first}"))
                .bounds()
                .top()
                < window
                    .find(format!("parts-item-Mesh:{second}"))
                    .bounds()
                    .top()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn deformer_tree_drag_keeps_part_tree_order(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (first, second, parts) = harness.view.read_with(cx, |view, _| {
        let tree = view.editor.deformer_tree();
        (
            tree[0].children[0].id.clone(),
            tree[0].children[1].id.clone(),
            view.editor.part_tree(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let panel = gpui_kit::component::dock::PanelId::from(dock.deformer_tree.entity_id());
            dock.area
                .update(cx, |area, cx| area.select_panel(panel, window, cx));
        });
        window.render_frame(cx);
        let from = window.find(format!("deformer-item-Mesh:{second}")).bounds();
        let to = window.find(format!("deformer-item-Mesh:{first}")).bounds();
        window.drag(
            point(from.left() + px(80.), from.center().y),
            point(to.left() + px(80.), to.top() + px(2.)),
            cx,
        );
        assert_eq!(
            harness.view.read(cx).editor.deformer_tree()[0].children[0].id,
            second
        );
        assert_eq!(harness.view.read(cx).editor.part_tree(), parts);
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
    })
    .unwrap();
}

#[gpui_kit::test]
fn parameter_names_drag_between_parameters_and_into_empty_groups(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(format!("parameter-name-{}", id(4))).bounds();
        let to = window.find(format!("parameter-name-{}", id(3))).bounds();
        window.drag(from.center(), point(to.center().x, to.top() + px(2.)), cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(format!("parameter-name-{}", id(4)))
                .bounds()
                .top()
                < window
                    .find(format!("parameter-name-{}", id(3)))
                    .bounds()
                    .top()
        );
        let from = window.find(format!("parameter-name-{}", id(3))).bounds();
        let to = window.find(format!("parameter-group-{}", id(104))).bounds();
        window.drag(from.center(), point(from.center().x, to.center().y), cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            let document = view.editor.document().unwrap();
            assert_eq!(
                document
                    .parameters()
                    .iter()
                    .find(|p| p.id == id(3))
                    .unwrap()
                    .group_id,
                Some(id(104))
            );
            assert_eq!(document.history_lengths(), (2, 0));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn group_drag_keeps_descendants_together_and_cancellation_does_not_write(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(format!("parameter-group-{}", id(103))).bounds();
        let to = window.find(format!("parameter-group-{}", id(101))).bounds();
        window.drag(
            point(from.left() + px(90.), from.center().y),
            point(to.left() + px(90.), to.top() + px(2.)),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(format!("parameter-group-{}", id(103)))
                .bounds()
                .top()
                < window
                    .find(format!("parameter-group-{}", id(101)))
                    .bounds()
                    .top()
        );
        let from = window
            .find(format!("parameter-name-{}", id(3)))
            .bounds()
            .center();
        let to = window
            .find(format!("parameter-group-{}", id(104)))
            .bounds()
            .center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseMoveEvent {
                position: from + point(px(8.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert!(cx.has_active_drag());
        window.press("escape", cx);
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert!(!cx.has_active_drag());
        harness.view.read_with(cx, |view, _| {
            let document = view.editor.document().unwrap();
            assert_eq!(document.history_lengths(), (1, 0));
            assert_eq!(
                document
                    .parameters()
                    .iter()
                    .find(|p| p.id == id(3))
                    .unwrap()
                    .group_id,
                Some(id(102))
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn tree_drag_moves_parts_with_contents_and_can_unassign_meshes(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window
            .find(format!("parts-drag-{}", id(103)))
            .bounds()
            .center();
        let to = window
            .find(format!("parts-item-Part:{}", id(102)))
            .bounds()
            .center();
        window.drag(from, to, cx);
        window.render_frame(cx);
        let mesh = harness.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            let tree = view.editor.part_tree();
            assert!(tree[0].children.iter().all(|node| node.id != id(103)));
            assert_eq!(tree[1].children[0].id, id(103));
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            tree[1].children[0].children[0].id.clone()
        });
        let from = window.find(format!("parts-drag-{mesh}")).bounds().center();
        let to = window.find("parts-tree-root-drop").bounds().center();
        window.drag(from, to, cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .part_tree()
                .last()
                .unwrap()
                .children[0]
                .id,
            mesh
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
fn tree_hover_expands_parts_and_cycle_drop_is_rejected(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("parts-tree-panel")
            .click(format!("tree-expand-Part:{}", id(101)), cx);
        let from = window
            .find(format!("parts-drag-{}", id(102)))
            .bounds()
            .center();
        let to = window
            .find(format!("parts-item-Part:{}", id(101)))
            .bounds()
            .center();
        begin_drag(window, cx, from, to);
    })
    .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(format!("parts-item-Part:{}", id(103)))
                .is_some()
        );
        window.press("escape", cx);
        let from = window
            .find(format!("parts-drag-{}", id(101)))
            .bounds()
            .center();
        let to = window
            .find(format!("parts-item-Part:{}", id(103)))
            .bounds()
            .center();
        window.drag(from, to, cx);
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
        assert!(harness.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn tree_visibility_controls_do_not_start_sorting(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let tree = harness.view.read(cx).editor.part_tree();
        let mesh = &tree[0].children[0].id;
        let from = window.find(format!("parts-vis-{mesh}")).bounds().center();
        let to = window.find("parts-tree-root-drop").bounds().center();
        begin_drag(window, cx, from, to);
        assert!(!cx.has_active_drag());
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
fn groups_can_cross_containers_and_return_to_root_with_undo(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    // Before/After on a nested folder uses its actual parent collection.
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(format!("parameter-group-{}", id(103))).bounds();
        let to = window.find(format!("parameter-group-{}", id(102))).bounds();
        window.drag(
            point(from.left() + px(90.), from.center().y),
            point(to.left() + px(90.), to.top() + px(2.)),
            cx,
        );
        harness.view.read_with(cx, |v, _| {
            let document = v.editor.document().unwrap();
            assert_eq!(document.history_lengths().0, 1);
            assert_eq!(
                document.parameter_group_draft(&id(103)).unwrap().parent_id,
                Some(id(101))
            );
        });
        harness.view.update(cx, |v, cx| {
            v.editor
                .execute(zipzi_editor_core::EditorCommand::Undo)
                .unwrap();
            v.redraw(cx);
        });
    })
    .unwrap();
    for target in [
        format!("parameter-group-{}", id(101)),
        "parameters-root-drop".into(),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let from = window.find(format!("parameter-group-{}", id(102))).bounds();
            let to = window.find(target.clone()).bounds();
            window.drag(
                point(from.left() + px(90.), from.center().y),
                point(to.left() + px(90.), to.top() + px(2.)),
                cx,
            );
        })
        .unwrap();
        harness.view.read_with(cx, |v, _| {
            assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
            let doc = v.editor.document().unwrap();
            assert_eq!(doc.parameter_group_draft(&id(102)).unwrap().parent_id, None);
            assert_eq!(doc.parameter_draft(&id(3)).unwrap().group_id, Some(id(102)));
            assert_eq!(doc.history_lengths().0, 1);
        });
        cx.update_window(harness.window, |_, _, cx| {
            harness.view.update(cx, |v, cx| {
                let result = v.editor.execute(zipzi_editor_core::EditorCommand::Undo);
                v.complete_edit(result, cx);
            })
        })
        .unwrap();
        harness.view.read_with(cx, |v, _| {
            assert_eq!(
                v.editor
                    .document()
                    .unwrap()
                    .parameter_group_draft(&id(102))
                    .unwrap()
                    .parent_id,
                Some(id(101))
            )
        });
    }
    // The root drop target also moves an already top-level group to the end.
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(format!("parameter-group-{}", id(101))).bounds();
        let to = window.find("parameters-root-drop").bounds();
        window.drag(
            point(from.left() + px(90.), from.center().y),
            point(to.left() + px(90.), to.top() + px(2.)),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |v, _| {
        let groups = v.editor.document().unwrap().parameter_groups();
        assert_eq!(
            groups.iter().rfind(|g| g.parent_id.is_none()).unwrap().id,
            id(101)
        );
        assert_eq!(
            groups.iter().find(|g| g.id == id(102)).unwrap().parent_id,
            Some(id(101))
        );
    });
}

#[gpui_kit::test]
fn dropping_a_folder_inside_a_collapsed_folder_reveals_it_and_undo_keeps_ids(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(h.window, size(px(1100.), px(1400.)));
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click(format!("parameter-group-toggle-{}", id(102)), cx);
        let from = window.find(format!("parameter-group-{}", id(103))).bounds();
        let to = window.find(format!("parameter-group-{}", id(102))).bounds();
        window.drag(
            point(from.left() + px(90.), from.center().y),
            point(to.left() + px(90.), to.center().y),
            cx,
        );
        window.render_frame(cx);
        let doc = h.view.read(cx).editor.document().unwrap();
        assert_eq!(
            doc.parameter_group_draft(&id(103)).unwrap().parent_id,
            Some(id(102))
        );
        assert_eq!(doc.history_lengths(), (1, 0));
        assert!(
            window
                .find(format!("parameter-group-{}", id(103)))
                .visible()
        );
        h.view.update(cx, |view, cx| {
            view.run_command(zipzi_editor_core::CommandId::Undo, window, cx);
        });
        window.render_frame(cx);
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .parameter_group_draft(&id(103))
                .unwrap()
                .parent_id,
            None
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn invalid_cycle_drop_shows_the_core_reason_without_modifying_history(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(h.window, size(px(1100.), px(1400.)));
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(format!("parameter-group-{}", id(101))).bounds();
        let to = window.find(format!("parameter-group-{}", id(102))).bounds();
        begin_drag(
            window,
            cx,
            point(from.left() + px(90.), from.center().y),
            point(to.left() + px(90.), to.center().y),
        );
        window.render_frame(cx);
        assert!(window.find("palette-drop-reason").visible());
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
        window.press("escape", cx);
        assert!(!cx.has_active_drag());
    })
    .unwrap();
}

#[gpui_kit::test]
fn tree_multiselection_moves_as_one_transaction_without_changing_selection(
    cx: &mut TestAppContext,
) {
    use zipzi_editor_core::{EditorCommand, ObjectKind, ObjectRef};
    let (document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let h = setup_with_document(cx, document, assets);
    let meshes = h.view.read_with(cx, |view, _| {
        let tree = view.editor.part_tree();
        fn collect(nodes: &[zipzi_editor_core::TreeNode], out: &mut Vec<ObjectRef>) {
            for node in nodes {
                if node.kind == zipzi_editor_core::TreeNodeKind::Mesh {
                    out.push(ObjectRef::new(ObjectKind::Mesh, &node.id));
                }
                collect(&node.children, out);
            }
        }
        let mut meshes = Vec::new();
        collect(&tree, &mut meshes);
        meshes
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::SelectObjects(meshes.clone()))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        let from = window
            .find(format!("parts-drag-{}", meshes[0].id))
            .bounds()
            .center();
        let to = window
            .find(format!("parts-item-Part:{}", id(102)))
            .bounds()
            .center();
        window.drag(from, to, cx);
        window.render_frame(cx);
        let view = h.view.read(cx);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(view.editor.document().unwrap().context().selections, meshes);
        let tree = view.editor.part_tree();
        assert_eq!(
            tree[1]
                .children
                .iter()
                .filter(|node| node.kind == zipzi_editor_core::TreeNodeKind::Mesh)
                .count(),
            meshes.len()
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_locked_member_rejects_the_whole_tree_drag(cx: &mut TestAppContext) {
    use zipzi_editor_core::{EditorCommand, ObjectKind};
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let meshes: Vec<_> = document
        .object_index()
        .objects
        .iter()
        .filter(|info| info.reference.kind == ObjectKind::Mesh)
        .map(|info| info.reference.clone())
        .collect();
    document
        .set_objects_locked(std::slice::from_ref(&meshes[1]), true)
        .unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::SelectObjects(meshes.clone()))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        let tree = h.view.read(cx).editor.part_tree();
        let from = window
            .find(format!("parts-drag-{}", meshes[0].id))
            .bounds()
            .center();
        let to = window
            .find(format!("parts-item-Part:{}", id(102)))
            .bounds()
            .center();
        window.drag(from, to, cx);
        assert_eq!(h.view.read(cx).editor.part_tree(), tree);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .context()
                .selections,
            meshes
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_parameter_drag_settles_the_old_name_before_the_move(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameters(3).unwrap();
    let first = document.parameters()[0].id.clone();
    document.focus_parameter(&first).unwrap();
    let h = setup_with_document(cx, document, assets);
    super::properties::fill(&h, "property-input-name", "Committed before move", cx);
    cx.update_window(h.window, |_, window, cx| {
        let from = window.find(format!("parameter-name-{}", id(3))).bounds();
        let to = window.find(format!("parameter-name-{}", id(2))).bounds();
        window.drag(from.center(), point(to.center().x, to.top() + px(2.)), cx);
        window.render_frame(cx);
        let doc = h.view.read(cx).editor.document().unwrap();
        assert_eq!(
            doc.parameter_draft(&first).unwrap().name,
            "Committed before move"
        );
        assert_eq!(doc.parameters()[1].id, id(3));
        assert_eq!(doc.history_lengths(), (2, 0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn parameter_row_menu_omits_steps_and_preserves_move_to(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameters(3).unwrap();
    let inspected = document.parameters()[0].id.clone();
    document.focus_parameter(&inspected).unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            format!("parameter-name-{}", id(3)),
            gpui_kit::test::ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let labels: Vec<_> = (0..24usize)
            .filter_map(|i| {
                window
                    .within("popup-menu")
                    .try_find(i)
                    .and_then(|item| item.label().map(str::to_owned))
            })
            .collect();
        assert!(labels.iter().any(|label| label == "Move to…"));
        assert!(
            !labels
                .iter()
                .any(|label| label.starts_with("Move up") || label.starts_with("Move down"))
        );
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(document.parameters()[2].id, id(3));
        assert_eq!(document.context().inspected.as_ref().unwrap().id, inspected);
        assert_eq!(document.history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn hovering_the_same_destination_reuses_one_plan_across_input_and_render(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(h.window, size(px(1100.), px(1400.)));
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let from = window
            .find(format!("parameter-name-{}", id(3)))
            .bounds()
            .center();
        let to = window
            .find(format!("parameter-group-{}", id(104)))
            .bounds()
            .center();
        begin_drag(window, cx, from, to);
        let panel = h.view.read(cx).dock.as_ref().unwrap().parameters.clone();
        let plans = panel.read(cx).sorting_plans_built(cx);
        assert!(plans > 0);
        for _ in 0..10 {
            window.dispatch_event(
                MouseMoveEvent {
                    position: to,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
        }
        assert_eq!(panel.read(cx).sorting_plans_built(cx), plans);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
        window.press("escape", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn leaving_and_reentering_a_folder_restarts_the_hover_expand_delay(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(h.window, size(px(1100.), px(1400.)));
    let target = cx
        .update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(format!("parameter-group-toggle-{}", id(104)), cx);
            let from = window
                .find(format!("parameter-name-{}", id(3)))
                .bounds()
                .center();
            let to = window
                .find(format!("parameter-group-{}", id(104)))
                .bounds()
                .center();
            begin_drag(window, cx, from, to);
            to
        })
        .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let outside = window.find("canvas-body").bounds().center();
        super::slide_number_input::slide(window, outside, false, cx);
        window.render_frame(cx);
        super::slide_number_input::slide(window, target, false, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(104)))
                .expanded(),
            Some(false)
        );
    })
    .unwrap();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(104)))
                .expanded(),
            Some(true)
        );
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
        window.press("escape", cx);
    })
    .unwrap();
}
