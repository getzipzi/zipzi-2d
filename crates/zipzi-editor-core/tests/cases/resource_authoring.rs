use zipzi_contract::document::{DocumentField as F, DocumentValue as V, ResourceEdit as R};
use zipzi_editor_core::{Document, ObjectKind, ObjectRef};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[test]
fn texture_assignment_replacement_delete_and_history_preserve_geometry_and_references()
-> anyhow::Result<()> {
    let (mut d, assets) = super::support::sample_document_with_two_layers()?;
    let source = d.resource_collection()?[0].id.clone();
    let mesh = ObjectRef::new(ObjectKind::Mesh, &d.layers()[0].id);
    let other = ObjectRef::new(ObjectKind::Mesh, &d.layers()[1].id);
    let original = d.object_index().get(&mesh).unwrap().properties.clone();
    let replacement = id();
    let p = d.plan_resource_authoring(R::Create {
        id: replacement.clone(),
        name: "Empty".into(),
        before: None,
    })?;
    d.execute_resource_authoring(&p)?;
    d.set_object_property(&mesh, "texture_choice", serde_json::json!(replacement))?;
    assert_eq!(d.texture_info(&replacement)?.users, [mesh.clone()]);
    assert_eq!(
        d.property_values(&mesh)?[&zipzi_editor_core::property::mesh::TEXTURE.key],
        zipzi_editor_core::PropertyValue::Text(replacement.clone())
    );
    // An empty asset retains evaluated vertices and the authoring topology.
    assert!(
        d.snapshot()?
            .runtime_frame
            .drawables
            .iter()
            .any(|m| m.id == mesh.id && !m.positions.is_empty())
    );
    let png = assets.path().join("replace.png");
    std::fs::write(&png, include_bytes!("../fixtures/accent.png"))?;
    let prepared = zipzi_project::prepare_image_content(&png)?;
    let token = d.image_load_token(&replacement)?;
    d.replace_prepared_image_content(&token, &prepared)?;
    d.set_object_property(&other, "texture_choice", serde_json::json!(replacement))?;
    let users = d.texture_info(&replacement)?.users;
    assert_eq!(users.len(), 2);
    let rejected = d
        .plan_resource_authoring(R::Delete {
            id: replacement.clone(),
            replacement: None,
        })
        .unwrap_err();
    let rejected = rejected.downcast_ref::<zipzi_sdk::Status>().unwrap();
    assert!(
        rejected.code.contains("REFERENCE") || rejected.code.contains("IN_USE"),
        "{rejected}"
    );
    let history = d.history_lengths();
    d.set_objects_locked(std::slice::from_ref(&mesh), true)?;
    assert!(
        d.plan_resource_authoring(R::Delete {
            id: replacement.clone(),
            replacement: Some(source.clone().into())
        })
        .is_err()
    );
    d.undo()?;
    assert_eq!(d.history_lengths().0, history.0);
    let p = d.plan_resource_authoring(R::Delete {
        id: replacement.clone(),
        replacement: Some(source.clone().into()),
    })?;
    d.execute_resource_authoring(&p)?;
    assert!(d.texture_info(&replacement).is_err());
    let now = d.object_index();
    assert_eq!(
        now.get(&mesh).unwrap().properties["texture_asset_id"],
        source
    );
    for field in ["vertices", "triangles", "appearance", "runtime_id"] {
        assert_eq!(
            now.get(&mesh).unwrap().properties[field],
            original[field],
            "{field}"
        );
    }
    d.undo()?;
    assert_eq!(d.texture_info(&replacement)?.users.len(), 2);
    assert!(
        d.snapshot()?
            .textures
            .iter()
            .any(|t| t.asset_id == replacement)
    );
    d.redo()?;
    let saved = d.save(&assets.path().join("texture-flow.zipzi"))?;
    let reopened = Document::open(&saved.path)?;
    assert_eq!(reopened.texture_info(&source)?.users.len(), 2);
    assert!(reopened.texture_info(&replacement).is_err());
    Ok(())
}

