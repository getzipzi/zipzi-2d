use super::{Harness, setup_with_document};
use gpui_kit::{
    AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, TestAppContext,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_editor_core::{Document, EditorCommand, ObjectRef, SceneObjectType as T};

fn fixture(cx: &mut TestAppContext) -> (Harness, ObjectRef, Vec<ObjectRef>) {
    let mut document = Document::new_blank().unwrap();
    let plan = document
        .plan_scene_object_creation(T::Part, "Container".into(), &[])
        .unwrap();
    let part = document
        .execute_scene_object_creation(&plan)
        .unwrap()
        .suggested_selection[0]
        .clone();
    let meshes = ["Match 0", "Hidden 1", "Match 2", "Hidden 3"]
        .into_iter()
        .map(|name| {
            let plan = document
                .plan_scene_object_creation(T::Mesh, name.into(), std::slice::from_ref(&part))
                .unwrap();
            document
                .execute_scene_object_creation(&plan)
                .unwrap()
                .suggested_selection[0]
                .clone()
        })
        .collect();
    (
        setup_with_document(cx, document, tempfile::tempdir().unwrap()),
        part,
        meshes,
    )
}
fn row(object: &ObjectRef) -> String {
    format!("parts-item-{:?}:{}", object.kind, object.id)
}
fn selected(h: &Harness, cx: &TestAppContext) -> Vec<ObjectRef> {
    h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().context().selections.clone()
    })
}
fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}
fn cmd() -> Modifiers {
    Modifiers {
        platform: true,
        ..Default::default()
    }
}

#[gpui_kit::test]
fn command_toggle_and_shift_ranges_keep_anchor_and_clicked_primary(cx: &mut TestAppContext) {
    let (h, _, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[0]), cx);
        window.click_with_modifiers(row(&m[2]), cmd(), cx);
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[0].clone(), m[2].clone()]);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_modifiers(row(&m[1]), shift(), cx)
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[2].clone(), m[1].clone()]);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_modifiers(row(&m[3]), shift(), cx)
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[2].clone(), m[3].clone()]);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_modifiers(
            row(&m[0]),
            Modifiers {
                shift: true,
                ..cmd()
            },
            cx,
        );
    })
    .unwrap();
    let selection = selected(&h, cx);
    assert_eq!(selection.len(), 4);
    assert_eq!(selection.last(), Some(&m[0]));
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_modifiers(row(&m[0]), cmd(), cx)
    })
    .unwrap();
    assert!(!selected(&h, cx).contains(&m[0]));
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (5, 0))
    });
}

#[gpui_kit::test]
fn filtered_range_uses_visible_rows_and_collapsing_keeps_hidden_selection(cx: &mut TestAppContext) {
    let (h, part, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[0]), cx);
        window.click("parts-tree-search", cx);
        window.input("Match", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_modifiers(row(&m[2]), shift(), cx)
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[0].clone(), m[2].clone()]);
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("tree-expand-Part:{}", part.id), cx);
        assert!(window.try_find(row(&m[0])).is_none());
        assert!(window.try_find(row(&m[2])).is_none());
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[0].clone(), m[2].clone()]);
}

#[gpui_kit::test]
fn state_menu_targets_selection_and_other_branches_through_search(cx: &mut TestAppContext) {
    let (h, part, meshes) = fixture(cx);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&meshes[0]), cx);
        window.click_with_modifiers(row(&meshes[2]), cmd(), cx);
        window.click("parts-tree-search", cx);
        window.input("Match", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let selection = selected(&h, cx);
    for (label, hidden, locked) in [
        ("Hide others", [false, true, false, true], [false; 4]),
        ("Toggle visibility", [true; 4], [false; 4]),
        ("Show others", [true, false, true, false], [false; 4]),
        (
            "Lock others",
            [true, false, true, false],
            [false, true, false, true],
        ),
        ("Toggle lock", [true, false, true, false], [true; 4]),
        (
            "Unlock others",
            [true, false, true, false],
            [true, false, true, false],
        ),
    ] {
        cx.update_window(h.window, |_, window, cx| {
            assert!(window.try_find(row(&meshes[1])).is_none());
            window.click_with_options(
                row(&meshes[0]),
                ClickOptions::new().with_button(MouseButton::Right),
                cx,
            );
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            let index = (0..6usize)
                .find(|index| window.within("popup-menu").find(*index).label() == Some(label))
                .expect(label);
            window.within("popup-menu").click(index, cx);
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert_eq!(doc.context().selections, selection);
            assert!(doc.object_state(&part).visible());
            assert!(!doc.object_state(&part).locked());
            for (index, mesh) in meshes.iter().enumerate() {
                let state = doc.object_state(mesh);
                assert_eq!(state.explicit.hidden_in_editor, hidden[index], "{label}");
                assert_eq!(state.explicit.locked, locked[index], "{label}");
            }
        });
    }
}

