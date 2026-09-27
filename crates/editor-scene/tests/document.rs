use editor_scene::Document;
mod support;
use support::sample_document;

#[test]
fn blank_document_starts_clean_and_can_be_saved() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    assert!(document.path().is_none());
    assert!(document.layers().is_empty());
    assert!(document.selected_layer().is_none());
    assert_eq!(document.history_lengths(), (0, 0));
    assert!(!document.modified());
    let snapshot = document.snapshot()?;
    assert!(snapshot.frame.drawables.is_empty());
    assert!(snapshot.textures.is_empty());

    let output = tempfile::tempdir()?;
    let receipt = document.save(&output.path().join("blank.kasane"))?;
    assert!(!document.modified());
    let mut reopened = Document::open(&receipt.manifest)?;
    assert!(reopened.layers().is_empty());
    assert!(reopened.snapshot()?.frame.drawables.is_empty());
    Ok(())
}

#[test]
fn sdk_imports_create_editable_documents_with_textures() -> anyhow::Result<()> {
    let sdk_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kasane-2d");
    let model3 = sdk_root.join("tests/fixtures/external_v50/model.model3.json");
    let (mut model, notices) = Document::import_model3(&model3)?;
    assert!(!model.layers().is_empty());
    assert!(model.path().is_none());
    assert_eq!(model.import_name(), Some("external_v50"));
    assert!(!model.snapshot()?.textures.is_empty());
    assert!(!notices.is_empty()); // Missing optional model3 attachments are reported.
    let output = tempfile::tempdir()?;
    model.save(&output.path().join("model.kasane"))?;
    assert!(!model.modified());

    let psd = sdk_root.join("modules/kasane-psd/tests/fixtures/layered.psd");
    let destination = output.path().join("layers.kasane");
    let (mut psd_document, _) = Document::import_psd(&psd, &destination)?;
    assert!(!psd_document.layers().is_empty());
    assert!(psd_document.path().is_some());
    assert!(!psd_document.snapshot()?.textures.is_empty());
    assert!(destination.join("project.kasane.json").exists());
    Ok(())
}

#[test]
fn opacity_gesture_previews_then_commits_one_undo_step() -> anyhow::Result<()> {
    let (mut document, _source) = sample_document()?;
    let original = document.snapshot()?;
    let history = document.history_lengths();
    for opacity in [0.9, 0.7, 0.4] {
        document.preview_opacity(opacity)?;
    }
    assert_eq!(document.history_lengths(), history);
    assert_eq!(document.selected_layer().unwrap().opacity, 0.4);
    assert_eq!(document.snapshot()?.frame.drawables[0].opacity, 0.4);
    assert_eq!(original.frame.drawables[0].opacity, 1.);
    document.finish_edit()?;
    assert_eq!(document.history_lengths(), (history.0 + 1, 0));
    document.undo()?;
    assert_eq!(document.selected_layer().unwrap().opacity, 1.);
    assert_eq!(document.snapshot()?.frame.drawables[0].opacity, 1.);
    document.redo()?;
    assert_eq!(document.selected_layer().unwrap().opacity, 0.4);
    Ok(())
}

#[test]
fn cancelled_and_unchanged_gestures_preserve_history_and_redo() -> anyhow::Result<()> {
    let (mut document, _source) = sample_document()?;
    document.set_visible(false)?;
    document.undo()?;
    let history = document.history_lengths();
    document.preview_name("New name")?;
    document.cancel_edit();
    assert_eq!(document.selected_layer().unwrap().name, "Diamond");
    document.preview_opacity(0.25)?;
    document.preview_opacity(1.)?;
    document.finish_edit()?;
    assert_eq!(document.history_lengths(), history);
    assert!(document.preview_opacity(f32::NAN).is_err());
    assert_eq!(document.history_lengths(), history);
    document.redo()?;
    assert!(!document.selected_layer().unwrap().visible);
    Ok(())
}

#[test]
fn rename_and_visibility_are_real_model_edits() -> anyhow::Result<()> {
    let (mut document, _source) = sample_document()?;
    document.preview_name("Face")?;
    document.finish_edit()?;
    document.set_visible(false)?;
    assert!(
        document
            .snapshot()?
            .frame
            .drawables
            .iter()
            .all(|drawable| !drawable.visible)
    );
    document.undo()?;
    assert!(document.selected_layer().unwrap().visible);
    document.undo()?;
    assert_eq!(document.selected_layer().unwrap().name, "Diamond");
    assert_eq!(document.history_lengths(), (0, 2));
    Ok(())
}

