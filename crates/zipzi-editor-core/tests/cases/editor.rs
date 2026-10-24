use super::support;
use support::sample_document;
use zipzi_editor_core::{CloseDisposition, CommandId, Editor, EditorCommand, LoadRequest};

#[test]
fn parameter_preview_is_transient_and_survives_document_switch() -> anyhow::Result<()> {
    let (document, _source) = support::sample_document_with_binding(true)?;
    let mut editor = crate::support::editor_from_document(document);
    let first = editor.active_id().unwrap();
    let parameter = editor.document().unwrap().parameters().remove(0);
    let history = editor.document().unwrap().history_lengths();

    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.id.clone(),
        value: 0.75,
    })?;
    assert_eq!(editor.document().unwrap().parameters()[0].value, 0.75);
    assert!(!editor.document().unwrap().modified());
    assert_eq!(editor.document().unwrap().history_lengths(), history);

    assert!(
        editor
            .execute(EditorCommand::SetPreviewParameter {
                parameter_id: parameter.id.clone(),
                value: 2.0,
            })
            .is_err()
    );
    assert_eq!(editor.document().unwrap().parameters()[0].value, 0.75);

    editor.execute(EditorCommand::PreviewName("Renamed".into()))?;
    editor.preview()?;
    editor.execute(EditorCommand::FinishEdit)?;
    editor.new_document()?;
    editor.activate(first)?;
    assert_eq!(editor.document().unwrap().parameters()[0].value, 0.75);

    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.id.clone(),
        value: parameter.default_value,
    })?;
    assert_eq!(
        editor.document().unwrap().parameters()[0].value,
        parameter.default_value
    );
    Ok(())
}

fn load(editor: &mut Editor, path: &std::path::Path) -> anyhow::Result<()> {
    if let Some(task) = editor.prepare_load(LoadRequest::Open(path.to_owned()))? {
        editor.finish_load(task.run())?;
    }
    Ok(())
}

#[test]
fn zoom_stays_within_four_to_thirty_two_hundred_percent() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);

    editor.zoom_by(-100.0);
    assert_eq!(editor.view().zoom, 0.04);
    support::attach_viewports(&mut editor);
    assert!(!editor.command_available(editor.command(CommandId::ZoomOut)));
    assert!(editor.command_available(editor.command(CommandId::ZoomIn)));

    editor.zoom_by(100.0);
    assert_eq!(editor.view().zoom, 32.0);
    assert!(!editor.command_available(editor.command(CommandId::ZoomIn)));
    assert!(editor.command_available(editor.command(CommandId::ZoomOut)));
    Ok(())
}

#[test]
fn tabs_preserve_selection_history_view_and_pending_property_edits() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    let first = editor.active_id().unwrap();
    let selected = editor.document().unwrap().selected_id().unwrap().to_owned();
    editor.execute(EditorCommand::PreviewName("First".into()))?;
    editor.zoom_by(0.75);
    let second = editor.new_document()?;
    assert_ne!(first, second);
    assert!(editor.document().unwrap().layers().is_empty());
    assert_eq!(editor.view().zoom, 1.0);
    editor.activate(first)?;
    assert_eq!(editor.view().zoom, 1.75);
    assert_eq!(
        editor.document().unwrap().selected_id(),
        Some(selected.as_str())
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().selected_layer().unwrap().name,
        "Diamond"
    );
    editor.activate(second)?;
    assert!(!editor.can_undo());
    assert_eq!(editor.adjacent(true), Some(first));
    Ok(())
}

