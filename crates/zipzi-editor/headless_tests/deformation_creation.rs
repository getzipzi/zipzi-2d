use super::{Harness, setup_with_document};
use gpui_kit::{
    AppContext as _, MouseButton, TestAppContext,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_editor_core::{
    CommandId as C, CommandSource, Document, EditorCommand, ObjectRef, PaletteProjection as P,
    SceneObjectType as T,
};

fn create(d: &mut Document, kind: T, selection: &[ObjectRef]) -> ObjectRef {
    let plan = d
        .plan_scene_object_creation(kind, format!("{kind:?}"), selection)
        .unwrap();
    d.execute_scene_object_creation(&plan)
        .unwrap()
        .suggested_selection[0]
        .clone()
}
fn fixture(cx: &mut TestAppContext, lock: bool) -> (Harness, ObjectRef, ObjectRef, Vec<ObjectRef>) {
    let mut d = Document::new_blank().unwrap();
    let p = create(&mut d, T::Part, &[]);
    let q = create(&mut d, T::Part, &[]);
    let parent = create(&mut d, T::Warp, std::slice::from_ref(&p));
    let meshes = ["Visible mesh", "Hidden sibling"]
        .into_iter()
        .map(|name| {
            let plan = d
                .plan_scene_authoring(zipzi_sdk::SceneEdit::Create {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: name.into(),
                    object: zipzi_sdk::SceneCreate::Mesh {
                        texture: Default::default(),
                    },
                    part_id: Some(q.id.clone()),
                    parent_id: Some(parent.id.clone()),
                    before: None,
                    deformation_before: None,
                })
                .unwrap();
            d.execute_scene_authoring(&plan)
                .unwrap()
                .suggested_selection[0]
                .clone()
        })
        .collect();
    d.select_object(parent.clone()).unwrap();
    if lock {
        d.set_objects_locked(std::slice::from_ref(&parent), true)
            .unwrap();
    }
    let h = setup_with_document(cx, d, tempfile::tempdir().unwrap());
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.dock.as_ref().unwrap().show(
                zipzi_editor_core::PanelKind::Deformers,
                None,
                window,
                cx,
            )
        });
    })
    .unwrap();
    (h, parent, q, meshes)
}
fn choose(h: &Harness, label: &str, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let i = (0..24usize)
            .find(|&i| {
                window
                    .within("popup-menu")
                    .try_find(i)
                    .is_some_and(|e| e.label() == Some(label))
            })
            .expect(label);
        window.within("popup-menu").click(i, cx);
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
fn children(h: &Harness, id: &str, cx: &TestAppContext) -> Vec<String> {
    fn find(nodes: &[zipzi_editor_core::TreeNode], id: &str) -> Option<Vec<String>> {
        nodes.iter().find_map(|n| {
            if n.id == id {
                Some(n.children.iter().map(|n| n.id.clone()).collect())
            } else {
                find(&n.children, id)
            }
        })
    }
    h.view.read_with(cx, |view, _| {
        find(&view.editor.document().unwrap().deformer_tree(), id).unwrap()
    })
}

#[gpui_kit::test]
fn toolbar_exposes_only_supported_types_creates_inside_and_focuses_name(cx: &mut TestAppContext) {
    let (h, parent, _, _) = fixture(cx, false);
    cx.update_window(h.window, |_, window, cx| {
        window.click("deformer-tree-create", cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, _| {
        let labels: Vec<_> = (0..12usize)
            .filter_map(|i| {
                window
                    .within("popup-menu")
                    .try_find(i)
                    .and_then(|e| e.label().map(str::to_owned))
            })
            .collect();
        assert_eq!(labels, ["New Mesh", "New Warp", "New Rotation"]);
    })
    .unwrap();
    choose(&h, "New Rotation", cx);
    let new = inspected(&h, cx);
    assert!(children(&h, &parent.id, cx).contains(&new.id));
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            window
                .find(format!("deformer-item-RotationDeformer:{}", new.id))
                .visible()
        );
        window.input("Nested rotation", cx);
        window.press("enter", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().object_index().name(&new),
            "Nested rotation"
        )
    });
}

#[gpui_kit::test]
fn row_context_creates_after_clicked_mesh_and_keeps_its_separate_part_owner(
    cx: &mut TestAppContext,
) {
    let (h, parent, part, meshes) = fixture(cx, false);
    let history = h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            format!("deformer-item-Mesh:{}", meshes[0].id),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    choose(&h, "New Mesh", cx);
    let new = inspected(&h, cx);
    assert_eq!(
        children(&h, &parent.id, cx),
        [meshes[0].id.clone(), new.id.clone(), meshes[1].id.clone()]
    );
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.object_index().get(&new).unwrap().properties["part_id"],
            part.id
        );
        assert_eq!(doc.history_lengths(), (history.0 + 1, 0));
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.run_command(C::Undo, window, cx);
        })
    })
    .unwrap();
    assert_eq!(
        children(&h, &parent.id, cx),
        [meshes[0].id.clone(), meshes[1].id.clone()]
    );
    cx.update_window(h.window, |_, window, cx| {
        h.view
            .update(cx, |view, cx| view.run_command(C::Redo, window, cx))
    })
    .unwrap();
    assert!(children(&h, &parent.id, cx).contains(&new.id));
}

