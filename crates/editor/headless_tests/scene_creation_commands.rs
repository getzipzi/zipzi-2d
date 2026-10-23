use super::{Harness, properties::fill, setup_with_document};
use crate::actions::RunCommand;
use editor_core::{
    CommandId as C, CommandSource, Document, EditorCommand, ObjectKind as K, ObjectRef,
    ShortcutOverride, ShortcutOverrides,
};
use gpui_kit::{
    AppContext as _, InputEvent as _, KeyDownEvent, Keystroke, OwnedMenuItem, TestAppContext,
    test::TestWindowExt,
};

fn blank(cx: &mut TestAppContext) -> Harness {
    setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    )
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
fn invoke(h: &Harness, id: C, source: CommandSource, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let command = view.editor.command(id).with_source(source);
            view.run_invocation(command, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
}
fn select(h: &Harness, object: ObjectRef, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SelectObject(object));
            view.complete_edit(result, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
}
fn native_action(h: &Harness, id: C, cx: &mut TestAppContext) -> (RunCommand, String, bool) {
    cx.update_window(h.window, |_, window, cx| {
        h.view
            .update(cx, |view, cx| view.refresh_menus(false, window, cx));
        let menus = cx.get_menus().unwrap();
        let edit = menus.iter().find(|menu| menu.name == "Edit").unwrap();
        let create = edit
            .items
            .iter()
            .find_map(|item| match item {
                OwnedMenuItem::Submenu(menu) if menu.name == "Create" => Some(menu),
                _ => None,
            })
            .unwrap();
        create
            .items
            .iter()
            .find_map(|item| match item {
                OwnedMenuItem::Action {
                    name,
                    action,
                    disabled,
                    ..
                } => action
                    .as_any()
                    .downcast_ref::<RunCommand>()
                    .filter(|action| action.id == id)
                    .map(|action| (action.clone(), name.clone(), *disabled)),
                _ => None,
            })
            .expect("native create action")
    })
    .unwrap()
}

#[gpui_kit::test]
fn native_create_menu_exposes_scene_and_parameter_commands_and_dispatches_targeted_actions(
    cx: &mut TestAppContext,
) {
    let h = blank(cx);
    let (_, reason, disabled) = native_action(&h, C::CreateOffscreen, cx);
    assert!(disabled);
    assert!(reason.contains("Select a Part"));
    let (action, label, disabled) = native_action(&h, C::CreatePart, cx);
    assert_eq!(label, "New Part");
    assert!(!disabled);
    assert_eq!(action.invocation.unwrap().source, CommandSource::NativeMenu);
    cx.update_window(h.window, |_, window, cx| {
        window.dispatch_action(Box::new(action), cx)
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(inspected(&h, cx).kind, K::Part);
    for id in [C::CreateParameter, C::CreateParameterGroup] {
        assert!(!native_action(&h, id, cx).2);
    }
    assert!(!native_action(&h, C::CreateOffscreen, cx).2);
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn every_scene_command_source_uses_the_same_owner_defaults_and_single_history(
    cx: &mut TestAppContext,
) {
    let h = blank(cx);
    invoke(&h, C::CreatePart, CommandSource::Direct, cx);
    let part = inspected(&h, cx);
    let mut history = 1;
    for source in [
        CommandSource::Direct,
        CommandSource::NativeMenu,
        CommandSource::Shortcut,
    ] {
        for id in [C::CreateMesh, C::CreateWarp, C::CreateRotation] {
            select(&h, part.clone(), cx);
            invoke(&h, id, source, cx);
            history += 1;
            let created = inspected(&h, cx);
            assert_eq!(
                created.kind,
                id.scene_creation_type().unwrap().object_kind()
            );
            h.view.read_with(cx, |view, _| {
                let doc = view.editor.document().unwrap();
                let tree = doc.mesh_tree();
                assert!(
                    tree.iter()
                        .find(|n| n.id == part.id)
                        .unwrap()
                        .children
                        .iter()
                        .any(|n| n.id == created.id)
                );
                assert_eq!(doc.history_lengths(), (history, 0));
                assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            });
        }
    }
    select(&h, part, cx);
    invoke(&h, C::CreateOffscreen, CommandSource::NativeMenu, cx);
    let offscreen = inspected(&h, cx);
    assert_eq!(offscreen.kind, K::Offscreen);
    invoke(&h, C::Undo, CommandSource::Direct, cx);
    h.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .object_index()
                .get(&offscreen)
                .is_none()
        )
    });
    invoke(&h, C::Redo, CommandSource::Direct, cx);
    h.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .object_index()
                .get(&offscreen)
                .is_some()
        )
    });
}