#[test]
fn failed_and_duplicate_opens_preserve_the_existing_session() -> anyhow::Result<()> {
    let (document, source) = sample_document()?;
    let path = document.path().unwrap().to_owned();
    let mut editor = crate::support::editor_from_document(document);
    let id = editor.active_id();
    editor.execute(EditorCommand::PreviewOpacity(0.4))?;
    editor.zoom_by(0.75);
    let task = editor
        .prepare_load(LoadRequest::Open(source.path().join("missing")))?
        .unwrap();
    assert!(editor.finish_load(task.run()).is_err());
    assert_eq!(editor.active_id(), id);
    assert_eq!(editor.view().zoom, 1.75);
    assert_eq!(
        editor.document().unwrap().selected_layer().unwrap().opacity,
        0.4
    );
    editor.new_document()?;
    load(&mut editor, path.parent().unwrap())?;
    assert_eq!(editor.documents().len(), 2);
    assert_eq!(editor.active_id(), id);
    load(&mut editor, &path)?;
    assert_eq!(editor.documents().len(), 2);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn same_model_uuid_in_two_files_has_distinct_document_identity() -> anyhow::Result<()> {
    let (mut document, _source) = sample_document()?;
    let original = document.path().unwrap().to_owned();
    let output = tempfile::tempdir()?;
    let copy = document.save(&output.path().join("copy"))?.manifest;
    let mut editor = crate::support::empty_editor();
    load(&mut editor, &original)?;
    let first = editor.active_id().unwrap();
    let mesh = editor.layers()[0].id.clone();
    editor.execute(EditorCommand::Select(mesh.clone()))?;
    load(&mut editor, &copy)?;
    let second = editor.active_id().unwrap();
    editor.execute(EditorCommand::Select(mesh))?;
    assert_ne!(first, second);
    editor.execute(EditorCommand::SetObjectVisible {
        object: editor
            .document()
            .unwrap()
            .context()
            .selection
            .clone()
            .unwrap(),
        visible: false,
    })?;
    editor.activate(first)?;
    assert!(editor.document().unwrap().selected_layer().unwrap().visible);
    assert!(!editor.can_undo());
    assert!(editor.prepare_save(first, &copy, false).is_err());
    Ok(())
}

#[test]
fn save_moves_the_same_session_to_a_thread_and_freezes_only_its_document() -> anyhow::Result<()> {
    // These compile-time bounds include owned SDK candidates and save error results.
    fn assert_send<T: Send>() {}
    assert_send::<zipzi_editor_core::SaveTask>();
    assert_send::<zipzi_editor_core::SaveCompletion>();
    let (document, _source) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    let first = editor.active_id().unwrap();
    editor.execute(EditorCommand::PreviewName("Saved".into()))?;
    editor.zoom_by(0.5);
    let mesh_tree = editor.mesh_tree();
    let deformer_tree = editor.deformer_tree();
    let layer_names: Vec<_> = editor
        .layers()
        .into_iter()
        .map(|layer| (layer.id, layer.name))
        .collect();
    let selected = editor.selected_layer_id().map(str::to_owned);
    let output = tempfile::tempdir()?;
    let task = editor.prepare_save(first, &output.path().join("saved"), false)?;
    assert!(editor.is_busy());
    assert!(editor.document().is_none());
    assert_eq!(editor.mesh_tree(), mesh_tree);
    assert_eq!(editor.deformer_tree(), deformer_tree);
    assert_eq!(
        editor
            .layers()
            .into_iter()
            .map(|layer| (layer.id, layer.name))
            .collect::<Vec<_>>(),
        layer_names
    );
    assert_eq!(editor.selected_layer_id(), selected.as_deref());
    assert!(editor.execute_for(first, EditorCommand::Undo).is_err());
    assert_eq!(editor.selected_layer().unwrap().name, "Saved");
    assert_eq!(editor.view().zoom, 1.5);
    let second = editor.new_document()?;
    assert!(!editor.is_busy());
    editor.zoom_by(1.0);
    let receipt =
        editor.finish_save(&mut std::thread::spawn(move || task.run()).join().unwrap())?;
    assert_eq!(editor.active_id(), Some(second));
    assert_eq!(editor.view().zoom, 2.0);
    editor.activate(first)?;
    assert_eq!(
        editor.document().unwrap().path(),
        Some(receipt.manifest.as_path())
    );
    assert!(!editor.document().unwrap().modified());
    assert!(editor.can_undo());
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().selected_layer().unwrap().name,
        "Diamond"
    );
    Ok(())
}