#[gpui_kit::test]
fn filtered_creation_uses_hidden_sibling_and_reveals_without_changing_query(
    cx: &mut TestAppContext,
) {
    let (h, parent, _, meshes) = fixture(cx, false);
    cx.update_window(h.window, |_, window, cx| {
        window.click("deformer-tree-search", cx);
        window.input("Visible", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            window
                .try_find(format!("deformer-item-Mesh:{}", meshes[1].id))
                .is_none()
        );
        window.click_with_options(
            format!("deformer-item-Mesh:{}", meshes[0].id),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    choose(&h, "New Warp", cx);
    let new = inspected(&h, cx);
    assert_eq!(
        children(&h, &parent.id, cx),
        [meshes[0].id.clone(), new.id.clone(), meshes[1].id.clone()]
    );
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(window.find("deformer-tree-search").value(), Some("Visible"));
        assert!(
            window
                .find(format!("deformer-item-WarpDeformer:{}", new.id))
                .visible()
        );
        assert!(window.find("tree-created-item").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn menu_feedback_explains_locked_destination_and_stale_menu_cannot_create(cx: &mut TestAppContext) {
    let (h, _, _, _) = fixture(cx, true);
    let history = h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("deformer-tree-create", cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let i = (0..8usize)
            .find(|&i| {
                window
                    .within("popup-menu")
                    .try_find(i)
                    .is_some_and(|e| e.label().is_some_and(|l| l == "New Mesh"))
            })
            .unwrap();
        window.within("popup-menu").click(i, cx);
        window.press("escape", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), history)
    });
    let (h, parent, _, _) = fixture(cx, false);
    cx.update_window(h.window, |_, window, cx| {
        window.click("deformer-tree-create", cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::CreateParameter(
                zipzi_editor_core::NewParameter::default(),
            ));
            view.complete_edit(result, cx);
        })
    })
    .unwrap();
    choose(&h, "New Mesh", cx);
    assert_eq!(children(&h, &parent.id, cx).len(), 2);
}

#[gpui_kit::test]
fn explicit_deformation_commands_keep_projection_for_each_source_and_root_menu_works(
    cx: &mut TestAppContext,
) {
    let (h, parent, _, _) = fixture(cx, false);
    for (command, source) in [
        (C::CreateDeformationMesh, CommandSource::Direct),
        (C::CreateDeformationWarp, CommandSource::NativeMenu),
        (C::CreateDeformationRotation, CommandSource::Shortcut),
    ] {
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                let result = view
                    .editor
                    .execute(EditorCommand::SelectObject(parent.clone()));
                view.complete_edit(result, cx);
                view.run_invocation(view.editor.command(command).with_source(source), window, cx);
            })
        })
        .unwrap();
        cx.run_until_parked();
        let new = inspected(&h, cx);
        assert!(children(&h, &parent.id, cx).contains(&new.id));
        assert_eq!(command.scene_creation_projection(), Some(P::Deformation));
    }
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            "deformer-tree-root-drop",
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        )
    })
    .unwrap();
    choose(&h, "New Mesh", cx);
    let root = inspected(&h, cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.object_index().get(&root).unwrap().properties["deformer_id"],
            ""
        );
        assert_eq!(
            doc.object_index().get(&root).unwrap().properties["part_id"],
            ""
        );
    });
}