#[test]
fn direct_image_import_is_atomic_and_retains_pixels_for_history_and_save() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let dir = tempfile::tempdir()?;
    let png = dir.path().join("import.png");
    std::fs::write(&png, include_bytes!("../fixtures/accent.png"))?;
    let prepared = zipzi_project::prepare_image_content(&png)?;
    std::fs::remove_file(&png)?;
    let asset = id();
    d.import_prepared_texture(asset.clone(), "Imported".into(), &prepared)?;
    assert_eq!(d.history_lengths(), (1, 0));
    let pixels = d.snapshot()?.textures[0].rgba.clone();
    d.undo()?;
    assert!(d.resource_collection()?.is_empty());
    d.redo()?;
    assert_eq!(d.snapshot()?.textures[0].rgba, pixels);
    let before = d.history_lengths();
    assert!(
        d.import_prepared_texture(asset.clone(), "Duplicate ID".into(), &prepared)
            .is_err()
    );
    assert_eq!(d.history_lengths(), before);
    let saved = d.save(&dir.path().join("imported.zipzi"))?;
    let mut reopened = Document::open(&saved.path)?;
    assert_eq!(reopened.snapshot()?.textures[0].rgba, pixels);
    Ok(())
}

#[test]
fn cached_editor_preview_adopts_replaced_pixels_for_all_users_and_retains_name_only_frames()
-> anyhow::Result<()> {
    let (d, _assets) = super::support::sample_document_with_two_layers()?;
    let asset = d.resource_collection()?[0].id.clone();
    let mut editor = super::support::editor_from_document(d);
    super::support::attach_viewports(&mut editor);
    let document = editor.active_id().unwrap();
    let original = editor.preview()?.scene;
    let address = editor.document().unwrap().resource_name_address(&asset)?;
    let plan = editor
        .document()
        .unwrap()
        .plan_resource_name_patch(&address, "Renamed texture".into())?;
    editor.execute_resource_authoring(document, &plan)?;
    let named = editor.preview()?.scene;
    assert!(std::sync::Arc::ptr_eq(&original.textures, &named.textures));
    assert!(std::sync::Arc::ptr_eq(
        &original.runtime_frame,
        &named.runtime_frame
    ));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/zipzi-sdk/tests/fixtures/asymmetric-2x2.png");
    let image = zipzi_project::prepare_image_content(&path)?;
    let token = editor.image_load_token(document, &asset)?;
    editor.replace_prepared_image_content(document, &token, &image)?;
    let next = editor.preview()?.scene;
    assert_ne!(next.textures[0].cache_key, original.textures[0].cache_key);
    assert_eq!(next.runtime_frame.drawables.len(), 2);
    for mesh in &next.runtime_frame.drawables {
        assert_eq!(mesh.texture_asset_id, asset);
        assert!(mesh.renderable);
    }
    editor.execute(zipzi_editor_core::EditorCommand::Undo)?;
    assert_eq!(
        editor.preview()?.scene.textures[0].rgba,
        original.textures[0].rgba
    );
    editor.execute(zipzi_editor_core::EditorCommand::Redo)?;
    assert_eq!(
        editor.preview()?.scene.textures[0].rgba,
        next.textures[0].rgba
    );
    Ok(())
}

#[test]
fn image_captures_expire_after_publication_and_do_not_revive_on_undo() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document()?;
    let asset = d.resource_collection()?[0].id.clone();
    let capture = d.image_load_token(&asset)?;
    let pixels = zipzi_project::PreparedImageContent::from_rgba(
        1,
        1,
        vec![9, 8, 7, 255].into(),
        &Default::default(),
    )?;
    let address = d.resource_name_address(&asset)?;
    let plan = d.plan_resource_name_patch(&address, "Renamed".into())?;
    d.execute_resource_authoring(&plan)?;
    assert!(d.replace_prepared_image_content(&capture, &pixels).is_err());
    d.undo()?;
    assert!(d.replace_prepared_image_content(&capture, &pixels).is_err());

    let pending = id();
    let plan = d.plan_resource_authoring(R::Create {
        id: pending.clone(),
        name: "Pending".into(),
        before: None,
    })?;
    d.execute_resource_authoring(&plan)?;
    let token = d.image_load_token(&pending)?;
    let plan = d.plan_resource_authoring(R::Delete {
        id: pending.clone(),
        replacement: None,
    })?;
    d.execute_resource_authoring(&plan)?;
    d.undo()?;
    assert!(d.replace_prepared_image_content(&token, &pixels).is_err());
    let fresh = d.image_load_token(&pending)?;
    d.replace_prepared_image_content(&fresh, &pixels)?;
    Ok(())
}