#[test]
fn failed_save_returns_history_and_cancels_dependent_close_and_exit() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    let id = editor.active_id().unwrap();
    editor.execute(EditorCommand::PreviewName("Keep me".into()))?;
    let output = tempfile::tempdir()?;
    let blocker = output.path().join("not-a-directory");
    let task = editor.prepare_save(id, &blocker.join("project"), true)?;
    std::fs::write(&blocker, "blocked")?;
    editor.request_exit()?;
    assert!(!editor.close_document(id, true)?);
    assert!(
        editor
            .finish_save(&mut std::thread::spawn(move || task.run()).join().unwrap())
            .is_err()
    );
    assert!(!editor.exiting());
    assert_eq!(editor.active_id(), Some(id));
    assert_eq!(
        editor.close_disposition(id)?,
        CloseDisposition::NeedsConfirmation
    );
    assert!(editor.can_undo());
    assert_eq!(
        editor.document().unwrap().selected_layer().unwrap().name,
        "Keep me"
    );
    Ok(())
}

#[test]
fn close_waits_for_save_and_last_close_leaves_an_empty_workspace() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    let id = editor.active_id().unwrap();
    editor.execute(EditorCommand::PreviewName("Close me".into()))?;
    assert!(editor.close_document(id, false).is_err());
    editor.request_exit()?;
    editor.cancel_exit(); // Cancel dialog or Save As path picker: no document is removed.
    assert_eq!(editor.documents().len(), 1);
    let output = tempfile::tempdir()?;
    let task = editor.prepare_save(id, &output.path().join("saved"), false)?;
    assert!(!editor.close_document(id, false)?);
    editor.finish_save(&mut task.run())?;
    assert!(editor.documents().is_empty());
    assert!(editor.active_id().is_none());
    assert!(editor.execute_for(id, EditorCommand::Undo).is_err());
    editor.new_document()?;
    assert_eq!(editor.documents().len(), 1);
    Ok(())
}

#[test]
fn previews_and_load_completions_never_target_the_tab_active_at_completion() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let path = document.path().unwrap().to_owned();
    let mut editor = crate::support::editor_from_document(document);
    let original = editor.preview()?;
    let first = editor.active_id().unwrap();
    editor.new_document()?;
    assert!(!editor.accepts_preview(original.revision));
    let latest = editor.preview()?;
    assert!(editor.accepts_preview(latest.revision));
    editor.activate(first)?;
    assert!(!editor.accepts_preview(latest.revision));
    assert!(!editor.accepts_preview(original.revision));
    let mut other = crate::support::empty_editor();
    let task = other
        .prepare_load(LoadRequest::Open(path.clone()))?
        .unwrap();
    assert!(editor.finish_load(task.run()).is_err());
    assert_eq!(editor.documents().len(), 2);
    let mut loading = crate::support::empty_editor();
    let task = loading.prepare_load(LoadRequest::Open(path))?.unwrap();
    loading.new_document()?;
    loading.finish_load(task.run())?;
    assert_eq!(loading.documents().len(), 2);
    let document = loading.document().unwrap();
    assert_eq!(document.layers()[0].name, "Diamond");
    assert!(document.selected_id().is_none());
    assert!(document.context().selection.is_none());
    assert!(document.context().selections.is_empty());
    assert!(document.context().inspected.is_none());
    Ok(())
}

#[test]
fn in_flight_save_reserves_its_destination_against_other_tabs() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    let first = editor.active_id().unwrap();
    let output = tempfile::tempdir()?;
    let path = output.path().join("saved");
    let task = editor.prepare_save(first, &path, false)?;
    let second = editor.new_document()?;
    assert!(
        editor
            .prepare_save(second, &path.join("project.zipzi.json"), false)
            .is_err()
    );
    assert!(
        editor
            .prepare_save(first, &output.path().join("another"), false)
            .is_err()
    );
    editor.finish_save(&mut task.run())?;
    Ok(())
}

