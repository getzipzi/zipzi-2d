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
fn empty_ungrouped_bucket_receives_parameters(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    document
        .move_parameter(
            &id(5),
            editor_core::ParameterDestination::Group(Some(id(104))),
        )
        .unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("parameter-group-count-ungrouped").value(),
            Some("0")
        );
        let from = window
            .find(format!("parameter-name-{}", id(3)))
            .bounds()
            .center();
        let to = window.find("parameter-group-ungrouped").bounds().center();
        window.drag(from, to, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("parameter-group-count-ungrouped").value(),
            Some("1")
        );
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
fn filtered_tree_and_document_switch_cannot_commit_a_drag(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (first, second, original, active) = harness.view.read_with(cx, |view, _| {
        let tree = view.editor.mesh_tree();
        (
            tree[0].children[0].id.clone(),
            tree[0].children[1].id.clone(),
            tree,
            view.editor.active_id().unwrap(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-tree-filter", cx);
        window.click("tree-kind-Part", cx);
        window.click("mesh-tree-filter", cx);
        let from = window.find(format!("mesh-item-Mesh:{second}")).bounds();
        let to = window.find(format!("mesh-item-Mesh:{first}")).bounds();
        window.drag(
            point(from.left() + px(80.), from.center().y),
            point(to.left() + px(80.), to.top() + px(2.)),
            cx,
        );
        assert_eq!(harness.view.read(cx).editor.mesh_tree(), original);
        window.click("mesh-tree-clear", cx);
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
        assert_eq!(view.editor.mesh_tree(), original);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
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
        let bounds = window.find("parameters-panel").bounds();
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
        .read_with(cx, |view, _| view.editor.mesh_tree());
    let first = original[0].children[0].id.clone();
    let second = original[0].children[1].id.clone();
    cx.update_window(harness.window, |_, window, cx| {
        let from = window.find(format!("mesh-item-Mesh:{second}")).bounds();
        let to = window.find(format!("mesh-item-Mesh:{first}")).bounds();
        window.drag(
            point(from.left() + px(80.), from.center().y),
            point(to.left() + px(80.), to.top() + px(3.)),
            cx,
        );
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert_eq!(view.editor.mesh_tree()[0].children[0].id, second);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            assert_eq!(view.editor.deformer_tree()[0].children[0].id, first);
        });
        harness.view.update(cx, |view, cx| {
            let result = view.editor.execute(editor_core::EditorCommand::Undo);
            view.complete_edit(result, cx);
        });
        window.render_frame(cx);
        assert!(
            window
                .find(format!("mesh-item-Mesh:{first}"))
                .bounds()
                .top()
                < window
                    .find(format!("mesh-item-Mesh:{second}"))
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
            view.editor.mesh_tree(),
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
        assert_eq!(harness.view.read(cx).editor.mesh_tree(), parts);
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
        assert_eq!(
            window
                .find(format!("parameter-group-count-{}", id(104)))
                .value(),
            Some("1")
        );
        assert_eq!(
            window
                .find(format!("parameter-group-count-{}", id(102)))
                .value(),
            Some("1")
        );
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
            .find(format!("mesh-drag-{}", id(103)))
            .bounds()
            .center();
        let to = window
            .find(format!("mesh-item-Part:{}", id(102)))
            .bounds()
            .center();
        window.drag(from, to, cx);
        window.render_frame(cx);
        let mesh = harness.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            let tree = view.editor.mesh_tree();
            assert!(tree[0].children.iter().all(|node| node.id != id(103)));
            assert_eq!(tree[1].children[0].id, id(103));
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            tree[1].children[0].children[0].id.clone()
        });
        let from = window.find(format!("mesh-drag-{mesh}")).bounds().center();
        let to = window.find("mesh-tree-root-drop").bounds().center();
        window.drag(from, to, cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .mesh_tree()
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
            .within("mesh-tree-panel")
            .click(format!("tree-expand-Part:{}", id(101)), cx);
        let from = window
            .find(format!("mesh-drag-{}", id(102)))
            .bounds()
            .center();
        let to = window
            .find(format!("mesh-item-Part:{}", id(101)))
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
                .try_find(format!("mesh-item-Part:{}", id(103)))
                .is_some()
        );
        window.press("escape", cx);
        let from = window
            .find(format!("mesh-drag-{}", id(101)))
            .bounds()
            .center();
        let to = window
            .find(format!("mesh-item-Part:{}", id(103)))
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
        let tree = harness.view.read(cx).editor.mesh_tree();
        let mesh = &tree[0].children[0].id;
        let from = window.find(format!("mesh-vis-{mesh}")).bounds().center();
        let to = window.find("mesh-tree-root-drop").bounds().center();
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