#[test]
fn unreadable_image_content_does_not_publish_or_clear_redo() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let asset = d.resource_collection()?[0].id.clone();
    let address = d.resource_name_address(&asset)?;
    let p = d.plan_resource_name_patch(&address, "Temporary".into())?;
    d.execute_resource_authoring(&p)?;
    d.undo()?;
    let index = d.object_index();
    let history = d.history_lengths();
    let frame = d.snapshot()?;
    let temp = tempfile::tempdir()?;
    let missing = temp.path().join("missing.png");
    assert!(zipzi_project::prepare_image_content(&missing).is_err());
    assert_eq!(d.history_lengths(), history);
    assert_eq!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Texture, &asset))
            .unwrap()
            .properties,
        index
            .get(&ObjectRef::new(ObjectKind::Texture, &asset))
            .unwrap()
            .properties
    );
    assert_eq!(
        d.snapshot()?.runtime_frame.source_revision,
        frame.runtime_frame.source_revision
    );
    assert!(std::sync::Arc::ptr_eq(&index, &d.object_index()));
    Ok(())
}

#[test]
fn prepared_images_can_be_copied_without_the_source_file() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let asset = id();
    let copy = id();
    let p = d.plan_resource_authoring(R::Create {
        id: asset.clone(),
        name: String::new(),
        before: None,
    })?;
    d.execute_resource_authoring(&p)?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("image.png");
    std::fs::write(&path, include_bytes!("../fixtures/accent.png"))?;
    let image = zipzi_project::prepare_image_content(&path)?;
    std::fs::remove_file(path)?;
    let token = d.image_load_token(&asset)?;
    d.replace_prepared_image_content(&token, &image)?;
    let p = d.plan_resource_authoring(R::Duplicate {
        source: asset.clone(),
        id: copy.clone(),
        name: String::new(),
        before: None,
    })?;
    d.execute_resource_authoring(&p)?;
    let snapshot = d.snapshot()?;
    assert_eq!(snapshot.textures.len(), 2);
    assert!(std::sync::Arc::ptr_eq(
        &snapshot.textures[0].rgba,
        &snapshot.textures[1].rgba
    ));
    d.undo()?;
    assert_eq!(d.snapshot()?.textures.len(), 1);
    d.redo()?;
    assert_eq!(d.snapshot()?.textures.len(), 2);
    Ok(())
}
#[test]
fn pending_texture_is_inspectable_and_saveable_without_loading_fake_pixels() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let asset = id();
    let p = d.plan_resource_authoring(R::Create {
        id: asset.clone(),
        name: String::new(),
        before: None,
    })?;
    let r = d.execute_resource_authoring(&p)?;
    assert_eq!(
        r.suggested_selection,
        [ObjectRef::new(ObjectKind::Texture, &asset)]
    );
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Texture, &asset))
            .unwrap()
            .diagnostic
            .is_some()
    );
    assert_eq!(
        d.resource_collection()?,
        [ObjectRef::new(ObjectKind::Texture, &asset)]
    );
    let dir = tempfile::tempdir()?;
    let saved = d.save(&dir.path().join("resources"))?;
    let reopened = Document::open(&saved.path)?;
    assert!(matches!(
        reopened.texture_info(&asset)?.asset.content,
        zipzi_core::state::AssetContent::Empty
    ));
    Ok(())
}
#[test]
fn resource_content_publication_and_history_refresh_textures_and_metadata() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let asset = d.resource_collection()?[0].id.clone();
    let before = d.snapshot()?;
    let retained_index = d.object_index();
    let address = d.resource_name_address(&asset)?;
    let p = d.plan_resource_name_patch(&address, "Renamed".into())?;
    d.execute_resource_authoring(&p)?;
    let updated_index = d.object_index();
    assert!(std::ptr::eq(
        &retained_index.relations[0],
        &updated_index.relations[0]
    ));
    assert_eq!(
        before.runtime_frame.source_revision,
        d.snapshot()?.runtime_frame.source_revision
    );
    let token = d.image_load_token(&asset)?;
    let pixels = zipzi_project::PreparedImageContent::from_rgba(
        1,
        1,
        vec![9, 8, 7, 255].into(),
        &Default::default(),
    )?;
    d.replace_prepared_image_content(&token, &pixels)?;
    assert_eq!(&*d.snapshot()?.textures[0].rgba, [9, 8, 7, 255]);
    d.undo()?;
    assert_eq!(d.snapshot()?.textures[0].rgba, before.textures[0].rgba);
    assert!(d.replace_prepared_image_content(&token, &pixels).is_err());
    Ok(())
}
#[test]
fn document_layout_publication_updates_root_properties_and_preserves_frame_inputs()
-> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let frame = d.snapshot()?;
    let a = d.document_field_address(F::Layout)?;
    let p = d.plan_document_field_patch(
        &a,
        V::Layout(Some(std::collections::BTreeMap::from([(
            "CenterX".into(),
            3.,
        )]))),
    )?;
    d.execute_document_authoring(&p)?;
    let index = d.object_index();
    let doc = index
        .objects
        .iter()
        .find(|o| o.reference.kind == ObjectKind::Document)
        .unwrap();
    assert_eq!(doc.properties["model_settings"]["layout"]["CenterX"], 3.);
    assert_eq!(
        frame.runtime_frame.source_revision,
        d.snapshot()?.runtime_frame.source_revision
    );
    d.undo()?;
    assert!(d.plan_document_field_patch(&a, V::Layout(None)).is_err());
    Ok(())
}

