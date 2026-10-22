use editor_core::{Document, ObjectKind, ObjectRef};
use kasane_sdk::{DocumentField as F, DocumentValue as V, ResourceEdit as R};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
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
    let reopened = Document::open(&saved.manifest)?;
    assert!(
        reopened
            .object_index()
            .get(&ObjectRef::new(ObjectKind::Texture, &asset))
            .unwrap()
            .properties["content"]
            .is_null()
    );
    Ok(())
}
#[test]
fn resource_content_publication_and_history_refresh_textures_and_metadata() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let asset = d.resource_collection()?[0].id.clone();
    let before = d.snapshot()?;
    let content: kasane_core::ImageAsset = serde_json::from_value(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Texture, &asset))
            .unwrap()
            .properties
            .clone(),
    )?;
    let content = content.content;
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
    d.replace_image_content(&token, None)?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Texture, &asset))
            .unwrap()
            .diagnostic
            .is_some()
    );
    d.undo()?;
    assert!(
        !d.object_index()
            .get(&ObjectRef::new(ObjectKind::Texture, &asset))
            .unwrap()
            .properties["content"]
            .is_null()
    );
    assert!(d.replace_image_content(&token, content).is_err());
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
    editor.new_document()?;
    assert!(editor.replace_image_content(first, &token, None).is_err());
    editor.activate(first)?;
    editor.replace_image_content(first, &token, None)?;
    Ok(())
}
