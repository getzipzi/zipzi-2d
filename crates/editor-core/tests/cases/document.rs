use super::support;
use editor_core::{Document, TreeNodeKind};

#[test]
fn cached_dirty_state_tracks_preview_revert_save_and_history() -> anyhow::Result<()> {
    let (mut document, _source) = sample_document()?;
    let destination = tempfile::tempdir()?;
    let path = destination.path().join("model");
    document.save(&path)?;
    let name = document.selected_layer().unwrap().name;
    for _ in 0..2 {
        assert!(!document.modified());
        document.preview_name("Changed")?;
        assert!(document.modified());
        document.preview_name(&name)?;
        assert!(!document.modified());
        document.finish_edit()?;
        assert!(!document.modified());
    }
    document.preview_opacity(0.5)?;
    assert!(document.modified());
    document.preview_opacity(1.)?;
    assert!(!document.modified());
    document.preview_opacity(0.5)?;
    assert!(document.modified());
    document.cancel_edit();
    assert!(!document.modified());

    document.preview_name("Saved revision")?;
    document.finish_edit()?;
    assert!(document.modified());
    document.save(&path)?;
    assert!(!document.modified());
    document.undo()?;
    assert!(document.modified());
    document.redo()?;
    assert!(!document.modified());
    Ok(())
}
use support::sample_document;

#[test]
fn parameter_visuals_derive_keys_from_binding_and_selection() -> anyhow::Result<()> {
    let (document, _source) = support::sample_document_with_binding(true)?;
    let parameter = document.parameters().remove(0);
    let visual = &document.parameter_visuals()[&parameter.id];
    assert_eq!(visual.keys.len(), 2);
    assert_eq!(
        visual.keys.iter().map(|key| key.value).collect::<Vec<_>>(),
        vec![0., 1.]
    );
    assert!(visual.keys.iter().all(|key| key.selected));
    assert!(visual.keys.iter().all(|key| !key.base));
    Ok(())
}

#[test]
fn parameter_visuals_include_scene_binding_keyforms() -> anyhow::Result<()> {
    use kasane_core::{
        BindingAxis, Canvas, Parameter, Part, PartKeyform, SceneBinding, SceneTrack, Vec2,
    };
    use kasane_sdk::AuthoringSession;
    use uuid::Uuid;

    let directory = tempfile::tempdir()?;
    let parameter_id = Uuid::new_v4().to_string();
    let part_id = Uuid::new_v4().to_string();
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.edit("Scene keys", None, |edit| {
        edit.create_parameter(Parameter {
            id: parameter_id.clone(),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        edit.create_part(Part {
            id: part_id.clone(),
            ..Default::default()
        })?;
        edit.create_scene_binding(SceneBinding {
            id: Uuid::new_v4().to_string(),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: parameter_id.clone(),
                keys: vec![0., 1.],
            }],
            track: SceneTrack::Part {
                target_id: part_id.clone().into(),
                keyforms: vec![
                    PartKeyform {
                        samples: Default::default(),
                        keys: vec![0.],
                        draw_order: 0.,
                    },
                    PartKeyform {
                        samples: Default::default(),
                        keys: vec![1.],
                        draw_order: 1.,
                    },
                ],
            },
        })?;
        Ok(())
    })?;
    let receipt = session.save_project(&directory.path().join("scene.kasane"), None)?;
    let document = Document::open(&receipt.manifest)?;
    let visual = &document.parameter_visuals()[&parameter_id];
    assert_eq!(
        visual.keys.iter().map(|key| key.value).collect::<Vec<_>>(),
        vec![0., 1.]
    );
    assert!(visual.keys.iter().all(|key| !key.selected));
    let selected = document.parameter_visuals_for_targets(&[part_id.clone()]);
    assert!(
        selected[&parameter_id]
            .keys
            .iter()
            .all(|key| key.selected && !key.partial && key.targets == vec![part_id.clone()])
    );
    Ok(())
}

#[test]
fn batch_parameter_preview_is_atomic_and_updates_both_values() -> anyhow::Result<()> {
    let (mut document, _source) = support::sample_document_with_parameters(2)?;
    let before = document.parameters();
    assert!(
        document
            .set_preview_parameters(&[(before[0].id.clone(), 0.3), (before[1].id.clone(), 1.3)])
            .is_err()
    );
    assert_eq!(document.parameters(), before);

    document.set_preview_parameters(&[(before[0].id.clone(), 0.3), (before[1].id.clone(), 0.7)])?;
    let after = document.parameters();
    assert_eq!(after[0].value, 0.3);
    assert_eq!(after[1].value, 0.7);
    Ok(())
}

