use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::component::WindowExt;
use gpui_kit::{
    AppContext as _, Modifiers, MouseButton, TestAppContext,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_editor_core::{EditorCommand, ObjectKind, ObjectRef};

fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
fn settle(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
}
fn open(h: &Harness, row: String, cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(row, ClickOptions::new().with_button(MouseButton::Right), cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        let index = (0..24usize)
            .find(|index| {
                window
                    .within("popup-menu")
                    .try_find(*index)
                    .is_some_and(|item| item.label() == Some("Move to…"))
            })
            .expect("Move to menu");
        window.within("popup-menu").click(index, cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(window.find("move-destination-picker").visible());
        window.click("move-destination-submit", cx);
        assert!(window.has_active_dialog(cx));
    })
    .unwrap();
}
fn choose(h: &Harness, path: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.within("move-destination-select").click("input", cx);
        window.press("cmd-a", cx);
        window.input(path, cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "Selecting a target must not submit the dialog"
        )
    })
    .unwrap();
}

#[gpui_kit::test]
fn tree_move_to_submits_captured_multiselection_atomically_and_reveals_destination(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::sample_document_with_parts().unwrap();
    fn order(nodes: &[zipzi_editor_core::TreeNode], result: &mut Vec<String>) {
        for node in nodes {
            if node.kind == zipzi_editor_core::TreeNodeKind::Mesh {
                result.push(node.id.clone());
            }
            order(&node.children, result);
        }
    }
    let mut expected_order = Vec::new();
    order(&document.mesh_tree(), &mut expected_order);
    let meshes: Vec<_> = document
        .layers()
        .iter()
        .map(|layer| ObjectRef::new(ObjectKind::Mesh, &layer.id))
        .collect();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("mesh-item-Mesh:{}", meshes[0].id), cx);
        window.click_with_modifiers(
            format!("mesh-item-Mesh:{}", meshes[1].id),
            Modifiers {
                platform: true,
                ..Default::default()
            },
            cx,
        );
    })
    .unwrap();
    let inspected = h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().context().inspected.clone()
    });
    open(&h, format!("mesh-item-Mesh:{}", meshes[0].id), cx);
    choose(&h, "Body", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let tree = doc.mesh_tree();
        let body = tree.iter().find(|node| node.id == id(102)).unwrap();
        assert_eq!(
            body.children
                .iter()
                .map(|node| node.id.clone())
                .collect::<Vec<_>>(),
            expected_order
        );
        assert_eq!(doc.context().inspected, inspected);
        assert_eq!(doc.context().selections, meshes);
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    cx.update_window(h.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert!(
            window
                .find(format!("mesh-item-Mesh:{}", meshes[1].id))
                .visible()
        );
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Undo);
            view.complete_edit(result, cx);
            let result = view.editor.execute(EditorCommand::Redo);
            view.complete_edit(result, cx);
            assert_eq!(
                view.editor.document().unwrap().context().inspected,
                inspected
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn parameter_move_to_uses_clicked_row_and_reveals_empty_folder_without_changing_inspection(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    document.focus_parameter(&id(5)).unwrap();
    let h = setup_with_document(cx, document, assets);
    open(&h, format!("parameter-name-{}", id(1)), cx);
    choose(&h, "Expressions", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx)
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameter_draft(&id(1)).unwrap().group_id, Some(id(104)));
        assert_eq!(doc.context().inspected.as_ref().unwrap().id, id(5));
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    cx.update_window(h.window, |_, window, _| {
        assert!(window.find(format!("parameter-name-{}", id(1))).visible())
    })
    .unwrap();
}

#[gpui_kit::test]
fn cycle_and_locked_destination_remain_disabled_and_cancel_keeps_history(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    document
        .set_objects_locked(&[ObjectRef::new(ObjectKind::Part, id(102))], true)
        .unwrap();
    let h = setup_with_document(cx, document, assets);
    open(&h, format!("mesh-item-Part:{}", id(101)), cx);
    choose(&h, "Face / Eyes", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx);
        assert!(window.has_active_dialog(cx));
        assert!(
            window
                .find("move-destination-reason")
                .label()
                .unwrap()
                .contains("descendant")
        );
    })
    .unwrap();
    choose(&h, "Body", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx);
        assert!(window.has_active_dialog(cx));
        assert!(
            window
                .find("move-destination-reason")
                .label()
                .unwrap()
                .contains("Unlock")
        );
        window.click("move-destination-cancel", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn picker_rejects_changed_document_even_after_a_target_was_validated(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let h = setup_with_document(cx, document, assets);
    open(&h, format!("mesh-item-Part:{}", id(103)), cx);
    choose(&h, "Body", cx);
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SetObjectsLocked {
                objects: vec![ObjectRef::new(ObjectKind::Part, id(102))],
                locked: true,
            });
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx);
        assert!(window.has_active_dialog(cx));
        assert!(
            window
                .find("move-destination-reason")
                .label()
                .unwrap()
                .contains("context changed")
        );
        window.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.mesh_tree()[0].children[0].id, id(103));
        assert_eq!(doc.history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn folder_move_to_root_preserves_members_and_noop_keeps_history(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let h = setup_with_document(cx, document, assets);
    open(&h, format!("parameter-group-{}", id(102)), cx);
    choose(&h, "Root collection", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx)
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameter_group_draft(&id(102)).unwrap().parent_id, None);
        assert_eq!(doc.parameter_draft(&id(3)).unwrap().group_id, Some(id(102)));
        assert_eq!(doc.parameter_draft(&id(4)).unwrap().group_id, Some(id(102)));
    });
    open(&h, format!("parameter-group-{}", id(102)), cx);
    choose(&h, "Root collection", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx);
        assert!(window.has_active_dialog(cx));
        assert!(
            window
                .find("move-destination-reason")
                .label()
                .unwrap()
                .contains("Already")
        );
        window.click("move-destination-cancel", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn filtered_move_reveals_new_ancestor_path_and_keeps_query_and_inspection(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let object = document.layers()[0].id.clone();
    document
        .navigate(ObjectRef::new(ObjectKind::Part, id(101)))
        .unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-search", cx);
        window.input("Face", cx);
    })
    .unwrap();
    settle(cx);
    open(&h, format!("mesh-item-Mesh:{object}"), cx);
    choose(&h, "Body", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx)
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(window.find("mesh-tree-search").value(), Some("Face"));
        assert!(window.find(format!("mesh-item-Part:{}", id(102))).visible());
        assert!(window.find(format!("mesh-item-Mesh:{object}")).visible());
        assert!(window.find("tree-moved-item").visible());
        assert!(window.try_find("tree-created-item").is_none());
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .context()
                .inspected
                .as_ref()
                .unwrap()
                .id,
            id(101)
        )
    });
}

