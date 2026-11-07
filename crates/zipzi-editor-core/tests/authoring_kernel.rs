//! Native consumers of the resident Runtime and exact Draft previews.
use zipzi_contract::document::{SceneCreate, SceneEdit, SceneField, SceneFieldValue};
use zipzi_editor_core::{Document, ObjectKind, ObjectRef};
use zipzi_sdk::{PublicationKind, QuerySource};

fn scene(id: &str, name: &str) -> SceneEdit {
    SceneEdit::Create {
        id: id.into(),
        name: name.into(),
        object: SceneCreate::Rotation { origin: None },
        part_id: None,
        parent_id: None,
        before: None,
        deformation_before: None,
    }
}
fn version(document: &Document) -> zipzi_sdk::Version {
    let host = document.runtime_host();
    host.read()
        .session_read(host.client_id(), document.runtime_session_id())
        .unwrap()
        .version()
}

#[test]
fn confirmation_publishes_the_displayed_candidate_once() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let id = uuid::Uuid::new_v4().to_string();
    let plan = document.plan_scene_authoring(scene(&id, "rotation"))?;
    let cloned = plan.clone();
    let content = plan.snapshot().snapshot.content_id();
    assert!(
        document
            .object_index()
            .get(&ObjectRef::new(ObjectKind::Deformer, &id))
            .is_none()
    );
    let receipt = document.execute_scene_authoring(&plan)?.receipt;
    assert_eq!(receipt.kind, PublicationKind::Commit);
    let host = document.runtime_host();
    assert_eq!(
        host.read()
            .session_read(host.client_id(), document.runtime_session_id())?
            .snapshot()
            .content_id(),
        content
    );
    assert!(document.execute_scene_authoring(&cloned).is_err());
    assert!(
        document
            .object_index()
            .get(&ObjectRef::new(ObjectKind::Deformer, &id))
            .is_some()
    );
    document.undo()?;
    assert!(
        document
            .object_index()
            .get(&ObjectRef::new(ObjectKind::Deformer, &id))
            .is_none()
    );
    document.redo()?;
    assert!(
        document
            .object_index()
            .get(&ObjectRef::new(ObjectKind::Deformer, &id))
            .is_some()
    );
    Ok(())
}

#[test]
fn returning_to_equal_content_does_not_revive_a_field_capture() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let id = uuid::Uuid::new_v4().to_string();
    let plan = document.plan_scene_authoring(scene(&id, "rotation"))?;
    document.execute_scene_authoring(&plan)?;
    let field = document.scene_field_address(&id, SceneField::Label)?;
    let rename =
        document.plan_scene_field_patch(&field, SceneFieldValue::Label("renamed".into()), false)?;
    document.execute_scene_authoring(&rename)?;
    document.undo()?;
    assert_ne!(field.source, version(&document));
    assert!(
        document
            .plan_scene_field_patch(&field, SceneFieldValue::Label("stale".into()), false)
            .is_err()
    );
    Ok(())
}

#[test]
fn save_admission_preserves_a_confirmation_for_retry() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let id = uuid::Uuid::new_v4().to_string();
    let plan = document.plan_scene_authoring(scene(&id, "rotation"))?;
    let host = document.runtime_host();
    let client = host.client_id();
    let io = host.read().project_io().clone();
    let directory = tempfile::tempdir()?;
    let expected = version(&document);
    let job = host.write().request_save_as(
        client,
        expected,
        &io,
        &directory.path().join("retry.zipzi"),
        None,
        Default::default(),
    )?;
    let work = host.write().take_work(job)?;
    assert!(document.execute_scene_authoring(&plan).is_err());
    host.write()
        .complete(work.run())
        .map_err(|_| anyhow::anyhow!("wrong host"))?;
    host.write().release_job(client, job)?;
    assert_eq!(
        document.execute_scene_authoring(&plan)?.receipt.kind,
        PublicationKind::Commit
    );
    Ok(())
}

#[test]
fn deletion_worker_preserves_live_content_until_confirmation() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let id = uuid::Uuid::new_v4().to_string();
    let create = document.plan_scene_authoring(scene(&id, "rotation"))?;
    document.execute_scene_authoring(&create)?;
    let object = ObjectRef::new(ObjectKind::Deformer, &id);
    let intent = document.deletion_intent(std::slice::from_ref(&object))?;
    let original = version(&document);
    let task = document.start_tree_deletion(intent)?;
    assert_eq!(version(&document), original);
    let plan = std::thread::spawn(move || task.run())
        .join()
        .unwrap()
        .finish()?;
    assert_eq!(version(&document), original);
    assert!(plan.removed_ids().contains(&id));
    document.execute_tree_deletion(&plan)?;
    assert!(document.object_index().get(&object).is_none());
    document.undo()?;
    assert!(document.object_index().get(&object).is_some());
    let task = document.start_tree_deletion(document.deletion_intent(&[object])?)?;
    let cancellation = task.cancellation();
    cancellation.cancel();
    assert!(task.run().finish().is_err());
    assert_eq!(document.runtime_host().read().outstanding_jobs(), 0);
    Ok(())
}

#[test]
fn resource_candidate_retains_pixels_through_save_and_undo() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let host = document.runtime_host();
    let id = uuid::Uuid::new_v4().to_string();
    let image = zipzi_project::PreparedImageContent::from_rgba(
        1,
        1,
        vec![40, 80, 120, 255].into(),
        host.read().resources().budget(),
    )?;
    document.import_prepared_texture(id.clone(), "image".into(), &image)?;
    let captured = document.image_load_token(&id)?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("image.zipzi");
    document.save(&path)?;
    let opened = Document::open(&path)?;
    let reader = opened.runtime_host();
    let expected = version(&opened);
    let snapshot = reader
        .read()
        .read_snapshot(reader.client_id(), expected.session)?;
    assert_eq!(snapshot.source, QuerySource::Published(expected));
    let asset = snapshot.snapshot.document().asset(&id).unwrap();
    assert_eq!(
        snapshot
            .snapshot
            .resources()
            .read_texture(asset)?
            .rgba
            .as_ref(),
        &[40, 80, 120, 255]
    );
    let renamed = document.plan_resource_name_patch(
        &zipzi_editor_core::FieldCapture {
            source: captured.source,
            id: id.clone(),
            owner: None,
            value: captured.value.name,
        },
        "new".into(),
    )?;
    document.execute_resource_authoring(&renamed)?;
    document.undo()?;
    assert_eq!(document.texture_info(&id)?.asset.name, "image");
    Ok(())
}

#[test]
fn background_host_write_waits_for_an_in_progress_read() {
    let document = Document::new_blank().unwrap();
    let host = document.runtime_host();
    let reader = host.read();
    let worker_host = host.clone();
    let (entered, entering) = std::sync::mpsc::channel();
    let (done, finished) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        entered.send(()).unwrap();
        let client = worker_host.client_id();
        let session = worker_host
            .write()
            .create_session(
                client,
                &uuid::Uuid::new_v4().to_string(),
                Default::default(),
            )
            .unwrap();
        done.send(session).unwrap();
    });
    entering.recv().unwrap();
    assert!(finished.try_recv().is_err());
    drop(reader);
    let session = finished
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    worker.join().unwrap();
    assert!(host.read().session_read(host.client_id(), session).is_ok());
}