#[test]
fn blank_document_starts_clean_and_can_be_saved() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    assert!(document.path().is_none());
    assert!(document.layers().is_empty());
    assert!(document.selected_layer().is_none());
    assert_eq!(document.history_lengths(), (0, 0));
    assert!(!document.modified());
    let snapshot = document.snapshot()?;
    assert!(snapshot.viewport_frame.drawables.is_empty());
    assert!(snapshot.textures.is_empty());

    let output = tempfile::tempdir()?;
    let receipt = document.save(&output.path().join("blank.kasane"))?;
    assert!(!document.modified());
    let mut reopened = Document::open(&receipt.manifest)?;
    assert!(reopened.layers().is_empty());
    assert!(reopened.snapshot()?.viewport_frame.drawables.is_empty());
    Ok(())
}

#[test]
fn sdk_imports_create_editable_documents_with_textures() -> anyhow::Result<()> {
    let sdk_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model3 = sdk_root.join("tests/fixtures/external_v50/model.model3.json");
    let (mut model, notices) = Document::import_model3(&model3)?;
    assert!(!model.layers().is_empty());
    assert!(model.path().is_none());
    assert_no_selection(&model);
    assert_eq!(model.import_name(), Some("external_v50"));
    assert!(!model.snapshot()?.textures.is_empty());
    assert!(!notices.is_empty()); // Missing optional model3 attachments are reported.
    let output = tempfile::tempdir()?;
    model.save(&output.path().join("model.kasane"))?;
    assert!(!model.modified());

    let psd = sdk_root.join("crates/kasane-psd/tests/fixtures/layered.psd");
    let (mut psd_document, notices) = Document::import_psd(&psd)?;
    assert!(!psd_document.layers().is_empty());
    assert!(psd_document.path().is_none());
    assert_no_selection(&psd_document);
    assert!(psd_document.modified());
    assert_eq!(psd_document.import_name(), Some("layered"));
    assert!(notices.is_empty());
    let imported = psd_document.snapshot()?;
    assert!(!imported.textures.is_empty());
    let layer = psd_document.layers()[0].id.clone();
    psd_document.select(&layer)?;
    psd_document.preview_name("Edited PSD layer")?;
    let destination = output.path().join("layers.kasane");
    let receipt = psd_document.save(&destination)?;
    assert!(!psd_document.modified());
    assert_eq!(psd_document.history_lengths(), (1, 0));
    psd_document.undo()?;
    assert_eq!(psd_document.selected_layer().unwrap().name, "face");
    psd_document.redo()?;
    let mut reopened = Document::open(&receipt.manifest)?;
    assert_no_selection(&reopened);
    reopened.select(&layer)?;
    assert_eq!(reopened.selected_layer().unwrap().name, "Edited PSD layer");
    assert_eq!(
        reopened.snapshot()?.textures[0].rgba,
        imported.textures[0].rgba
    );
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
    assert_eq!(
        document.snapshot()?.viewport_frame.drawables[0].opacity,
        0.4
    );
    assert_eq!(original.viewport_frame.drawables[0].opacity, 1.);
    document.finish_edit()?;
    assert_eq!(document.history_lengths(), (history.0 + 1, 0));
    document.undo()?;
    assert_eq!(document.selected_layer().unwrap().opacity, 1.);
    assert_eq!(document.snapshot()?.viewport_frame.drawables[0].opacity, 1.);
    document.redo()?;
    assert_eq!(document.selected_layer().unwrap().opacity, 0.4);
    Ok(())
}

