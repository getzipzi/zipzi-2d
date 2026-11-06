use zipzi_contract::{
    authoring::ObjectKind,
    kernel_commands::{Command, ImageInput, ObjectAddress},
};
use zipzi_runtime::*;

fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            Default::default(),
        )
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    (rt, client, version)
}

fn asset(id: &str) -> Command {
    Command::CreateAsset {
        id: id.into(),
        name: "Image".into(),
    }
}

fn image(id: &str, bytes: Vec<u8>) -> Command {
    Command::ReplaceImage {
        id: id.into(),
        input: ImageInput::Rgba {
            width: 1,
            height: 1,
            bytes,
        },
    }
}

fn rename(kind: ObjectKind, id: &str) -> Command {
    Command::Rename {
        target: ObjectAddress {
            kind,
            id: id.into(),
        },
        name: "New name".into(),
    }
}

#[test]
fn image_failure_aborts_prior_candidate_writes_for_inline_and_blob_inputs() {
    for from_blob in [false, true] {
        let (mut rt, client, version) = fixture(Default::default());
        let draft = rt.begin_draft(client, version).unwrap();
        let parameter = uuid::Uuid::new_v4().to_string();
        let id = uuid::Uuid::new_v4().to_string();
        let (draft, ()) = rt
            .batch(client, draft, |c| {
                c.create_parameter(zipzi_core::Parameter {
                    id: parameter.clone(),
                    runtime_id: parameter.clone(),
                    ..Default::default()
                })?;
                c.create_empty_asset(id.clone(), "Image".into())
            })
            .unwrap();
        let error = if from_blob {
            let blob = rt.create_blob(client, 3).unwrap();
            rt.append_blob(client, blob, 0, &[1, 2, 3]).unwrap();
            rt.seal_blob(client, blob).unwrap();
            let result = rt.replace_image_blob(client, draft, id, blob, 1, 1);
            rt.release_blob(client, blob).unwrap();
            result.unwrap_err()
        } else {
            rt.commands(client, draft, vec![image(&id, vec![1, 2, 3])])
                .unwrap_err()
        };
        assert_eq!(error.code, "INVALID_IMAGE");
        assert_eq!(
            rt.publish(client, draft, "Invalid image").unwrap_err().code,
            "DRAFT_ENDED"
        );
        let formal = rt.subscribe(client, version.session).unwrap();
        assert!(formal
            .snapshot
            .snapshot
            .document()
            .parameter(&parameter)
            .is_none());
        assert_eq!(formal.history.undo_steps, 0);
        assert_eq!(rt.resources().budget().usage().total_bytes(), 0);
    }
}

#[test]
fn batch_admission_preserves_the_draft_but_accepted_command_failure_aborts_all_writes() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let oversized = vec![rename(ObjectKind::Mesh, "missing"); 4097];
    assert_eq!(
        rt.commands(client, draft, oversized.clone())
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(
        rt.command_job(client, draft, oversized).unwrap_err().code,
        "RESOURCE_LIMIT"
    );
    assert!(rt.next_work().is_none());
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    let id = uuid::Uuid::new_v4().to_string();
    let commands = vec![asset(&id), rename(ObjectKind::Mesh, "missing")];
    assert!(rt.commands(client, draft, commands).is_err());
    assert_eq!(
        rt.publish(client, draft, "Rejected").unwrap_err().code,
        "DRAFT_ENDED"
    );
    let formal = rt.subscribe(client, version.session).unwrap();
    assert!(formal.snapshot.snapshot.document().asset(&id).is_none());
    assert_eq!(formal.history.undo_steps, 0);
}

#[test]
fn blob_and_image_leases_follow_independent_representations() {
    let (mut rt, client, version) = fixture(Default::default());
    let blob = rt.create_blob(client, 4).unwrap();
    let budget = rt.resources().budget().clone();
    assert_eq!(budget.usage().transfer_bytes, 4);
    assert_eq!(
        rt.append_blob(client, blob, 1, &[7, 0, 0, 255])
            .unwrap_err()
            .code,
        "INVALID_BLOB_RANGE"
    );
    rt.append_blob(client, blob, 0, &[7, 0, 0, 255]).unwrap();
    rt.seal_blob(client, blob).unwrap();
    let draft = rt.begin_draft(client, version).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (draft, _) = rt.commands(client, draft, vec![asset(&id)]).unwrap();
    let draft = rt
        .replace_image_blob(client, draft, id, blob, 1, 1)
        .unwrap();
    assert_eq!(budget.usage().rgba_bytes, 4);
    rt.release_blob(client, blob).unwrap();
    assert_eq!(budget.usage().transfer_bytes, 0);
    assert_eq!(budget.usage().rgba_bytes, 4);
    rt.cancel_draft(client, draft.draft).unwrap();
    assert_eq!(budget.usage().total_bytes(), 0);
}