#[test]
fn misdelivered_save_can_be_returned_to_its_owner_without_losing_the_session() -> anyhow::Result<()>
{
    let (document, _source) = sample_document()?;
    let mut owner = crate::support::editor_from_document(document);
    let mut wrong_workspace = crate::support::new_editor()?;
    let id = owner.active_id().unwrap();
    owner.execute(EditorCommand::PreviewName("Survives".into()))?;
    let output = tempfile::tempdir()?;
    let mut completion = owner
        .prepare_save(id, &output.path().join("saved"), false)?
        .run();
    assert!(wrong_workspace.finish_save(&mut completion).is_err());
    assert!(owner.is_busy());
    owner.finish_save(&mut completion)?;
    assert!(owner.finish_save(&mut completion).is_err());
    assert_eq!(
        owner.document().unwrap().selected_layer().unwrap().name,
        "Survives"
    );
    assert!(owner.can_undo());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_project_directory_and_pending_save_aliases_share_identity() -> anyhow::Result<()> {
    let (document, source) = sample_document()?;
    let path = document.path().unwrap().to_owned();
    let alias_root = tempfile::tempdir()?;
    let alias = alias_root.path().join("source");
    std::os::unix::fs::symlink(source.path(), &alias)?;
    let relative = path.strip_prefix(source.path().canonicalize()?)?;
    let mut editor = crate::support::editor_from_document(document);
    let first = editor.active_id();
    editor.new_document()?;
    load(&mut editor, &alias.join(relative))?;
    assert_eq!(editor.active_id(), first);
    assert_eq!(editor.documents().len(), 2);
    let task = editor.prepare_save(first.unwrap(), &source.path().join("pending"), false)?;
    let second = editor.adjacent(true).unwrap();
    assert!(
        editor
            .prepare_save(second, &alias.join("pending/project.zipzi.json"), false)
            .is_err()
    );
    editor.finish_save(&mut task.run())?;
    Ok(())
}

#[test]
fn saving_a_background_document_does_not_invalidate_the_active_preview() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    let first = editor.active_id().unwrap();
    editor.execute(EditorCommand::PreviewName("Background".into()))?;
    editor.new_document()?;
    let preview = editor.preview()?;
    let output = tempfile::tempdir()?;
    let task = editor.prepare_save(first, &output.path().join("saved"), false)?;
    assert!(editor.accepts_preview(preview.revision));
    editor.finish_save(&mut task.run())?;
    assert!(editor.accepts_preview(preview.revision));
    Ok(())
}

#[test]
fn save_completion_resubmits_a_preview_rejected_while_saving() -> anyhow::Result<()> {
    let (document, dir) = sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    editor.set_canvas_metrics(zipzi_editor_core::ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let pending = editor.preview_if_needed()?.unwrap();
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &dir.path().join("saved"),
        false,
    )?;
    assert!(!editor.accepts_preview(pending.revision));
    editor.finish_save(&mut task.run())?;
    assert!(!editor.accepts_preview(pending.revision));
    assert!(editor.preview_if_needed()?.is_some());
    Ok(())
}

#[test]
fn cancelling_exit_revokes_only_exit_owned_deferred_closes() -> anyhow::Result<()> {
    for explicit_close in [false, true] {
        let (document, dir) = sample_document()?;
        let mut editor = crate::support::editor_from_document(document);
        let id = editor.active_id().unwrap();
        let task = editor.prepare_save(id, &dir.path().join("saved"), false)?;
        if explicit_close {
            assert!(!editor.close_document(id, false)?);
        }
        editor.request_exit()?;
        assert!(!editor.close_document(id, false)?);
        editor.cancel_exit();
        editor.finish_save(&mut task.run())?;
        assert_eq!(editor.documents().is_empty(), explicit_close);
    }
    Ok(())
}

#[test]
fn empty_workspace_starts_without_documents_and_is_not_busy() -> anyhow::Result<()> {
    let mut editor = crate::support::empty_editor();
    assert!(editor.documents().is_empty());
    assert!(editor.active_id().is_none());
    assert!(!editor.is_busy());
    assert!(editor.preview_if_needed()?.is_none());
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
    Ok(())
}