#[test]
fn cancelled_and_unchanged_gestures_preserve_history_and_redo() -> anyhow::Result<()> {
    let (mut document, _source) = sample_document()?;
    document.set_object_visible(&document.context().selection.clone().unwrap(), false)?;
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
    document.set_object_visible(&document.context().selection.clone().unwrap(), false)?;
    assert!(
        document
            .snapshot()?
            .viewport_frame
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
    assert_no_selection(&reopened);
    reopened.select(&id)?;
    assert_eq!(reopened.selected_layer().unwrap().name, "Saved face");
    assert_eq!(reopened.selected_layer().unwrap().opacity, 0.35);
    assert_eq!(reopened.history_lengths(), (0, 0));
    assert!(!reopened.modified());
    assert_eq!(
        reopened.snapshot()?.textures[0].rgba.as_ref(),
        &[100, 205, 230, 255]
    );
    assert_eq!(snapshot.viewport_frame.drawables[0].opacity, 0.35);
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
    assert_eq!(document.snapshot()?.viewport_frame.drawables.len(), 1);
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
    assert_no_selection(&document);
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
            .viewport_frame
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

fn assert_no_selection(document: &Document) {
    assert!(document.selected_id().is_none());
    assert!(document.selected_layer().is_none());
    assert!(document.context().selection.is_none());
    assert!(document.context().selections.is_empty());
    assert!(document.context().inspected.is_none());
}

#[test]
fn parts_and_deformer_trees_follow_their_distinct_parent_links() -> anyhow::Result<()> {
    use kasane_core::{
        Canvas, Part, RotationTransform, Transform, TransformData, Vec2, WarpTransform,
    };
    use kasane_sdk::AuthoringSession;
    use uuid::Uuid;

    let output = tempfile::tempdir()?;
    let (mut source, _texture) = support::sample_document_with_two_layers()?;
    let receipt = source.save(&output.path().join("trees"))?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.open_project(&receipt.manifest, None)?;
    let mesh = session.mesh_ids()[0].clone();
    let unassigned_mesh = session.mesh_ids()[1].clone();
    let root_part = Uuid::new_v4().to_string();
    let child_part = Uuid::new_v4().to_string();
    let rotation = Uuid::new_v4().to_string();
    let warp = Uuid::new_v4().to_string();
    session.edit("tree relationships", None, |edit| {
        edit.create_part(Part {
            id: root_part.clone(),
            name: "Head".into(),
            ..Default::default()
        })?;
        edit.create_part(Part {
            id: child_part.clone(),
            name: "Eye".into(),
            ..Default::default()
        })?;
        edit.set_organization_parent(&child_part, &root_part)?;
        edit.create_transform(Transform {
            id: rotation.clone(),
            name: "Head turn".into(),
            part_id: Some(root_part.clone().into()),
            data: TransformData::Rotation(RotationTransform::default()),
            ..Default::default()
        })?;
        edit.create_transform(Transform {
            id: warp.clone(),
            name: "Eye bend".into(),
            part_id: Some(child_part.clone().into()),
            parent_id: Some(rotation.clone().into()),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(0., 1.),
                    Vec2::new(1., 1.),
                ],
            }),
            ..Default::default()
        })?;
        edit.set_mesh_part(&mesh, &child_part)?;
        edit.set_deform_parent(&mesh, &warp)?;
        Ok(())
    })?;
    session.save_project(&receipt.manifest, None)?;

    let document = Document::open(&receipt.manifest)?;
    let parts = document.mesh_tree();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].id, root_part);
    assert_eq!(parts[0].kind, TreeNodeKind::Part);
    assert_eq!(parts[0].children[0].id, child_part);
    assert_eq!(parts[0].children[0].children[0].id, warp);
    assert_eq!(parts[0].children[0].children[1].id, mesh);
    assert_eq!(parts[0].children[1].id, rotation);
    assert_eq!(parts[1].kind, TreeNodeKind::Group);
    assert_eq!(parts[1].children[0].id, unassigned_mesh);

    let deformers = document.deformer_tree();
    assert_eq!(deformers.len(), 2);
    assert_eq!(deformers[0].id, rotation);
    assert_eq!(deformers[0].kind, TreeNodeKind::RotationDeformer);
    assert_eq!(deformers[0].children[0].id, warp);
    assert_eq!(deformers[0].children[0].kind, TreeNodeKind::WarpDeformer);
    assert_eq!(deformers[0].children[0].children[0].id, mesh);
    assert_eq!(deformers[1].kind, TreeNodeKind::Group);
    assert_eq!(deformers[1].children[0].id, unassigned_mesh);
    Ok(())
}

#[test]
fn key_coverage_distinguishes_partial_selection_from_unselected_targets() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document_with_bound_and_unbound_layers()?;
    let ids: Vec<_> = document
        .layers()
        .iter()
        .map(|layer| layer.id.clone())
        .collect();
    let parameter = document.parameters().remove(0);
    let visuals = document.parameter_visuals_for_targets(&ids);
    for key in &visuals[&parameter.id].keys {
        assert!(key.selected && key.partial);
        assert_eq!(key.targets.len(), 1);
    }
    let bound = visuals[&parameter.id].keys[0].targets[0].clone();
    let unbound = ids.into_iter().find(|id| id != &bound).unwrap();
    let visuals = document.parameter_visuals_for_targets(&[unbound]);
    assert!(
        visuals[&parameter.id]
            .keys
            .iter()
            .all(|key| !key.selected && !key.partial)
    );
    // Duplicate IDs in a selection must not incorrectly create partial coverage.
    let visuals = document.parameter_visuals_for_targets(&[bound.clone(), bound]);
    assert!(
        visuals[&parameter.id]
            .keys
            .iter()
            .all(|key| key.selected && !key.partial)
    );
    Ok(())
}

#[test]
fn unused_blend_key_table_does_not_claim_bound_keyforms() -> anyhow::Result<()> {
    use kasane_core::{BlendShapeKeyTable, Canvas, Parameter, ParameterKind, Vec2};
    use kasane_sdk::AuthoringSession;
    use uuid::Uuid;
    let assets = tempfile::tempdir()?;
    let parameter_id = Uuid::new_v4().to_string();
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.edit("Unused blend table", None, |edit| {
        edit.create_parameter(Parameter {
            id: parameter_id.clone(),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
        edit.create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: Uuid::new_v4().to_string(),
            parameter_id: parameter_id.clone(),
            keys: vec![0., 1.],
            base_key_idx: 0,
        })?;
        Ok(())
    })?;
    let saved = session.save_project(&assets.path().join("unused.kasane"), None)?;
    let doc = Document::open(&saved.manifest)?;
    assert!(doc.parameter_visuals()[&parameter_id].keys.is_empty());
    Ok(())
}