#[gpui_kit::test]
fn configured_creation_shortcut_ignores_text_and_repeat_but_creates_from_editor_focus(
    cx: &mut TestAppContext,
) {
    let h = blank(cx);
    cx.update_window(h.window, |_, window, cx| {
        cx.global::<crate::settings::Settings>()
            .store
            .write(
                "keybindings.json",
                &ShortcutOverrides {
                    bindings: vec![ShortcutOverride {
                        command: C::CreateMesh,
                        keys: vec!["ctrl-alt-m".into()],
                        platform: None,
                    }],
                },
            )
            .unwrap();
        crate::keybindings::reload(cx);
        window.click("mesh-tree-search", cx);
        window.press("ctrl-alt-m", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0))
    });
    cx.update_window(h.window, |_, window, cx| {
        window.focus(&h.view.read(cx).focus.clone(), cx);
        window.press("ctrl-alt-m", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let mesh = inspected(&h, cx);
    assert_eq!(mesh.kind, K::Mesh);
    cx.update_window(h.window, |_, window, cx| {
        window.focus(&h.view.read(cx).focus.clone(), cx);
        window.dispatch_event(
            KeyDownEvent {
                keystroke: Keystroke::parse("ctrl-alt-m").unwrap(),
                is_held: true,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn creation_settles_text_and_filtered_command_result_is_revealed_and_name_focused(
    cx: &mut TestAppContext,
) {
    let h = blank(cx);
    invoke(&h, C::CreatePart, CommandSource::Direct, cx);
    let part = inspected(&h, cx);
    fill(&h, "property-input-name", "Character", cx);
    invoke(&h, C::CreateMesh, CommandSource::NativeMenu, cx);
    let mesh = inspected(&h, cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.object_index().name(&part), "Character");
        assert_eq!(doc.history_lengths(), (3, 0));
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-search", cx);
        window.input("nothing matches", cx);
    })
    .unwrap();
    invoke(&h, C::CreateWarp, CommandSource::NativeMenu, cx);
    let warp = inspected(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(
            window.find("mesh-tree-search").value(),
            Some("nothing matches")
        );
        assert!(
            window
                .find(format!("mesh-item-WarpDeformer:{}", warp.id))
                .visible()
        );
        assert!(window.find("tree-created-item").visible());
        window.input("Warp renamed", cx); // default name was selected
        window.press("enter", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.object_index().name(&warp), "Warp renamed");
        assert_eq!(doc.object_index().name(&mesh), "Mesh");
        assert_eq!(doc.history_lengths(), (5, 0));
    });
}

#[gpui_kit::test]
fn native_unavailable_reason_refreshes_when_availability_stays_disabled(cx: &mut TestAppContext) {
    let h = blank(cx);
    invoke(&h, C::CreateMesh, CommandSource::Direct, cx);
    let unowned = inspected(&h, cx);
    invoke(&h, C::CreatePart, CommandSource::Direct, cx);
    let part = inspected(&h, cx);
    invoke(&h, C::CreateOffscreen, CommandSource::NativeMenu, cx);
    let (_, duplicate, disabled) = native_action(&h, C::CreateOffscreen, cx);
    assert!(disabled);
    assert!(duplicate.contains("already has an Offscreen"));
    let before = h
        .view
        .read_with(cx, |view, _| view.menu_state.commands.clone());
    select(&h, unowned, cx);
    let (_, missing, disabled) = native_action(&h, C::CreateOffscreen, cx);
    assert!(disabled);
    assert!(missing.contains("Select a Part"));
    h.view
        .read_with(cx, |view, _| assert_eq!(view.menu_state.commands, before));
    select(&h, part.clone(), cx);
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SetObjectsLocked {
                objects: vec![part],
                locked: true,
            });
            view.complete_edit(result, cx);
        })
    })
    .unwrap();
    let (_, locked, disabled) = native_action(&h, C::CreateMesh, cx);
    assert!(disabled);
    assert!(locked.contains("Unlock"));
    invoke(&h, C::CreateMesh, CommandSource::NativeMenu, cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (4, 0))
    });
}