#[gpui_kit::test]
fn configured_deformation_shortcut_uses_frozen_selection_and_ignores_search_focus(
    cx: &mut TestAppContext,
) {
    let (h, parent, _, meshes) = fixture(cx, false);
    cx.update_window(h.window, |_, window, cx| {
        cx.global::<crate::settings::Settings>()
            .store
            .write(
                "keybindings.json",
                &zipzi_editor_core::ShortcutOverrides {
                    bindings: vec![zipzi_editor_core::ShortcutOverride {
                        command: C::CreateDeformationMesh,
                        keys: vec!["ctrl-alt-d".into()],
                        platform: None,
                    }],
                },
            )
            .unwrap();
        crate::keybindings::reload(cx);
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SelectObjects(vec![
                meshes[1].clone(),
                meshes[0].clone(),
            ]));
            view.complete_edit(result, cx);
        });
        window.click("deformer-tree-search", cx);
        window.press("ctrl-alt-d", cx);
    })
    .unwrap();
    assert_eq!(children(&h, &parent.id, cx).len(), 2);
    cx.update_window(h.window, |_, window, cx| {
        window.focus(&h.view.read(cx).focus.clone(), cx);
        window.press("ctrl-alt-d", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let new = inspected(&h, cx);
    assert_eq!(
        children(&h, &parent.id, cx),
        [meshes[0].id.clone(), meshes[1].id.clone(), new.id]
    );
}

#[gpui_kit::test]
fn native_deformation_submenu_dispatches_the_typed_projection_and_explains_invalid_context(
    cx: &mut TestAppContext,
) {
    use gpui_kit::OwnedMenuItem as M;
    let (h, parent, part, _) = fixture(cx, false);
    let action = cx
        .update_window(h.window, |_, window, cx| {
            h.view
                .update(cx, |view, cx| view.refresh_menus(false, window, cx));
            let menus = cx.get_menus().unwrap();
            let edit = menus.iter().find(|m| m.name == "Edit").unwrap();
            let create = edit
                .items
                .iter()
                .find_map(|m| match m {
                    M::Submenu(m) if m.name == "Create" => Some(m),
                    _ => None,
                })
                .unwrap();
            let deform = create
                .items
                .iter()
                .find_map(|m| match m {
                    M::Submenu(m) if m.name == "In Deformation Tree" => Some(m),
                    _ => None,
                })
                .unwrap();
            assert_eq!(deform.items.len(), 3);
            deform
                .items
                .iter()
                .find_map(|m| match m {
                    M::Action {
                        action, disabled, ..
                    } => {
                        let action = action
                            .as_any()
                            .downcast_ref::<crate::actions::RunCommand>()?;
                        if action.id != C::CreateDeformationMesh {
                            return None;
                        }
                        assert!(!disabled);
                        assert_eq!(action.invocation.unwrap().source, CommandSource::NativeMenu);
                        Some(action.clone())
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    cx.update_window(h.window, |_, window, cx| {
        window.dispatch_action(Box::new(action), cx)
    })
    .unwrap();
    cx.run_until_parked();
    assert!(children(&h, &parent.id, cx).contains(&inspected(&h, cx).id));
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SelectObject(part));
            view.complete_edit(result, cx);
            view.refresh_menus(false, window, cx);
            let reason = view.menu_state.creation_reasons[5].as_ref().unwrap();
            assert!(reason.contains("Choose a mesh or deformer"));
            assert!(
                !view
                    .editor
                    .command_available(view.editor.command(C::CreateDeformationMesh))
            );
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn deformer_tree_deletion_reviews_and_deletes_deformation_descendants(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (h, parent, part, meshes) = fixture(cx, false);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click_with_options(
            format!("deformer-item-WarpDeformer:{}", parent.id),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    choose(&h, "Delete Object", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("tree-deletion-dialog").visible());
        window.click("tree-deletion-submit", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let d = view.editor.document().unwrap();
        assert!(d.object_index().get(&part).is_some());
        assert!(d.object_index().get(&parent).is_none());
        for mesh in &meshes {
            assert!(d.object_index().get(mesh).is_none());
        }
    });
}

#[gpui_kit::test]
fn menu_feedback_explains_deletion_locks_and_rejects_changed_document(cx: &mut TestAppContext) {
    let (h, parent, _, meshes) = fixture(cx, true);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click_with_options(
            format!("deformer-item-WarpDeformer:{}", parent.id),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    choose(&h, "Delete Object", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("tree-deletion-dialog").visible());
        let history = h.view.read(cx).editor.document().unwrap().history_lengths();
        window.click("tree-deletion-submit", cx);
        window.render_frame(cx);
        assert!(window.find("tree-deletion-dialog").visible());
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            history
        );
        assert!(h.view.read(cx).edit_error.is_none());
        assert!(
            h.view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .object_index()
                .get(&parent)
                .is_some()
        );
        window.click("tree-deletion-cancel", cx);
        h.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::SetObjectsLocked {
                    objects: vec![parent.clone()],
                    locked: false,
                })
                .unwrap();
            view.redraw(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click_with_options(
            format!("deformer-item-WarpDeformer:{}", parent.id),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, _| {
            view.editor
                .execute(EditorCommand::SetObjectsLocked {
                    objects: vec![parent.clone()],
                    locked: true,
                })
                .unwrap();
        });
    })
    .unwrap();
    choose(&h, "Delete Object", cx);
    h.view.read_with(cx, |view, _| {
        let d = view.editor.document().unwrap();
        assert!(d.object_index().get(&parent).is_some());
        for mesh in &meshes {
            assert!(d.object_index().get(mesh).is_some());
        }
    });
}

#[gpui_kit::test]
fn object_deletion_deformer_shortcut_and_edit_menu_use_deformation_scope(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    for shortcut in [true, false] {
        let (h, parent, part, meshes) = fixture(cx, false);
        cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(format!("deformer-item-WarpDeformer:{}", parent.id), cx);
            window.hover("canvas-body", cx);
            if shortcut {
                window.press("backspace", cx);
            } else {
                h.view.update(cx, |view, cx| {
                    let command = view.object_deletion_invocation(
                        zipzi_editor_core::CommandSource::NativeMenu,
                        window,
                        cx,
                    );
                    assert_eq!(command.deletion_projection(), P::Deformation);
                    view.run_invocation(command, window, cx);
                });
            }
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("tree-deletion-dialog").visible());
            window.click("tree-deletion-submit", cx);
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |view, _| {
            let d = view.editor.document().unwrap();
            assert!(d.object_index().get(&part).is_some());
            assert!(d.object_index().get(&parent).is_none());
            for mesh in &meshes {
                assert!(d.object_index().get(mesh).is_none());
            }
        });
    }
}
