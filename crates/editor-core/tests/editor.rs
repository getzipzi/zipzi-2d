use editor_core::{CloseDisposition, Document, Editor, EditorCommand, LoadRequest};

mod support;
use support::sample_document;

#[test]
fn busy_editor_blocks_mutations_and_preserves_history() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = Editor::from_document(document);
    editor.execute(EditorCommand::PreviewName("Renamed".into()))?;
    editor.begin_operation()?;
    let history = editor.document().history_lengths();
    assert_eq!(editor.close_disposition(), CloseDisposition::Busy);
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
    assert!(editor.begin_operation().is_err());
    assert!(editor.execute(EditorCommand::SetVisible(false)).is_err());
    assert!(editor.execute(EditorCommand::Undo).is_err());
    assert!(editor.execute(EditorCommand::CancelEdit).is_err());
    assert!(
        editor
            .save(editor.document().path().unwrap().to_owned().as_path())
            .is_err()
    );
    assert_eq!(editor.document().history_lengths(), history);
    assert!(editor.document().selected_layer().unwrap().visible);
    editor.end_operation();
    assert_eq!(
        editor.close_disposition(),
        CloseDisposition::NeedsConfirmation
    );
    assert!(editor.can_undo());
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().selected_layer().unwrap().name, "Diamond");
    assert_eq!(editor.close_disposition(), CloseDisposition::Ready);
    Ok(())
}

#[test]
fn failed_load_preserves_document_selection_view_and_unsaved_edits() -> anyhow::Result<()> {
    let (document, source) = sample_document()?;
    let mut editor = Editor::from_document(document);
    let selected = editor.document().selected_id().unwrap().to_owned();
    editor.execute(EditorCommand::PreviewOpacity(0.4))?;
    editor.zoom_by(0.75);
    editor.begin_operation()?;
    let history = editor.document().history_lengths();
    let result = LoadRequest::Open(source.path().join("missing.kasane")).load();
    assert!(editor.finish_load(result).is_err());
    assert!(!editor.is_busy());
    assert_eq!(editor.document().selected_id(), Some(selected.as_str()));
    assert_eq!(editor.document().selected_layer().unwrap().opacity, 0.4);
    assert_eq!(editor.document().history_lengths(), history);
    assert!(editor.document().modified());
    assert_eq!(editor.view().zoom, 1.75);
    Ok(())
}

#[test]
fn replacement_and_edits_reject_old_previews() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = Editor::from_document(document);
    let original = editor.preview()?;
    assert!(editor.accepts_preview(original.revision));
    editor.execute(EditorCommand::PreviewOpacity(0.4))?;
    assert!(!editor.accepts_preview(original.revision));
    let candidate = editor.preview()?;
    assert_eq!(candidate.scene.frame.drawables[0].opacity, 0.4);
    editor.execute(EditorCommand::CancelEdit)?;
    assert!(!editor.accepts_preview(candidate.revision));
    editor.zoom_by(1.0);
    let previous = editor.preview()?;
    editor.begin_operation()?;
    let notices = editor.finish_load(Ok((Document::new_blank()?, vec!["notice".into()])))?;
    assert_eq!(notices, ["notice"]);
    assert!(!editor.accepts_preview(previous.revision));
    assert!(editor.document().layers().is_empty());
    assert_eq!(editor.view().zoom, 1.0);
    assert_eq!(editor.close_disposition(), CloseDisposition::Ready);
    assert_eq!(previous.scene.frame.drawables[0].opacity, 1.0);
    assert!(editor.preview()?.scene.frame.drawables.is_empty());
    Ok(())
}

#[test]
fn save_and_reopen_preserve_edits_without_persisting_view_state() -> anyhow::Result<()> {
    let (document, _source) = sample_document()?;
    let mut editor = Editor::from_document(document);
    editor.execute(EditorCommand::PreviewName("Saved".into()))?;
    editor.zoom_by(100.0);
    assert_eq!(editor.view().zoom, 3.0);
    let output = tempfile::tempdir()?;
    let receipt = editor.save(&output.path().join("saved.kasane"))?;
    assert_eq!(editor.close_disposition(), CloseDisposition::Ready);
    assert!(editor.can_undo());
    editor.begin_operation()?;
    editor.finish_load(LoadRequest::Open(receipt.manifest).load())?;
    assert_eq!(editor.document().selected_layer().unwrap().name, "Saved");
    assert_eq!(editor.view().zoom, 1.0);
    editor.zoom_by(-100.0);
    assert_eq!(editor.view().zoom, 0.25);
    assert!(!editor.document().modified());
    Ok(())
}