#[gpui_kit::test]
fn saved_native_command_and_creation_in_animation_or_modal_do_not_publish(cx: &mut TestAppContext) {
    let h = blank(cx);
    let (saved, _, _) = native_action(&h, C::CreateMesh, cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            view.refresh_document(window, cx);
        });
        window.dispatch_action(Box::new(saved), cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0))
    });
    invoke(&h, C::AnimationWorkspace, CommandSource::Direct, cx);
    invoke(&h, C::CreateMesh, CommandSource::NativeMenu, cx);
    invoke(&h, C::ModelingWorkspace, CommandSource::Direct, cx);
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        })
    })
    .unwrap();
    invoke(&h, C::CreateMesh, CommandSource::NativeMenu, cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0))
    });
}

#[gpui_kit::test]
fn creating_from_native_menu_reopens_hidden_tree_and_properties_and_focuses_the_name(
    cx: &mut TestAppContext,
) {
    let h = blank(cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            dock.toggle(editor_core::PanelKind::MeshTree, window, cx);
            dock.toggle(editor_core::PanelKind::Properties, window, cx);
            assert!(!dock.displayed(editor_core::PanelKind::MeshTree, cx));
            assert!(!dock.displayed(editor_core::PanelKind::Properties, cx));
        });
    })
    .unwrap();
    invoke(&h, C::CreatePart, CommandSource::NativeMenu, cx);
    let part = inspected(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.find(format!("mesh-item-Part:{}", part.id)).visible());
        assert!(window.find("property-input-name").visible());
        window.input("Named after reopening", cx);
        window.press("enter", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.object_index().name(&part), "Named after reopening");
        assert_eq!(doc.history_lengths(), (2, 0));
    });
}

#[gpui_kit::test]
fn creation_name_focus_wins_when_tree_and_properties_share_a_tab_group(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::{DockLayout, PanelId, panel_handle};
    let h = blank(cx);
    cx.update_window(h.window, |_, window, cx| {
        let dock = h.view.read(cx).dock.as_ref().unwrap();
        let layout = DockLayout::h_split()
            .child(
                DockLayout::tabs()
                    .panel_view(panel_handle(dock.mesh_tree.clone()), cx)
                    .panel_view(panel_handle(dock.properties.clone()), cx),
                Some(gpui_kit::px(300.)),
            )
            .child(
                DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                None,
            );
        let tree = PanelId::from(dock.mesh_tree.entity_id());
        let area = dock.area.clone();
        area.update(cx, |area, cx| {
            area.set_center(layout, window, cx);
            area.select_panel(tree, window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    invoke(&h, C::CreatePart, CommandSource::NativeMenu, cx);
    let part = inspected(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.find("property-input-name").visible());
        window.input("Shared tabs", cx);
        window.press("enter", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().object_index().name(&part),
            "Shared tabs"
        )
    });
}

#[gpui_kit::test]
fn creation_resets_tree_range_anchor_to_the_new_selection(cx: &mut TestAppContext) {
    let h = blank(cx);
    invoke(&h, C::CreateMesh, CommandSource::Direct, cx);
    let a = inspected(&h, cx);
    invoke(&h, C::CreateMesh, CommandSource::Direct, cx);
    let b = inspected(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("mesh-item-Mesh:{}", a.id), cx)
    })
    .unwrap();
    invoke(&h, C::CreatePart, CommandSource::NativeMenu, cx);
    let part = inspected(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_modifiers(
            format!("mesh-item-Mesh:{}", b.id),
            gpui_kit::Modifiers {
                shift: true,
                ..Default::default()
            },
            cx,
        );
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let selected = &view.editor.document().unwrap().context().selections;
        assert_eq!(selected, &[part, a, b]);
    });
}