#[test]
fn project_round_trip_preserves_edits_and_copies_source_texture() -> anyhow::Result<()> {
    let output = tempfile::tempdir()?;
    let (mut document, source_fixture) = sample_document()?;
    document.preview_name("Saved face")?;
    document.finish_edit()?;
    document.preview_opacity(0.35)?;
    // Saving also commits an active gesture.
    let receipt = document.save(&output.path().join("model.kasane"))?;
    assert!(!document.modified());
    let history = document.history_lengths();
    let snapshot = document.snapshot()?;
    let id = document.selected_id().unwrap().to_owned();
    document.select(&id)?;
    assert!(!document.modified());
    assert_eq!(document.history_lengths(), history);
    document.undo()?;
    assert!(document.modified());
    assert_eq!(document.selected_layer().unwrap().opacity, 1.);
    document.redo()?;
    document.save(&receipt.manifest)?;
    drop(document);
    drop(source_fixture); // Reopening must use the copied asset, not the test fixture.
    let mut reopened = Document::open(&receipt.manifest)?;
    assert_eq!(reopened.selected_layer().unwrap().name, "Saved face");
    assert_eq!(reopened.selected_layer().unwrap().opacity, 0.35);
    assert_eq!(reopened.history_lengths(), (0, 0));
    assert!(!reopened.modified());
    assert_eq!(
        reopened.snapshot()?.textures[0].rgba.as_ref(),
        &[100, 205, 230, 255]
    );
    assert_eq!(snapshot.frame.drawables[0].opacity, 0.35);
    assert_eq!(snapshot.textures[0].rgba.as_ref(), &[100, 205, 230, 255]);
    Ok(())
}

#[test]
fn failed_save_keeps_live_document_and_bad_open_does_not_require_replacing_it() -> anyhow::Result<()>
{
    let output = tempfile::tempdir()?;
    let (mut document, _source) = sample_document()?;
    let receipt = document.save(&output.path().join("existing"))?;
    let (mut other, _other_source) = sample_document()?;
    other.preview_name("Unsaved")?;
    other.finish_edit()?;
    let original_path = other.path().unwrap().to_path_buf();
    assert!(other.save(&receipt.manifest).is_err());
    assert!(other.modified());
    assert_eq!(other.path(), Some(original_path.as_path()));
    assert_eq!(other.selected_layer().unwrap().name, "Unsaved");
    let broken = output.path().join("broken.json");
    std::fs::write(&broken, b"not a project")?;
    assert!(Document::open(&broken).is_err());
    assert!(!document.modified());
    assert_eq!(document.snapshot()?.frame.drawables.len(), 1);
    Ok(())
}

#[test]
fn opening_multiple_layers_edits_only_the_selected_mesh() -> anyhow::Result<()> {
    use kasane_core::{Canvas, Vec2};
    use kasane_sdk::{AuthoringSession, rectangle_mesh};
    use uuid::Uuid;

    let output = tempfile::tempdir()?;
    let (mut source_document, _source) = sample_document()?;
    let receipt = source_document.save(&output.path().join("multiple"))?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(512., 256., Vec2::new(128., 128.), 2.),
    )?;
    session.open_project(&receipt.manifest, None)?;
    let first = session.mesh_ids()[0].clone();
    let second = Uuid::new_v4().to_string();
    let mesh = rectangle_mesh(
        &second,
        "Second",
        &session.asset_ids()[0],
        Vec2::new(20., 20.),
        Vec2::new(80., 80.),
    )?;
    session.edit("second layer", None, |edit| edit.create_mesh(mesh))?;
    session.save_project(&receipt.manifest, None)?;

    let mut document = Document::open(&receipt.manifest)?;
    assert_eq!(document.layers().len(), 2);
    document.select(&second)?;
    document.preview_name("Edited second")?;
    document.preview_opacity(0.2)?; // Changing gesture commits the rename first.
    document.select(&first)?; // Changing selection commits the opacity gesture.
    assert_eq!(document.selected_layer().unwrap().name, "Diamond");
    assert_eq!(document.selected_layer().unwrap().opacity, 1.);
    assert_eq!(document.history_lengths(), (2, 0));
    let frame = document.snapshot()?;
    assert_eq!(
        frame
            .frame
            .drawables
            .iter()
            .find(|d| d.id == second)
            .unwrap()
            .opacity,
        0.2
    );
    document.save(&receipt.manifest)?;
    let mut reopened = Document::open(&receipt.manifest)?;
    reopened.select(&second)?;
    assert_eq!(reopened.selected_layer().unwrap().name, "Edited second");
    assert_eq!(reopened.selected_layer().unwrap().opacity, 0.2);
    Ok(())
}
