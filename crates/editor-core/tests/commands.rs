use editor_core::*;
mod support;
#[test]
fn commands_check_targets_busy_state_and_workspace_availability() -> anyhow::Result<()> {
    let (doc, dir) = support::sample_document()?;
    let mut editor = Editor::from_document(doc);
    editor.execute(EditorCommand::SetVisible(false))?;
    let undo = editor.command(CommandId::Undo);
    assert!(editor.command_available(undo));
    let original = editor.active_id().unwrap();
    editor.new_document()?;
    assert!(!editor.command_available(undo));
    assert!(editor.dispatch(undo).is_err());
    editor.activate(original)?;
    let task = editor.prepare_save(original, &dir.path().join("save"), false)?;
    for id in [
        CommandId::Undo,
        CommandId::Save,
        CommandId::Vertices,
        CommandId::ZoomIn,
    ] {
        assert!(!editor.command_available(editor.command(id)));
    }
    assert!(editor.command_available(editor.command(CommandId::Open)));
    editor.new_document()?;
    assert!(editor.command_available(editor.command(CommandId::Save)));
    editor.finish_save(&mut task.run())?;
    Ok(())
}

#[test]
fn visibility_targets_the_clicked_layer_without_changing_selection() -> anyhow::Result<()> {
    let (doc, _dir) = support::sample_document_with_two_layers()?;
    let mut editor = Editor::from_document(doc);
    let selected = editor.document().unwrap().selected_id().unwrap().to_owned();
    let second = editor
        .layers()
        .into_iter()
        .find(|layer| layer.name == "Second")
        .unwrap();

    editor.execute(EditorCommand::SetLayerVisible {
        layer_id: second.id.clone(),
        visible: false,
    })?;
    assert_eq!(
        editor.document().unwrap().selected_id(),
        Some(selected.as_str())
    );
    assert!(editor.selected_layer().unwrap().visible);
    assert!(
        !editor
            .layers()
            .into_iter()
            .find(|l| l.id == second.id)
            .unwrap()
            .visible
    );

    editor.execute(EditorCommand::Undo)?;
    assert!(
        editor
            .layers()
            .into_iter()
            .find(|l| l.id == second.id)
            .unwrap()
            .visible
    );
    Ok(())
}
#[test]
fn shortcut_override_is_atomic_and_allows_disjoint_contexts() -> anyhow::Result<()> {
    let normalize = |s: &str| Ok(s.to_lowercase());
    let defaults = ShortcutOverrides::default().resolve(normalize)?;
    let overrides = ShortcutOverrides {
        bindings: vec![
            ShortcutOverride {
                command: CommandId::Open,
                keys: vec!["cmd-alt-o".into()],
            },
            ShortcutOverride {
                command: CommandId::Save,
                keys: vec![],
            },
        ],
    };
    let resolved = overrides.resolve(normalize)?;
    assert_eq!(
        resolved
            .iter()
            .filter(|b| b.command == CommandId::Open)
            .map(|b| b.key.as_str())
            .collect::<Vec<_>>(),
        ["cmd-alt-o"]
    );
    assert!(!resolved.iter().any(|b| b.command == CommandId::Save));
    assert_eq!(defaults.iter().filter(|b| b.key == "escape").count(), 2);
    let conflict = ShortcutOverrides {
        bindings: vec![ShortcutOverride {
            command: CommandId::Open,
            keys: vec!["cmd-s".into()],
        }],
    };
    assert!(conflict.resolve(normalize).is_err());
    assert!(
        ShortcutOverrides {
            bindings: vec![ShortcutOverride {
                command: CommandId::Open,
                keys: vec!["a".into()]
            }]
        }
        .resolve(normalize)
        .is_err()
    );
    assert!(
        ShortcutOverrides {
            bindings: vec![ShortcutOverride {
                command: CommandId::Open,
                keys: vec!["cmd-k cmd-o".into()]
            }]
        }
        .resolve(normalize)
        .is_err()
    );
    Ok(())
}
#[test]
fn versioned_preferences_roundtrip_reject_corruption_and_unknown_commands() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let store = ConfigStore::new(dir.path());
    let config = ShortcutOverrides {
        bindings: vec![ShortcutOverride {
            command: CommandId::Open,
            keys: vec![],
        }],
    };
    store.write("keys.json", &config)?;
    assert!(
        store
            .read::<ShortcutOverrides>("keys.json")?
            .unwrap()
            .bindings[0]
            .keys
            .is_empty()
    );
    std::fs::write(
        dir.path().join("keys.json"),
        r#"{"version":1,"data":{"bindings":[{"command":"unknown","keys":[]}]}}"#,
    )?;
    assert!(store.read::<ShortcutOverrides>("keys.json").is_err());
    std::fs::write(
        dir.path().join("keys.json"),
        r#"{"version":2,"data":{"bindings":[]}}"#,
    )?;
    assert!(store.read::<ShortcutOverrides>("keys.json").is_err());
    std::fs::write(dir.path().join("keys.json"), "{")?;
    assert!(store.read::<ShortcutOverrides>("keys.json").is_err());
    store.write("keys.json", &ShortcutOverrides::default())?;
    assert!(
        store
            .read::<ShortcutOverrides>("keys.json")?
            .unwrap()
            .bindings
            .is_empty()
    );
    assert_eq!(std::fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn tree_cache_revision_and_visibility_follow_edits_not_navigation() -> anyhow::Result<()> {
    fn visibility(nodes: &[TreeNode], id: &str) -> Option<bool> {
        nodes.iter().find_map(|node| {
            if node.id == id && node.kind == TreeNodeKind::Mesh {
                node.visible
            } else {
                visibility(&node.children, id)
            }
        })
    }
    let (doc, _dir) = support::sample_document_with_two_layers()?;
    let mut editor = Editor::from_document(doc);
    let id = editor
        .layers()
        .into_iter()
        .find(|layer| layer.name == "Second")
        .unwrap()
        .id;
    let revision = editor.tree_revision();
    editor.execute(EditorCommand::Select(id.clone()))?;
    editor.zoom_by(0.1);
    editor.pan_by(CanvasPoint { x: 10., y: 10. });
    assert_eq!(editor.tree_revision(), revision);
    assert_eq!(editor.selected_layer_id(), Some(id.as_str()));
    editor.execute(EditorCommand::SetLayerVisible {
        layer_id: id.clone(),
        visible: false,
    })?;
    assert_ne!(editor.tree_revision(), revision);
    for tree in [editor.mesh_tree(), editor.deformer_tree()] {
        assert_eq!(visibility(&tree, &id), Some(false));
    }
    let hidden_revision = editor.tree_revision();
    editor.execute(EditorCommand::Undo)?;
    assert_ne!(editor.tree_revision(), hidden_revision);
    for tree in [editor.mesh_tree(), editor.deformer_tree()] {
        assert_eq!(visibility(&tree, &id), Some(true));
    }
    editor.execute(EditorCommand::Redo)?;
    for tree in [editor.mesh_tree(), editor.deformer_tree()] {
        assert_eq!(visibility(&tree, &id), Some(false));
    }
    Ok(())
}