#[gpui_kit::test]
fn locked_member_rejects_the_entire_captured_selection(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let meshes: Vec<_> = document
        .layers()
        .iter()
        .map(|layer| ObjectRef::new(ObjectKind::Mesh, &layer.id))
        .collect();
    document
        .set_objects_locked(&[meshes[1].clone()], true)
        .unwrap();
    let before = document.mesh_tree();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("mesh-item-Mesh:{}", meshes[0].id), cx);
        window.click_with_modifiers(
            format!("mesh-item-Mesh:{}", meshes[1].id),
            Modifiers {
                platform: true,
                ..Default::default()
            },
            cx,
        );
    })
    .unwrap();
    open(&h, format!("mesh-item-Mesh:{}", meshes[0].id), cx);
    choose(&h, "Body", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx);
        assert!(window.has_active_dialog(cx));
        assert!(
            window
                .find("move-destination-reason")
                .label()
                .unwrap()
                .contains("Unlock")
        );
        window.click("move-destination-cancel", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.mesh_tree(), before);
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(doc.context().selections, meshes);
    });
}

#[gpui_kit::test]
fn cancelling_noop_picker_preserves_an_existing_redo_branch(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    document
        .create_parameter(zipzi_editor_core::NewParameter::default())
        .unwrap();
    document.undo().unwrap();
    assert_eq!(document.history_lengths(), (0, 1));
    let h = setup_with_document(cx, document, assets);
    open(&h, format!("parameter-group-{}", id(104)), cx);
    choose(&h, "Root collection", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx);
        assert!(window.has_active_dialog(cx));
        assert!(
            window
                .find("move-destination-reason")
                .label()
                .unwrap()
                .contains("Already")
        );
        window.click("move-destination-cancel", cx);
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 1))
    });
}

#[gpui_kit::test]
fn configurable_move_to_command_uses_the_focused_tree_row(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        cx.global::<crate::settings::Settings>()
            .store
            .write(
                "keybindings.json",
                &zipzi_editor_core::ShortcutOverrides {
                    bindings: vec![zipzi_editor_core::ShortcutOverride {
                        command: zipzi_editor_core::CommandId::PaletteMoveTo,
                        keys: vec!["alt-m".into()],
                        platform: None,
                    }],
                },
            )
            .unwrap();
        crate::keybindings::reload(cx);
        window.click(format!("mesh-item-Part:{}", id(103)), cx);
        window.press("alt-m", cx);
    })
    .unwrap();
    settle(cx);
    choose(&h, "Body", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-submit", cx)
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let tree = doc.mesh_tree();
        assert_eq!(
            tree.iter()
                .find(|node| node.id == id(102))
                .unwrap()
                .children[0]
                .id,
            id(103)
        );
        assert_eq!(doc.context().inspected.as_ref().unwrap().id, id(103));
        assert_eq!(doc.history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn offscreen_targets_explain_owner_requirements_and_keep_the_document_unchanged(
    cx: &mut TestAppContext,
) {
    let mut document = zipzi_editor_core::Document::new_blank().unwrap();
    use zipzi_editor_core::SceneObjectType as T;
    let mut create = |kind, name: &str, selection: &[ObjectRef]| {
        let plan = document
            .plan_scene_object_creation(kind, name.into(), selection)
            .unwrap();
        document
            .execute_scene_object_creation(&plan)
            .unwrap()
            .suggested_selection[0]
            .clone()
    };
    let a = create(T::Part, "A", &[]);
    let b = create(T::Part, "B", &[]);
    let offscreen = create(T::Offscreen, "Offscreen A", &[a]);
    create(T::Offscreen, "Offscreen B", &[b]);
    let before = document.mesh_tree();
    let h = setup_with_document(cx, document, tempfile::tempdir().unwrap());
    open(&h, format!("mesh-item-Offscreen:{}", offscreen.id), cx);
    for (path, reason) in [
        ("Root collection", "Choose a Part"),
        ("B", "already has an Offscreen"),
    ] {
        choose(&h, path, cx);
        cx.update_window(h.window, |_, window, cx| {
            window.click("move-destination-submit", cx);
            assert!(window.has_active_dialog(cx));
            let label = window
                .find("move-destination-reason")
                .label()
                .unwrap()
                .to_owned();
            assert!(label.contains(reason), "{label}");
        })
        .unwrap();
    }
    cx.update_window(h.window, |_, window, cx| {
        window.click("move-destination-cancel", cx)
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.mesh_tree(), before);
        assert_eq!(doc.history_lengths(), (4, 0));
    });
}
