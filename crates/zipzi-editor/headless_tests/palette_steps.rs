use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::{
    AppContext as _, Modifiers, MouseButton, TestAppContext,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_editor_core::{Document, EditorCommand, ObjectRef, SceneObjectType as T};

fn fixture(cx: &mut TestAppContext) -> (Harness, ObjectRef, Vec<ObjectRef>) {
    let mut d = Document::new_blank().unwrap();
    let plan = d
        .plan_scene_object_creation(T::Part, "Part".into(), &[])
        .unwrap();
    let part = d
        .execute_scene_object_creation(&plan)
        .unwrap()
        .suggested_selection[0]
        .clone();
    let objects = ["Hidden 0", "Keep 1", "Hidden 2", "Keep 3", "Hidden 4"]
        .into_iter()
        .map(|name| {
            let plan = d
                .plan_scene_object_creation(T::Mesh, name.into(), std::slice::from_ref(&part))
                .unwrap();
            d.execute_scene_object_creation(&plan)
                .unwrap()
                .suggested_selection[0]
                .clone()
        })
        .collect();
    (
        setup_with_document(cx, d, tempfile::tempdir().unwrap()),
        part,
        objects,
    )
}
fn row(object: &ObjectRef) -> String {
    format!("mesh-item-{:?}:{}", object.kind, object.id)
}
fn order(h: &Harness, cx: &TestAppContext) -> Vec<String> {
    h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().mesh_tree()[0]
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect()
    })
}
fn cmd() -> Modifiers {
    Modifiers {
        platform: true,
        ..Default::default()
    }
}
fn choose(h: &Harness, label: &str, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let index = (0..24usize)
            .find(|index| {
                window
                    .within("popup-menu")
                    .try_find(*index)
                    .is_some_and(|item| item.label() == Some(label))
            })
            .expect(label);
        window.within("popup-menu").click(index, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn disjoint_selected_rows_move_through_hidden_neighbors_with_one_history_entry(
    cx: &mut TestAppContext,
) {
    let (h, _, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-search", cx);
        window.input("Keep", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[1]), cx);
        window.click_with_modifiers(row(&m[3]), cmd(), cx);
        window.press("alt-up", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(order(&h, cx), [1, 0, 3, 2, 4].map(|i| m[i].id.clone()));
    h.view.read_with(cx, |view, _| {
        let d = view.editor.document().unwrap();
        assert_eq!(d.context().selections, [m[1].clone(), m[3].clone()]);
        assert_eq!(d.context().inspected.as_ref(), Some(&m[3]));
        assert_eq!(d.history_lengths(), (7, 0));
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(window.find("mesh-tree-search").value(), Some("Keep"));
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Undo);
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    assert_eq!(
        order(&h, cx),
        m.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
    );
}

#[gpui_kit::test]
fn menu_and_keyboard_share_disjoint_step_semantics_and_repeated_boundary_steps_are_noops(
    cx: &mut TestAppContext,
) {
    let (h, _, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[1]), cx);
        window.click_with_modifiers(row(&m[3]), cmd(), cx);
        window.click_with_options(
            row(&m[1]),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    choose(&h, "Move down", cx);
    assert_eq!(order(&h, cx), [0, 2, 1, 4, 3].map(|i| m[i].id.clone()));
    cx.update_window(h.window, |_, window, cx| {
        window.press("alt-down", cx);
        window.press("alt-down", cx);
        window.press("alt-down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(order(&h, cx), [0, 2, 4, 1, 3].map(|i| m[i].id.clone()));
    h.view.read_with(cx, |view, _| {
        let d = view.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (8, 0));
        assert_eq!(d.context().selections, [m[1].clone(), m[3].clone()]);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
}

#[gpui_kit::test]
fn menu_freezes_the_clicked_selection_without_retargeting_later_inspection(
    cx: &mut TestAppContext,
) {
    let (h, _, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&m[1]), cx);
        window.click_with_modifiers(row(&m[3]), cmd(), cx);
        window.click_with_options(
            row(&m[1]),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view
                .editor
                .execute(EditorCommand::SelectObject(m[4].clone()));
            view.complete_edit(result, cx);
        })
    })
    .unwrap();
    choose(&h, "Move up", cx);
    assert_eq!(order(&h, cx), [1, 0, 3, 2, 4].map(|i| m[i].id.clone()));
    h.view.read_with(cx, |view, _| {
        let d = view.editor.document().unwrap();
        assert_eq!(d.context().inspected.as_ref(), Some(&m[4]));
        assert_eq!(d.context().selections, [m[4].clone()]);
        assert_eq!(d.history_lengths(), (7, 0));
    });
}

#[gpui_kit::test]
fn locked_boundary_member_prevents_other_selected_blocks_from_moving(cx: &mut TestAppContext) {
    let (h, _, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SetObjectsLocked {
                objects: vec![m[0].clone()],
                locked: true,
            });
            view.complete_edit(result, cx);
        });
        window.click(row(&m[0]), cx);
        window.click_with_modifiers(row(&m[3]), cmd(), cx);
        window.press("alt-up", cx);
    })
    .unwrap();
    assert_eq!(
        order(&h, cx),
        m.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
    );
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (7, 0));
        assert!(
            view.edit_error
                .as_ref()
                .is_some_and(|error| error.contains("Unlock"))
        );
    });
}

#[gpui_kit::test]
fn selecting_parent_and_child_never_steps_the_child_a_second_time(cx: &mut TestAppContext) {
    let (h, part, m) = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(row(&part), cx);
        window.click_with_modifiers(row(&m[3]), cmd(), cx);
        window.press("alt-up", cx);
        window.press("alt-down", cx);
    })
    .unwrap();
    assert_eq!(
        order(&h, cx),
        m.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
    );
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (6, 0));
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
}

#[gpui_kit::test]
fn deformation_parent_mismatch_rejects_the_batch_with_localized_reason(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.dock.as_ref().unwrap().show(
                zipzi_editor_core::PanelKind::Deformers,
                None,
                window,
                cx,
            )
        });
        window.render_frame(cx);
        window.click(
            format!(
                "deformer-item-RotationDeformer:{}",
                uuid::Uuid::from_u128(501)
            ),
            cx,
        );
        window.click_with_modifiers(
            format!("deformer-item-WarpDeformer:{}", uuid::Uuid::from_u128(502)),
            cmd(),
            cx,
        );
        window.press("alt-up", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(
            view.edit_error
                .as_ref()
                .is_some_and(|error| error.contains("same deformation parent"))
        );
    });
}
