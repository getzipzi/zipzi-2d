use super::{Harness, document_fixture, setup_with_document};
use editor_core::{CommandId, Document, EditorCommand, ObjectKind, ObjectRef};
use gpui_kit::{
    AppContext as _, MouseButton, TestAppContext,
    test::{ClickOptions, TestWindowExt},
};

fn choose(h: &Harness, label: &str, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let index = (0..24usize)
            .find(|index| {
                window
                    .within("popup-menu")
                    .try_find(*index)
                    .is_some_and(|item| item.label() == Some(label))
            })
            .expect(label);
        assert_ne!(
            window.within("popup-menu").find(index).disabled(),
            Some(true)
        );
        window.within("popup-menu").click(index, cx);
    })
    .unwrap();
    cx.run_until_parked();
}
fn inspected(h: &Harness, cx: &TestAppContext) -> ObjectRef {
    h.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .context()
            .inspected
            .clone()
            .unwrap()
    })
}
fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}

#[gpui_kit::test]
fn toolbar_creation_focuses_name_and_rename_has_separate_history(cx: &mut TestAppContext) {
    let h = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-create", cx)
    })
    .unwrap();
    choose(&h, "New Part", cx);
    let part = inspected(&h, cx);
    assert_eq!(part.kind, ObjectKind::Part);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(format!("mesh-item-Part:{}", part.id)).visible());
        assert!(window.find("property-input-name").visible());
        window.input("Character", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.object_index().name(&part), "Character");
        assert_eq!(doc.history_lengths(), (2, 0));
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.run_command(CommandId::Undo, window, cx);
            assert_eq!(
                view.editor.document().unwrap().object_index().name(&part),
                "Part"
            );
            view.run_command(CommandId::Undo, window, cx);
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .object_index()
                    .get(&part)
                    .is_none()
            );
            view.run_command(CommandId::Redo, window, cx);
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .object_index()
                    .get(&part)
                    .is_some()
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn row_creation_uses_clicked_container_instead_of_current_selection(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    document
        .navigate(ObjectRef::new(ObjectKind::Part, id(101)))
        .unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            format!("mesh-item-Part:{}", id(102)),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    choose(&h, "New Mesh", cx);
    let object = inspected(&h, cx);
    assert_eq!(object.kind, ObjectKind::Mesh);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let tree = doc.mesh_tree();
        let body = tree.iter().find(|node| node.id == id(102)).unwrap();
        assert_eq!(body.children[0].id, object.id);
        assert_eq!(doc.history_lengths(), (1, 0));
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(format!("mesh-item-Mesh:{}", object.id))
                .visible()
        );
        assert!(window.find("property-input-name").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn filtered_creation_reveals_new_item_and_ancestors_without_clearing_conditions(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    document
        .navigate(ObjectRef::new(ObjectKind::Part, id(103)))
        .unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-search", cx);
        window.input("no existing match", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-filter", cx);
        window.click("tree-kind-Mesh", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.find("mesh-tree-empty").visible());
        window.click("mesh-tree-create", cx);
    })
    .unwrap();
    choose(&h, "New Mesh", cx);
    let object = inspected(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("mesh-tree-search").value(),
            Some("no existing match")
        );
        assert!(window.find(format!("mesh-item-Part:{}", id(101))).visible());
        assert!(window.find(format!("mesh-item-Part:{}", id(103))).visible());
        assert!(
            window
                .find(format!("mesh-item-Mesh:{}", object.id))
                .visible()
        );
        assert!(window.find("tree-created-item").visible());
        window.click("mesh-tree-filter", cx);
        assert_eq!(window.find("tree-kind-Mesh").checked(), Some(false));
        window.press("escape", cx);
        window.click(format!("mesh-item-Part:{}", id(103)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.try_find("tree-created-item").is_none());
        assert!(window.find("mesh-tree-empty").visible());
        assert_eq!(
            window.find("mesh-tree-search").value(),
            Some("no existing match")
        );
        window.click("mesh-tree-clear", cx);
        assert!(
            window
                .find(format!("mesh-item-Mesh:{}", object.id))
                .visible()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn locked_container_creation_is_disabled_with_reason_and_no_publication(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    document
        .set_objects_locked(&[ObjectRef::new(ObjectKind::Part, id(101))], true)
        .unwrap();
    let history = document.history_lengths();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            format!("mesh-item-Part:{}", id(103)),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let index = (0..24usize)
            .find(|index| {
                window
                    .within("popup-menu")
                    .try_find(*index)
                    .is_some_and(|item| {
                        item.label()
                            .is_some_and(|label| label.starts_with("New Mesh · Unlock"))
                    })
            })
            .expect("locked creation reason");

        window.within("popup-menu").click(index, cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), history)
    });
}

#[gpui_kit::test]
fn open_creation_menu_cannot_execute_after_document_version_changes(cx: &mut TestAppContext) {
    let h = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-create", cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::CreateParameter(
                editor_core::NewParameter::default(),
            ));
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    choose(&h, "New Part", cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert!(
            doc.mesh_tree()
                .iter()
                .all(|node| node.kind != editor_core::TreeNodeKind::Part)
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn root_menu_explains_offscreen_owner_and_toolbar_dispatches_each_scene_type(
    cx: &mut TestAppContext,
) {
    let h = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            "mesh-tree-root-drop",
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let index = (0..8usize)
            .find(|index| {
                window
                    .within("popup-menu")
                    .try_find(*index)
                    .is_some_and(|item| {
                        item.label()
                            .is_some_and(|label| label.starts_with("New Offscreen · Select a Part"))
                    })
            })
            .expect("owner reason");
        window.within("popup-menu").click(index, cx);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
    choose(&h, "New Part", cx);
    let part = inspected(&h, cx);
    use editor_core::TreeNodeKind as K;
    for (offset, (label, kind)) in [
        ("New Mesh", K::Mesh),
        ("New Warp", K::WarpDeformer),
        ("New Rotation", K::RotationDeformer),
        ("New Offscreen", K::Offscreen),
    ]
    .into_iter()
    .enumerate()
    {
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                let result = view.editor.execute(EditorCommand::Navigate(part.clone()));
                view.complete_edit(result, cx);
            });
            window.click("mesh-tree-create", cx);
        })
        .unwrap();
        choose(&h, label, cx);
        let created = inspected(&h, cx);
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            let tree = doc.mesh_tree();
            let owner = tree.iter().find(|node| node.id == part.id).unwrap();
            assert!(
                owner
                    .children
                    .iter()
                    .any(|node| node.id == created.id && node.kind == kind)
            );
            assert_eq!(doc.history_lengths(), (offset + 2, 0));
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
    }
}
