use super::{document_fixture, setup_with_document};
use gpui_kit::{AppContext as _, Modifiers, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{
    CommandId, EditorCommand, ObjectKind, ObjectRef, ShortcutOverride, ShortcutOverrides,
};

#[gpui_kit::test]
fn focused_tree_move_uses_full_order_under_filter_and_preserves_inspection(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::sample_document_with_two_layers().unwrap();
    let layers = document.layers();
    let second = layers
        .iter()
        .find(|layer| layer.name == "Second")
        .unwrap()
        .id
        .clone();
    let first = layers
        .iter()
        .find(|layer| layer.name == "Diamond")
        .unwrap()
        .id
        .clone();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        // The same physical key is legal in disjoint Palette/Viewport scopes.
        cx.global::<crate::settings::Settings>()
            .store
            .write(
                "keybindings.json",
                &ShortcutOverrides {
                    bindings: vec![ShortcutOverride {
                        command: CommandId::ZoomIn,
                        keys: vec!["alt-up".into()],
                        platform: None,
                    }],
                },
            )
            .unwrap();
        crate::keybindings::reload(cx);
        window.click("mesh-tree-search", cx);
        window.input("Second", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("mesh-item-Mesh:{second}"), cx);
        window.hover("canvas-body", cx); // hover must not change keyboard ownership
        window.press("alt-up", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.mesh_tree()[0].children[0].id, second);
        assert_eq!(doc.mesh_tree()[0].children[1].id, first);
        assert_eq!(doc.context().inspected.as_ref().unwrap().id, second);
        assert_eq!(doc.history_lengths(), (1, 0));
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(window.find("mesh-tree-search").value(), Some("Second"));
        window.press("alt-up", cx); // boundary no-op
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn palette_move_remapping_replaces_default_and_never_reorders_from_text_focus(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::sample_document_with_parameters(3).unwrap();
    let parameters = document.parameters();
    let id = parameters[1].id.clone();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        cx.global::<crate::settings::Settings>()
            .store
            .write(
                "keybindings.json",
                &ShortcutOverrides {
                    bindings: vec![ShortcutOverride {
                        command: CommandId::PaletteMoveUp,
                        keys: vec!["alt-k".into()],
                        platform: None,
                    }],
                },
            )
            .unwrap();
        crate::keybindings::reload(cx);
        window.click(format!("parameter-name-{id}"), cx);
        window.press("alt-up", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0))
    });
    cx.update_window(h.window, |_, window, cx| window.press("alt-k", cx))
        .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameters()[0].id, id);
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(doc.context().inspected.as_ref().unwrap().id, id);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-input-name", cx);
        window.press("alt-down", cx);
        window.press("alt-k", cx);
        window.click("mesh-tree-search", cx);
        window.press("alt-down", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn all_selected_shortcut_is_noop_and_locked_members_cannot_move(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_two_layers().unwrap();
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
        window.press("alt-up", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.context().selections.len(), 2);
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("mesh-item-Mesh:{}", meshes[1].id), cx);
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SetObjectsLocked {
                objects: vec![meshes[1].clone()],
                locked: true,
            });
            view.complete_edit(result, cx);
        });
        window.press("alt-up", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(doc.mesh_tree()[0].children[1].id, meshes[1].id);
        assert!(
            view.edit_error
                .as_ref()
                .is_some_and(|error| error.contains("Unlock"))
        );
    });
}
