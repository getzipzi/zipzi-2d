use zipzi_contract::kernel_commands::Command;
use zipzi_core::{
    document::{DeletionRequest, DeletionTarget},
    Vec2,
};
use zipzi_runtime::*;

fn id(n: u32) -> String {
    format!("40000000-0000-4000-8000-{n:012x}")
}

fn fixture(limits: RuntimeLimits) -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::new(limits);
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |c| {
            c.create_empty_asset(id(4), "Texture".into())?;
            for n in [2, 3] {
                let mut mesh = zipzi_sdk::rectangle_mesh(
                    &id(n),
                    "Mesh",
                    &id(4),
                    Vec2::default(),
                    Vec2::new(1., 1.),
                )?;
                if n == 3 {
                    mesh.masks = vec![id(2)];
                }
                c.create_mesh(mesh)?;
            }
            Ok(())
        })
        .unwrap();
    let version = rt.publish(client, draft, "Fixture").unwrap().version;
    (rt, client, version)
}

fn delete_mesh() -> DeletionRequest {
    DeletionRequest {
        roots: vec![DeletionTarget::Mesh(id(2))],
        ..Default::default()
    }
}

#[test]
fn reviewed_candidate_has_one_owner_and_cannot_publish_after_the_source_changes() {
    let (mut rt, owner, version) = fixture(Default::default());
    let other = rt.register_client().unwrap();
    rt.attach(other, version.session).unwrap();
    let draft = rt.begin_draft(owner, version).unwrap();
    let (draft, review) = rt
        .preview(owner, draft, |c| {
            c.review_deletion(&delete_mesh(), Default::default(), &Default::default())
        })
        .unwrap();
    assert!(review.report.requires_confirmation());
    assert!(review
        .report
        .rewritten
        .iter()
        .any(|row| row.owner.id == id(3)));
    assert_eq!(rt.read_draft(other, draft).unwrap_err().code, "NOT_OWNER");
    assert_eq!(
        rt.publish(other, draft, "Foreign").unwrap_err().code,
        "NOT_OWNER"
    );
    let reviewed = rt.read_draft(owner, draft).unwrap();
    assert!(reviewed.snapshot.document().mesh(&id(2)).is_none());
    let competing = rt.begin_draft(other, version).unwrap();
    let (competing, ()) = rt
        .batch(other, competing, |c| {
            c.rename(zipzi_sdk::NameTarget::Mesh(&id(3)), "New".into())
        })
        .unwrap();
    let published = rt.publish(other, competing, "Rename").unwrap();
    assert_eq!(
        rt.publish(owner, draft, "Stale deletion").unwrap_err().code,
        "STALE_SOURCE"
    );
    assert!(reviewed.snapshot.document().mesh(&id(2)).is_none());
    let formal = rt.subscribe(owner, version.session).unwrap();
    assert_eq!(formal.cursor, published.version);
    assert!(formal.snapshot.snapshot.document().mesh(&id(2)).is_some());
}

#[test]
fn worker_deletion_publishes_once_and_blocked_review_keeps_the_candidate() {
    let (mut rt, client, version) = fixture(Default::default());
    let draft = rt.begin_draft(client, version).unwrap();
    let job = rt
        .command_job(
            client,
            draft,
            vec![Command::Delete {
                request: delete_mesh(),
            }],
        )
        .unwrap();
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    let Some(Ok(JobValue::DraftUpdated(draft))) = rt.job(client, job).unwrap().result else {
        panic!("expected prepared draft")
    };
    rt.release_job(client, job).unwrap();
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
    let published = rt.publish(client, draft, "Delete").unwrap();
    assert_eq!(published.history.undo_steps, 2);
    let formal = rt.read_snapshot(client, version.session).unwrap();
    assert!(formal.snapshot.document().mesh(&id(2)).is_none());
    assert!(formal
        .snapshot
        .document()
        .mesh(&id(3))
        .unwrap()
        .masks
        .is_empty());
    let undone = rt.undo(client, published.version).unwrap();
    let draft = rt.begin_draft(client, undone.version).unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |c| {
            let mut state = c.read_document()?.editor_state().clone();
            state.objects.entry(id(3)).or_default().locked = true;
            c.replace_editor_state(state)
        })
        .unwrap();
    let locked = rt.publish(client, draft, "Lock").unwrap();
    let draft = rt.begin_draft(client, locked.version).unwrap();
    let (draft, review) = rt
        .preview(client, draft, |c| {
            c.review_deletion(&delete_mesh(), Default::default(), &Default::default())
        })
        .unwrap();
    assert!(review
        .report
        .blockers
        .iter()
        .any(|blocker| blocker.source_id == id(3)));
    assert!(rt
        .read_draft(client, draft)
        .unwrap()
        .snapshot
        .document()
        .mesh(&id(2))
        .is_some());
    assert_eq!(
        rt.commands(
            client,
            draft,
            vec![Command::Delete {
                request: delete_mesh()
            }]
        )
        .unwrap_err()
        .code,
        "OBJECT_LOCKED"
    );
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        locked.version
    );
}

#[test]
fn cancelled_and_over_budget_deletion_jobs_publish_nothing() {
    let (mut rt, client, version) = fixture(RuntimeLimits {
        task_bytes: 1024,
        ..Default::default()
    });
    let draft = rt.begin_draft(client, version).unwrap();
    let result = rt.request_batch(
        client,
        draft,
        TaskBudget {
            workspace_bytes: 1025,
            result_bytes: 0,
        },
        |_, _| panic!("over budget"),
    );
    assert_eq!(result.unwrap_err().code, "BUDGET_EXCEEDED");
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    let job = rt
        .request_batch(
            client,
            draft,
            TaskBudget {
                workspace_bytes: 1024,
                result_bytes: 0,
            },
            |c, cancel| {
                c.edit_deletion(
                    &delete_mesh(),
                    Default::default(),
                    &cancel.conversion_control(),
                )?;
                Ok(())
            },
        )
        .unwrap();
    let work = rt.take_work(job).unwrap();
    rt.cancel_job(client, job).unwrap();
    rt.complete(work.run()).unwrap();
    assert_eq!(rt.job(client, job).unwrap().state, JobState::Cancelled);
    rt.release_job(client, job).unwrap();
    assert_eq!(rt.task_bytes(), 0);
    let formal = rt.subscribe(client, version.session).unwrap();
    assert_eq!(formal.cursor, version);
    assert!(formal.snapshot.snapshot.document().mesh(&id(2)).is_some());
}

#[test]
fn deletion_command_requires_explicit_resolution_for_referenced_texture() {
    let (mut rt, client, version) = fixture(Default::default());
    let mut request = DeletionRequest {
        roots: vec![DeletionTarget::Texture(id(4))],
        ..Default::default()
    };
    let draft = rt.begin_draft(client, version).unwrap();
    let result = rt.commands(
        client,
        draft,
        vec![Command::Delete {
            request: request.clone(),
        }],
    );
    assert_eq!(result.unwrap_err().code, "DELETION_DECISION_REQUIRED");
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
    request.texture_replacements.insert(id(4), None);
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![Command::Delete { request }])
        .unwrap();
    let published = rt.publish(client, draft, "Remove texture").unwrap();
    assert_eq!(published.history.undo_steps, 2);
    let source = rt.read_snapshot(client, version.session).unwrap();
    assert!(source.snapshot.document().asset(&id(4)).is_none());
    assert!(source
        .snapshot
        .document()
        .mesh(&id(3))
        .unwrap()
        .texture_asset_id
        .is_empty());
}