#[test]
fn asynchronous_resource_completion_rejects_a_different_active_document() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let asset = id();
    let p = document.plan_resource_authoring(R::Create {
        id: asset.clone(),
        name: String::new(),
        before: None,
    })?;
    document.execute_resource_authoring(&p)?;
    let mut editor = super::support::editor_from_document(document);
    let first = editor.active_id().unwrap();
    let token = editor.image_load_token(first, &asset)?;
    let pixels = zipzi_project::PreparedImageContent::from_rgba(
        1,
        1,
        vec![9, 8, 7, 255].into(),
        &Default::default(),
    )?;
    editor.new_document()?;
    assert!(
        editor
            .replace_prepared_image_content(first, &token, &pixels)
            .is_err()
    );
    editor.activate(first)?;
    editor.replace_prepared_image_content(first, &token, &pixels)?;
    Ok(())
}

#[test]
fn resource_execution_rejects_timeline_gesture_without_consuming_plan() -> anyhow::Result<()> {
    let (document, _assets) = super::support::combined_animation_document()?;
    let mut editor = super::support::editor_from_document(document);
    let document_id = editor.active_id().unwrap();
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Animation)?;
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(document_id, motion)?;
    let asset = id();
    let plan = editor.plan_resource_authoring(
        document_id,
        R::Create {
            id: asset.clone(),
            name: "New".into(),
            before: None,
        },
    )?;
    editor.begin_timeline_edit(document_id, "Pending gesture")?;
    let history = editor.document().unwrap().history_lengths();
    let generation = editor.document().unwrap().model_generation();
    let error = editor
        .execute_resource_authoring(document_id, &plan)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Finish the active Timeline gesture first"
    );
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert_eq!(editor.document().unwrap().model_generation(), generation);
    editor.cancel_timeline_edit();
    let result = editor.execute_resource_authoring(document_id, &plan)?;
    assert!(result.receipt.kind != zipzi_sdk::PublicationKind::Unchanged);
    assert_eq!(
        result.suggested_selection,
        vec![ObjectRef::new(ObjectKind::Texture, &asset)]
    );
    assert!(editor.animation().unwrap().motion.is_some());
    Ok(())
}