#[gpui_kit::test]
fn sweeping_state_columns_includes_skipped_rows_and_undoes_in_one_step(cx: &mut TestAppContext) {
    let (h, _, meshes) = fixture(cx);
    let selection = selected(&h, cx);
    let history = h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    for column in ["vis", "lock"] {
        cx.update_window(h.window, |_, window, cx| {
            window.hover(row(&meshes[0]), cx);
            let start = window
                .find(format!("parts-{column}-{}", meshes[0].id))
                .bounds()
                .center();
            let end = window
                .find(format!("parts-{column}-{}", meshes[2].id))
                .bounds()
                .center();
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            for position in [end, start] {
                window.dispatch_event(
                    MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
            }
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: start,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        h.view.update(cx, |view, cx| {
            let doc = view.editor.document().unwrap();
            assert_eq!(doc.history_lengths().0, history.0 + 1, "{column}");
            assert_eq!(doc.context().selections, selection);
            for (index, mesh) in meshes.iter().enumerate() {
                let state = doc.object_state(mesh).explicit;
                assert_eq!(state.hidden_in_editor, column == "vis" && index < 3);
                assert_eq!(state.locked, column == "lock" && index < 3);
            }
            view.editor.execute(EditorCommand::Undo).unwrap();
            let doc = view.editor.document().unwrap();
            assert!(meshes.iter().all(|mesh| {
                let state = doc.object_state(mesh).explicit;
                !state.hidden_in_editor && !state.locked
            }));
            cx.notify();
        });
    }
}

#[gpui_kit::test]
fn arrows_move_cursor_enter_selects_and_left_right_fold_without_losing_selection(
    cx: &mut TestAppContext,
) {
    let (h, part, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[0]), cx);
        window.press("down", cx);
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[0].clone()]);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    assert_eq!(selected(&h, cx), [m[1].clone()]);
    cx.update_window(h.window, |_, window, cx| {
        window.press("left", cx); // parent cursor
        window.press("left", cx); // collapse parent
        assert!(window.try_find(row(&m[1])).is_none());
        window.press("right", cx); // expand parent
        assert!(window.find(row(&m[1])).visible());
        window.press("right", cx); // first child cursor
        window.press("enter", cx);
        assert!(window.find(row(&part)).visible());
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[0].clone()]);
    cx.update_window(h.window, |_, window, cx| {
        window.press("shift-down", cx);
        window.press("shift-down", cx);
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[0].clone(), m[1].clone(), m[2].clone()]);
}

#[gpui_kit::test]
fn navigation_does_not_intercept_search_input_or_apply_an_old_document_anchor(
    cx: &mut TestAppContext,
) {
    let (h, _, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[0]), cx);
        window.click("parts-tree-search", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    assert_eq!(selected(&h, cx), [m[0].clone()]);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            view.refresh_document(window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert!(selected(&h, cx).is_empty());
}

#[gpui_kit::test]
fn keyboard_move_uses_cursor_without_activating_it_and_fold_preserves_selection(
    cx: &mut TestAppContext,
) {
    let (h, _, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[0]), cx);
        window.press("down", cx);
        window.press("alt-up", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.part_tree()[0].children[0].id, m[1].id);
        assert_eq!(doc.context().selection.as_ref(), Some(&m[0]));
        assert_eq!(doc.history_lengths(), (6, 0));
    });
    cx.update_window(h.window, |_, window, cx| {
        // Cursor is still the moved row. Its parent can fold while selection
        // remains on a different, now hidden child.
        window.press("left", cx);
        window.press("left", cx);
        assert!(window.try_find(row(&m[0])).is_none());
        window.press("alt-up", cx); // only root Part; no-op
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (6, 0))
    });
}