#[test]
fn releasing_client_reclaims_bulk_buffers_and_unpublished_images_but_keeps_history() {
    let (mut rt, client, version) = fixture(Default::default());
    let id = uuid::Uuid::new_v4().to_string();
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(
            client,
            draft,
            vec![asset(&id), image(&id, vec![8, 0, 0, 255])],
        )
        .unwrap();
    let published = rt.publish(client, draft, "Image").unwrap();
    let draft = rt.begin_draft(client, published.version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![image(&id, vec![9, 0, 0, 255])])
        .unwrap();
    let peer = rt.register_client().unwrap();
    rt.attach(peer, version.session).unwrap();
    assert_eq!(
        rt.cancel_draft(peer, draft.draft).unwrap_err().code,
        "NOT_OWNER"
    );
    rt.create_blob(client, 4096).unwrap();
    let budget = rt.resources().budget().clone();
    assert_eq!(budget.usage().transfer_bytes, 4096);
    assert_eq!(budget.usage().rgba_bytes, 8);
    rt.release_client(client);
    assert_eq!(budget.usage().transfer_bytes, 0);
    assert_eq!(budget.usage().rgba_bytes, 4);
    let formal = rt.subscribe(peer, version.session).unwrap();
    let asset = formal.snapshot.snapshot.document().asset(&id).unwrap();
    assert_eq!(
        formal
            .snapshot
            .snapshot
            .resources()
            .read_texture(asset)
            .unwrap()
            .rgba[0],
        8
    );
    assert_eq!(formal.history.undo_steps, 1);
}

#[test]
fn batch_requests_cannot_be_replayed_at_another_draft_sequence() {
    let (mut rt, client, version) = fixture(Default::default());
    let first = rt.begin_draft(client, version).unwrap();
    let second = rt.begin_draft(client, version).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (current, _) = rt.commands(client, first, vec![asset(&id)]).unwrap();
    assert_eq!(
        rt.commands(client, second, vec![rename(ObjectKind::Asset, &id)])
            .unwrap_err()
            .code,
        "MISSING_OBJECT"
    );
    assert_eq!(
        rt.read_draft(client, current).unwrap().source,
        QuerySource::Draft(current)
    );
    assert_eq!(
        rt.commands(client, first, vec![rename(ObjectKind::Asset, &id)])
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let formal = rt.subscribe(client, version.session).unwrap();
    assert_eq!(formal.history.undo_steps, 0);
    assert!(formal.snapshot.snapshot.document().asset(&id).is_none());
}

#[test]
fn image_budget_refusal_does_not_publish_candidate_pixels() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        task_bytes: 3,
        ..Default::default()
    });
    let id = uuid::Uuid::new_v4().to_string();
    let draft = rt.begin_draft(client, version).unwrap();
    assert_eq!(
        rt.commands(
            client,
            draft,
            vec![asset(&id), image(&id, vec![0, 0, 0, 255])]
        )
        .unwrap_err()
        .code,
        "RESOURCE_BUDGET_EXCEEDED"
    );
    assert_eq!(rt.resources().budget().usage().total_bytes(), 0);
    assert!(rt
        .read_snapshot(client, version.session)
        .unwrap()
        .snapshot
        .document()
        .asset(&id)
        .is_none());
}

#[test]
fn worker_admission_refuses_before_taking_the_draft() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        task_bytes: 1024,
        ..Default::default()
    });
    let id = uuid::Uuid::new_v4().to_string();
    let draft = rt.begin_draft(client, version).unwrap();
    let result = rt.command_job(client, draft, vec![asset(&id)]);
    assert_eq!(result.unwrap_err().code, "BUDGET_EXCEEDED");
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    assert!(rt.next_work().is_none());
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(
        rt.subscribe(client, version.session)
            .unwrap()
            .history
            .undo_steps,
        0
    );
}
