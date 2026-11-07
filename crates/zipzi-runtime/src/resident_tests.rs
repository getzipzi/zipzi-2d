use super::*;
use std::{sync::mpsc, time::Duration};
use zipzi_core::Parameter;
use zipzi_sdk::{NameTarget, ResourceStore};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn parameter(n: u32) -> Parameter {
    Parameter {
        id: id(n),
        name: format!("p{n}"),
        ..Default::default()
    }
}
fn setup() -> (Runtime, ClientId, SessionKey) {
    let mut r = Runtime::default();
    let c = r.register_client().unwrap();
    let s = r.create_session(c, &id(1), ModelSetup::default()).unwrap();
    (r, c, s)
}

#[test]
fn glue_axes_and_samples_support_address_reads_and_pagination() {
    use zipzi_contract::{
        authoring::ObjectKind, kernel_commands::ObjectAddress, kernel_wire::Query,
    };
    use zipzi_core::{
        state::{DocumentState, OwnedDocument},
        BindingAxis, BindingAxisEditing, Glue, GlueBinding, GlueKeyform,
    };
    let state = DocumentState::from_owned(OwnedDocument {
        id: id(1),
        parameters: vec![Parameter {
            id: id(2),
            runtime_id: "P".into(),
            ..Default::default()
        }],
        glues: (3..5)
            .map(|n| Glue {
                id: id(n),
                runtime_id: format!("G{n}"),
                binding: Some(GlueBinding {
                    axes: vec![BindingAxis::new(id(2), vec![0.])],
                    keyforms: vec![GlueKeyform {
                        samples: vec![],
                        intensity: 1.,
                    }],
                }),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    })
    .unwrap();
    let axes: Vec<_> = (3..5)
        .map(|n| state.glue(&id(n)).unwrap().binding.as_ref().unwrap().axes[0].clone())
        .collect();
    let mut r = Runtime::default();
    let c = r.register_client().unwrap();
    let s = r
        .adopt_session(
            c,
            AuthoringSession::new(Snapshot::new(state, Default::default()).unwrap()),
        )
        .unwrap();
    let published = QuerySource::Published(version(&r, c, s));
    let candidate = QuerySource::Draft(draft(&mut r, c, s));
    for source in [published, candidate] {
        for (kind, expected) in [
            (
                ObjectKind::BindingAxis,
                axes.iter()
                    .map(|a| serde_json::to_value(a).unwrap())
                    .collect::<Vec<_>>(),
            ),
            (
                ObjectKind::BindingSample,
                axes.iter()
                    .map(|a| serde_json::to_value(&a.samples[0]).unwrap())
                    .collect(),
            ),
        ] {
            let addresses: Vec<_> = expected
                .iter()
                .map(|value| ObjectAddress {
                    kind,
                    id: value["id"].as_str().unwrap().into(),
                })
                .collect();
            let selected = r.query(c, source, Query::Objects { addresses }).unwrap();
            assert_eq!(selected["source"], serde_json::to_value(source).unwrap());
            for (offset, value) in expected.iter().enumerate() {
                let page = r
                    .query(
                        c,
                        source,
                        Query::Collection {
                            kind,
                            offset,
                            limit: 1,
                        },
                    )
                    .unwrap();
                assert_eq!(page["source"], selected["source"]);
                assert_eq!(page["content"], selected["content"]);
                assert_eq!(
                    page["value"]["records"][0],
                    selected["value"]["records"][offset]
                );
                assert_eq!(page["value"]["records"][0]["value"], *value);
                assert_eq!(
                    page["value"]["next_offset"],
                    if offset == 0 {
                        serde_json::json!(1)
                    } else {
                        serde_json::Value::Null
                    }
                );
            }
        }
    }
}

#[test]
fn metadata_query_bounds_all_roots_and_preserves_source() {
    use zipzi_contract::kernel_wire::Query;
    use zipzi_core::{
        document::*,
        state::{DocumentState, OwnedDocument},
    };
    let state = DocumentState::from_owned(OwnedDocument {
        id: id(1),
        display_info: DisplayInfo {
            extensions: [("vendor".into(), serde_json::json!({"keep": [1, 2]}))].into(),
            ..Default::default()
        },
        model3_settings: Model3Settings {
            layout: Some([("Width".into(), 2.)].into()),
            source_content: Some("opaque".into()),
            ..Default::default()
        },
        missing_attachments: vec!["missing.bin".into()],
        ..Default::default()
    })
    .unwrap();
    let expected = serde_json::json!({
        "editor_state": state.editor_state(),
        "display_info": state.display_info(),
        "model3_settings": state.model3_settings(),
        "hierarchy_order": state.hierarchy_order(),
        "missing_attachments": state.missing_attachments(),
    });
    let length = serde_json::to_vec(&expected).unwrap().len();
    let mut r = Runtime::default();
    let c = r.register_client().unwrap();
    let s = r
        .adopt_session(
            c,
            AuthoringSession::new(Snapshot::new(state, Default::default()).unwrap()),
        )
        .unwrap();
    let snapshot = r.read_snapshot(c, s).unwrap();
    let response = crate::query::read(&snapshot, Query::Metadata, length).unwrap();
    assert_eq!(response["value"], expected);
    assert_eq!(
        response["source"],
        serde_json::to_value(snapshot.source).unwrap()
    );
    assert_eq!(
        response["content"],
        serde_json::to_value(snapshot.snapshot.content_id()).unwrap()
    );
    assert_eq!(
        code(crate::query::read(&snapshot, Query::Metadata, length - 1)),
        "RESOURCE_LIMIT"
    );
}
fn version(r: &Runtime, c: ClientId, s: SessionKey) -> Version {
    r.session_read(c, s).unwrap().version()
}
fn draft(r: &mut Runtime, c: ClientId, s: SessionKey) -> DraftSource {
    r.begin_draft(c, version(r, c, s)).unwrap()
}
fn add(r: &mut Runtime, c: ClientId, s: SessionKey, n: u32) -> Publication {
    let d = draft(r, c, s);
    let (d, _) = r
        .batch(c, d, |cmd| cmd.create_parameter(parameter(n)))
        .unwrap();
    r.publish(c, d, "add").unwrap()
}
fn code<T>(value: Result<T>) -> String {
    match value {
        Ok(_) => panic!("expected failure"),
        Err(e) => e.code,
    }
}
fn result<'a>(r: &'a Runtime, c: ClientId, j: JobId) -> &'a Result<JobValue> {
    r.job(c, j).unwrap().result.as_ref().unwrap()
}
fn error_code(r: &Runtime, c: ClientId, j: JobId) -> &str {
    &result(r, c, j).as_ref().unwrap_err().code
}
fn run(r: &mut Runtime) {
    let completion = r.next_work().unwrap().run();
    r.complete(completion).unwrap();
}

#[test]
fn worker_updates_only_its_draft_and_session_remains_resident() {
    let (mut r, c, s) = setup();
    let a = draft(&mut r, c, s);
    let b = draft(&mut r, c, s);
    let ptr = r.session_read(c, s).unwrap() as *const AuthoringSession;
    let j = r
        .request_batch(c, a, TaskBudget::default(), |cmd, _| {
            cmd.create_parameter(parameter(2))
        })
        .unwrap();
    let work = r.next_work().unwrap();
    assert_eq!(r.session_read(c, s).unwrap() as *const _, ptr);
    assert!(r
        .read_snapshot(c, s)
        .unwrap()
        .snapshot
        .document()
        .parameter_order()
        .is_empty());
    assert_eq!(code(r.batch(c, a, |_| Ok(()))), "DRAFT_BUSY");
    let (b, _) = r
        .batch(c, b, |cmd| cmd.create_parameter(parameter(3)))
        .unwrap();
    let completed = std::thread::spawn(move || work.run()).join().unwrap();
    r.complete(completed).unwrap();
    let JobValue::DraftUpdated(a) = result(&r, c, j).as_ref().unwrap() else {
        panic!()
    };
    let a = *a;
    assert_eq!(a.sequence, 1);
    assert_eq!(
        r.read_draft(c, a)
            .unwrap()
            .snapshot
            .document()
            .parameter_order()
            .len(),
        1
    );
    r.publish(c, a, "worker edit").unwrap();
    assert_eq!(code(r.publish(c, b, "stale isolated edit")), "STALE_SOURCE");
    assert_eq!(code(r.read_draft(c, b)), "DRAFT_ENDED");
    assert_eq!(r.session_read(c, s).unwrap() as *const _, ptr);
    assert_eq!(r.session_read(c, s).unwrap().history_state().undo_steps, 1);
}

#[test]
fn errors_ignored_command_errors_and_stale_sequences_end_the_draft() {
    let (mut r, c, s) = setup();
    let d = draft(&mut r, c, s);
    assert!(r
        .batch(c, d, |cmd| {
            cmd.create_parameter(parameter(2))?;
            let _ignored = cmd.rename(NameTarget::Parameter(&id(999)), "missing".into());
            Ok(())
        })
        .is_err());
    assert_eq!(code(r.read_draft(c, d)), "DRAFT_ENDED");
    assert!(r
        .read_snapshot(c, s)
        .unwrap()
        .snapshot
        .document()
        .parameter(&id(2))
        .is_none());
    let d = draft(&mut r, c, s);
    r.batch(c, d, |_| Ok(())).unwrap();
    assert_eq!(code(r.batch(c, d, |_| Ok(()))), "STALE_SOURCE");
    assert_eq!(code(r.read_draft(c, d)), "DRAFT_ENDED");
    let d = draft(&mut r, c, s);
    assert_eq!(
        code(r.batch::<()>(c, d, |_| panic!("injected command panic"))),
        "COMMAND_PANIC"
    );
    assert_eq!(code(r.read_draft(c, d)), "DRAFT_ENDED");
}

#[test]
fn ownership_and_pre_admission_limits_do_not_consume_drafts() {
    let (mut r, c, s) = setup();
    let d = draft(&mut r, c, s);
    let other = r.register_client().unwrap();
    r.attach(other, s).unwrap();
    assert_eq!(code(r.publish(other, d, "unauthorized")), "NOT_OWNER");
    assert_eq!(code(r.cancel_draft(other, d.draft)), "NOT_OWNER");
    r.limits.jobs = 0;
    assert_eq!(
        code(r.request_publish(c, d, "full", TaskBudget::default())),
        "RESOURCE_LIMIT"
    );
    assert!(r.read_draft(c, d).is_ok());
    r.limits.jobs = 256;
    assert!(r
        .request_batch(
            c,
            d,
            TaskBudget {
                workspace_bytes: usize::MAX,
                result_bytes: 0
            },
            |_, _| Ok(())
        )
        .is_err());
    assert!(r.read_draft(c, d).is_ok());
    r.publish(c, d, "no-op").unwrap();
    assert_eq!(r.session_read(c, s).unwrap().history_state().undo_steps, 0);
}

#[test]
fn cancelled_or_panicking_workers_never_require_session_recovery() {
    let (mut r, c, s) = setup();
    add(&mut r, c, s, 2);
    let before = r.read_snapshot(c, s).unwrap();
    let d = draft(&mut r, c, s);
    let j = r
        .request_batch(
            c,
            d,
            TaskBudget {
                workspace_bytes: 4096,
                result_bytes: 0,
            },
            |cmd, _| {
                cmd.create_parameter(parameter(3))?;
                panic!("injected worker failure")
            },
        )
        .unwrap();
    let work = r.next_work().unwrap();
    assert_eq!(r.task_bytes(), 4096);
    let completed = work.run();
    assert_eq!(r.task_bytes(), 0);
    r.complete(completed).unwrap();
    assert_eq!(error_code(&r, c, j), "WORKER_PANIC");
    assert_eq!(r.read_snapshot(c, s).unwrap().source, before.source);
    assert_eq!(r.session_read(c, s).unwrap().history_state().undo_steps, 1);
    let d = draft(&mut r, c, s);
    let j = r
        .request_batch(
            c,
            d,
            TaskBudget {
                workspace_bytes: 4096,
                result_bytes: 0,
            },
            |cmd, _| cmd.create_parameter(parameter(3)),
        )
        .unwrap();
    let work = r.next_work().unwrap();
    r.cancel_job(c, j).unwrap();
    assert_eq!(r.task_bytes(), 4096, "cancellation is not resource release");
    r.complete(work.run()).unwrap();
    assert_eq!(error_code(&r, c, j), "CANCELLED");
    assert_eq!(code(r.read_draft(c, d)), "DRAFT_ENDED");
    r.undo(c, version(&r, c, s)).unwrap();
    assert!(r
        .session_read(c, s)
        .unwrap()
        .snapshot()
        .document()
        .parameter(&id(2))
        .is_none());
}

#[test]
fn late_prepared_publication_checks_revision_even_when_undo_restores_content() {
    let (mut r, c, s) = setup();
    let content = r.session_read(c, s).unwrap().snapshot().content_id();
    let d = draft(&mut r, c, s);
    let (d, _) = r
        .batch(c, d, |cmd| cmd.create_parameter(parameter(3)))
        .unwrap();
    let j = r
        .request_publish(c, d, "late", TaskBudget::default())
        .unwrap();
    assert_eq!(code(r.read_draft(c, d)), "DRAFT_ENDED");
    let completed = r.next_work().unwrap().run();
    add(&mut r, c, s, 2);
    r.undo(c, version(&r, c, s)).unwrap();
    assert_eq!(
        r.session_read(c, s).unwrap().snapshot().content_id(),
        content
    );
    r.complete(completed).unwrap();
    assert_eq!(error_code(&r, c, j), "STALE_SOURCE");
    assert_eq!(r.session_read(c, s).unwrap().history_state().redo_steps, 1);
    assert_eq!(r.publications_since(c, d.base).unwrap().len(), 2);
}

#[test]
fn replacement_and_late_cancellation_discard_worker_candidates() {
    for replace in [true, false] {
        let (mut r, c, s) = setup();
        let d = draft(&mut r, c, s);
        let j = r
            .request_batch(c, d, TaskBudget::default(), |cmd, _| {
                cmd.create_parameter(parameter(2))
            })
            .unwrap();
        let completion = r.next_work().unwrap().run();
        if replace {
            let snapshot = r.read_snapshot(c, s).unwrap().snapshot;
            r.replace(c, version(&r, c, s), snapshot).unwrap();
        } else {
            r.cancel_job(c, j).unwrap();
        }
        r.complete(completion).unwrap();
        assert_eq!(
            error_code(&r, c, j),
            if replace { "STALE_SOURCE" } else { "CANCELLED" }
        );
        assert_eq!(code(r.read_draft(c, d)), "DRAFT_ENDED");
        assert!(r
            .session_read(c, s)
            .unwrap()
            .snapshot()
            .document()
            .parameter(&id(2))
            .is_none());
    }
}

#[test]
fn frozen_reads_keep_their_source_and_result_charge_outlives_job() {
    let (mut r, c, s) = setup();
    let source = QuerySource::Published(version(&r, c, s));
    let j = r
        .request_read(
            c,
            source,
            TaskBudget {
                workspace_bytes: 100,
                result_bytes: 4,
            },
            |snapshot, _| {
                assert!(snapshot.snapshot.document().parameter(&id(2)).is_none());
                Ok(vec![1, 2, 3, 4])
            },
        )
        .unwrap();
    let work = r.next_work().unwrap();
    add(&mut r, c, s, 2);
    r.complete(work.run()).unwrap();
    assert_eq!(r.task_bytes(), 4);
    let JobValue::Read(value) = result(&r, c, j).as_ref().unwrap() else {
        panic!()
    };
    let retained = value.clone();
    assert_eq!(retained.source, source);
    r.release_job(c, j).unwrap();
    assert_eq!(r.task_bytes(), 4);
    assert_eq!(retained.bytes(), [1, 2, 3, 4]);
    drop(retained);
    assert_eq!(r.task_bytes(), 0);
}

#[test]
fn oversized_read_results_fail_and_disconnected_work_stays_charged_until_exit() {
    let (mut r, c, s) = setup();
    let source = QuerySource::Published(version(&r, c, s));
    let j = r
        .request_read(
            c,
            source,
            TaskBudget {
                workspace_bytes: 100,
                result_bytes: 4,
            },
            |_, _| Ok(Vec::with_capacity(5)),
        )
        .unwrap();
    run(&mut r);
    assert_eq!(error_code(&r, c, j), "RESOURCE_LIMIT");
    assert_eq!(r.task_bytes(), 0);
    let j = r
        .request_read(
            c,
            source,
            TaskBudget {
                workspace_bytes: 100,
                result_bytes: 4,
            },
            |_, _| Ok(vec![1]),
        )
        .unwrap();
    let work = r.next_work().unwrap();
    r.release_client(c);
    assert_eq!(r.task_bytes(), 104);
    r.complete(work.run()).unwrap();
    assert_eq!(r.task_bytes(), 0);
    assert!(!r.jobs.contains_key(&j));
    let c = r.register_client().unwrap();
    r.attach(c, s).unwrap();
    assert_eq!(r.read_snapshot(c, s).unwrap().source, source);
}

struct Reject;
impl Executor for Reject {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        Err(Box::new(work))
    }
}
#[test]
fn dispatch_rejection_preserves_work_then_pool_delivers_on_a_worker_thread() {
    let (mut r, c, s) = setup();
    let d = draft(&mut r, c, s);
    let j = r
        .request_batch(c, d, TaskBudget::default(), |cmd, _| {
            cmd.create_parameter(parameter(2))
        })
        .unwrap();
    r.dispatch(&mut Reject);
    assert_eq!(r.job(c, j).unwrap().state, JobState::Queued);
    assert!(r.read_snapshot(c, s).is_ok());
    let (tx, rx) = mpsc::channel();
    let owner = std::thread::current().id();
    let mut pool = WorkerPool::new(1, 2 << 20, move |completion| {
        tx.send((std::thread::current().id(), completion)).unwrap()
    })
    .unwrap();
    r.dispatch(&mut pool);
    let (worker, completed) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_ne!(worker, owner);
    r.complete(completed).unwrap();
    assert!(matches!(result(&r, c, j), Ok(JobValue::DraftUpdated(_))));
}

#[test]
fn save_is_exclusive_but_readable_and_late_cancel_installs_the_actual_marker() {
    let (mut r, c, s) = setup();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("model.zipzi");
    let d = draft(&mut r, c, s);
    let v = version(&r, c, s);
    let before = r.read_snapshot(c, s).unwrap().snapshot;
    let j = r
        .request_save_as(c, v, &io, &path, None, TaskBudget::default())
        .unwrap();
    let work = r.next_work().unwrap();
    assert_eq!(code(r.begin_draft(c, v)), "PROJECT_BUSY");
    assert_eq!(code(r.batch(c, d, |_| Ok(()))), "PROJECT_BUSY");
    assert_eq!(code(r.undo(c, v)), "PROJECT_BUSY");
    assert_eq!(code(r.replace(c, v, before.clone())), "PROJECT_BUSY");
    assert_eq!(
        code(r.request_save_as(c, v, &io, &path, None, TaskBudget::default())),
        "PROJECT_BUSY"
    );
    assert!(r.read_draft(c, d).is_ok());
    let read = r
        .request_read(
            c,
            QuerySource::Published(v),
            TaskBudget {
                workspace_bytes: 0,
                result_bytes: 1,
            },
            |_, _| Ok(vec![1]),
        )
        .unwrap();
    run(&mut r);
    assert!(result(&r, c, read).is_ok());
    let completed = work.run();
    assert!(path.exists());
    assert!(r.session_read(c, s).unwrap().is_dirty());
    r.cancel_job(c, j).unwrap();
    assert_eq!(code(r.close_session(c, s, true)), "SESSION_BUSY");
    r.complete(completed).unwrap();
    assert!(matches!(result(&r, c, j), Ok(JobValue::Saved(receipt)) if receipt.published));
    let saved = r.session_read(c, s).unwrap();
    assert_eq!(saved.saved_marker().unwrap().content, before.content_id());
    assert!(!saved.is_dirty());
    let (d, _) = r
        .batch(c, d, |cmd| cmd.create_parameter(parameter(2)))
        .unwrap();
    r.publish(c, d, "after save").unwrap();
    assert!(r.session_read(c, s).unwrap().is_dirty());
    r.undo(c, version(&r, c, s)).unwrap();
    assert!(!r.session_read(c, s).unwrap().is_dirty());
}

#[test]
fn cancellation_before_save_publication_releases_permit_and_target() {
    let (mut r, c, s) = setup();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("model.zipzi");
    let v = version(&r, c, s);
    let j = r
        .request_save_as(c, v, &io, &path, None, TaskBudget::default())
        .unwrap();
    r.cancel_job(c, j).unwrap();
    run(&mut r);
    assert_eq!(error_code(&r, c, j), "CANCELLED");
    assert!(!path.exists());
    assert!(r.session_read(c, s).unwrap().project().is_none());
    add(&mut r, c, s, 2);
    r.request_save_as(
        c,
        version(&r, c, s),
        &io,
        &path,
        None,
        TaskBudget::default(),
    )
    .unwrap();
    run(&mut r);
    assert!(!r.session_read(c, s).unwrap().is_dirty());
}

#[test]
fn disconnected_save_completion_still_updates_resident_session() {
    let (mut r, c, s) = setup();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("model.zipzi");
    let j = r
        .request_save_as(
            c,
            version(&r, c, s),
            &io,
            &path,
            None,
            TaskBudget::default(),
        )
        .unwrap();
    let completed = r.next_work().unwrap().run();
    r.release_client(c);
    r.complete(completed).unwrap();
    assert!(!r.jobs.contains_key(&j));
    let c = r.register_client().unwrap();
    r.attach(c, s).unwrap();
    assert!(!r.session_read(c, s).unwrap().is_dirty());
    assert_eq!(
        r.session_read(c, s).unwrap().project().unwrap().path(),
        io.normalize_path(&path).unwrap()
    );
    r.request_save(c, version(&r, c, s), &io, TaskBudget::default())
        .unwrap();
    run(&mut r);
}

#[test]
fn open_checks_request_source_and_resets_history_only_on_success() {
    let (mut r, c, s) = setup();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("model.zipzi");
    r.request_save_as(
        c,
        version(&r, c, s),
        &io,
        &path,
        None,
        TaskBudget::default(),
    )
    .unwrap();
    run(&mut r);
    let j = r
        .request_open(c, version(&r, c, s), &io, &path, TaskBudget::default())
        .unwrap();
    let completed = r.next_work().unwrap().run();
    add(&mut r, c, s, 2);
    r.complete(completed).unwrap();
    assert_eq!(error_code(&r, c, j), "STALE_SOURCE");
    assert_eq!(r.session_read(c, s).unwrap().history_state().undo_steps, 1);
    let v = version(&r, c, s);
    let j = r
        .request_open(c, v, &io, &path, TaskBudget::default())
        .unwrap();
    run(&mut r);
    assert!(matches!(result(&r, c, j), Ok(JobValue::Opened(_))));
    assert_eq!(version(&r, c, s).generation, v.generation + 1);
    assert_eq!(r.session_read(c, s).unwrap().history_state().undo_steps, 0);
    assert!(!r.session_read(c, s).unwrap().is_dirty());
    assert!(r
        .session_read(c, s)
        .unwrap()
        .snapshot()
        .document()
        .parameter(&id(2))
        .is_none());
}

#[test]
fn save_reserves_paths_and_waits_for_mutation_tasks() {
    let (mut r, c, s) = setup();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("model.zipzi");
    let d = draft(&mut r, c, s);
    r.request_batch(c, d, TaskBudget::default(), |_, _| Ok(()))
        .unwrap();
    assert_eq!(
        code(r.request_save_as(
            c,
            version(&r, c, s),
            &io,
            &path,
            None,
            TaskBudget::default()
        )),
        "PROJECT_BUSY"
    );
    run(&mut r);
    r.request_save_as(
        c,
        version(&r, c, s),
        &io,
        &path,
        None,
        TaskBudget::default(),
    )
    .unwrap();
    let other = r.create_session(c, &id(9), ModelSetup::default()).unwrap();
    assert_eq!(
        code(r.request_save_as(
            c,
            version(&r, c, other),
            &io,
            &path,
            None,
            TaskBudget::default()
        )),
        "PROJECT_BUSY"
    );
    run(&mut r);
    assert_eq!(
        code(r.request_open(c, version(&r, c, other), &io, &path, TaskBudget::default())),
        "PROJECT_BUSY"
    );
    r.close_session(c, s, false).unwrap();
    r.request_open(c, version(&r, c, other), &io, &path, TaskBudget::default())
        .unwrap();
    run(&mut r);
    assert!(!r.session_read(c, other).unwrap().is_dirty());
}

#[test]
fn worker_publication_updates_history_and_bounded_notifications_once() {
    let (mut r, c, s) = setup();
    r.limits.events_per_session = 2;
    let before = r.subscribe(c, s).unwrap();
    let d = draft(&mut r, c, s);
    let (d, _) = r
        .batch(c, d, |cmd| cmd.create_parameter(parameter(2)))
        .unwrap();
    let j = r
        .request_publish(c, d, "worker commit", TaskBudget::default())
        .unwrap();
    run(&mut r);
    assert!(matches!(result(&r, c, j), Ok(JobValue::Published(_))));
    let first = version(&r, c, s);
    r.undo(c, first).unwrap();
    assert_eq!(r.publications_since(c, before.cursor).unwrap().len(), 2);
    r.redo(c, version(&r, c, s)).unwrap();
    assert_eq!(code(r.publications_since(c, before.cursor)), "EVENT_GAP");
    assert_eq!(r.publications_since(c, first).unwrap().len(), 2);
    let other = r.register_client().unwrap();
    r.attach(other, s).unwrap();
    assert_eq!(r.publications_since(other, first).unwrap().len(), 2);
    let now = r.subscribe(c, s).unwrap();
    assert_eq!(now.snapshot.source, QuerySource::Published(now.cursor));
    assert_eq!(now.history.undo_steps, 1);
    assert!(r.publications_since(c, now.cursor).unwrap().is_empty());
    assert_eq!(
        code(r.publications_since(
            c,
            Version {
                generation: 999,
                ..now.cursor
            }
        )),
        "INVALID_CURSOR"
    );
    assert!(r.sessions.get_mut(&s).unwrap().sdk.take_events().is_empty());
}

#[test]
fn misrouted_completion_is_returned_for_delivery_including_published_saves() {
    let (mut r, c, s) = setup();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let j = r
        .request_save_as(
            c,
            version(&r, c, s),
            &io,
            &root.path().join("model.zipzi"),
            None,
            TaskBudget::default(),
        )
        .unwrap();
    let completed = r.next_work().unwrap().run();
    let mut wrong_host = Runtime::default();
    let completed = wrong_host.complete(completed).unwrap_err();
    assert_eq!(completed.job_id(), j);
    r.complete(completed).unwrap();
    assert!(!r.session_read(c, s).unwrap().is_dirty());
}

#[test]
fn cancellation_does_not_release_running_slots_before_completion() {
    let (mut r, c, s) = setup();
    r.limits.running_jobs = 1;
    let source = QuerySource::Published(version(&r, c, s));
    let j = r
        .request_read(c, source, TaskBudget::default(), |_, _| Ok(vec![]))
        .unwrap();
    let second = r
        .request_read(c, source, TaskBudget::default(), |_, _| Ok(vec![]))
        .unwrap();
    let work = r.next_work().unwrap();
    r.cancel_job(c, j).unwrap();
    assert!(r.next_work().is_none());
    assert_eq!(code(r.release_job(c, j)), "JOB_BUSY");
    r.complete(work.run()).unwrap();
    assert_eq!(r.job(c, j).unwrap().state, JobState::Cancelled);
    run(&mut r);
    assert_eq!(r.job(c, second).unwrap().state, JobState::Succeeded);
}

#[test]
fn preview_rejection_preserves_candidate_but_success_invalidates_previous_source() {
    let (mut r, c, s) = setup();
    let d = draft(&mut r, c, s);
    let (d, _) = r
        .batch(c, d, |cmd| cmd.create_parameter(parameter(2)))
        .unwrap();
    let failed = r.preview(c, d, |cmd| {
        cmd.rename(NameTarget::Parameter(&id(2)), "temporary".into())?;
        cmd.rename(NameTarget::Parameter(&id(999)), "missing".into())
    });
    assert_eq!(code(failed), "MISSING_OBJECT");
    assert_eq!(
        r.read_draft(c, d)
            .unwrap()
            .snapshot
            .document()
            .parameter(&id(2))
            .unwrap()
            .name,
        "p2"
    );
    let (next, ()) = r
        .preview(c, d, |cmd| {
            cmd.rename(NameTarget::Parameter(&id(2)), "valid".into())
        })
        .unwrap();
    assert_eq!(next.sequence, d.sequence + 1);
    assert_eq!(code(r.read_draft(c, d)), "STALE_SOURCE");
    r.publish(c, next, "gesture").unwrap();
    assert_eq!(
        r.session_read(c, s)
            .unwrap()
            .snapshot()
            .document()
            .parameter(&id(2))
            .unwrap()
            .name,
        "valid"
    );
    assert_eq!(r.session_read(c, s).unwrap().history_state().undo_steps, 1);
}

#[test]
fn read_quanta_yield_fairly_and_keep_frozen_source_and_reservations() {
    let (mut r, c, s) = setup();
    let source = QuerySource::Published(version(&r, c, s));
    let captured = r.read_snapshot(c, s).unwrap().snapshot.content_id();
    let mut turns = 0;
    let long = r
        .request_read_steps(
            c,
            source,
            TaskBudget {
                workspace_bytes: 64,
                result_bytes: 4,
            },
            move |snapshot, _| {
                assert_eq!(snapshot.snapshot.content_id(), captured);
                turns += 1;
                Ok(if turns == 3 {
                    ReadProgress::Complete(vec![1, 2, 3, 4])
                } else {
                    ReadProgress::Yield
                })
            },
        )
        .unwrap();
    let short = r
        .request_read(c, source, TaskBudget::default(), |_, _| Ok(vec![]))
        .unwrap();
    run(&mut r);
    assert_eq!(r.job(c, long).unwrap().state, JobState::Queued);
    assert_eq!(r.task_bytes(), 68);
    add(&mut r, c, s, 2);
    run(&mut r);
    assert_eq!(r.job(c, short).unwrap().state, JobState::Succeeded);
    run(&mut r);
    run(&mut r);
    assert_eq!(r.job(c, long).unwrap().state, JobState::Succeeded);
    assert_eq!(r.task_bytes(), 4);
    let JobValue::Read(value) = result(&r, c, long).as_ref().unwrap() else {
        panic!()
    };
    assert_eq!(value.source, source);
    assert_eq!(value.bytes(), &[1, 2, 3, 4]);
    r.release_job(c, long).unwrap();
    assert_eq!(r.task_bytes(), 0);
}

#[test]
fn detached_yielded_read_is_retired_on_worker() {
    let (mut r, c, s) = setup();
    let source = QuerySource::Published(version(&r, c, s));
    let j = r
        .request_read_steps(
            c,
            source,
            TaskBudget {
                workspace_bytes: 32,
                result_bytes: 0,
            },
            |_, _| Ok(ReadProgress::Yield),
        )
        .unwrap();
    let completion = r.next_work().unwrap().run();
    r.release_client(c);
    r.complete(completion).unwrap();
    assert_eq!(r.outstanding_jobs(), 1);
    assert_eq!(r.task_bytes(), 32);
    run(&mut r);
    assert_eq!(r.outstanding_jobs(), 0);
    assert!(!r.jobs.contains_key(&j));
    assert_eq!(r.task_bytes(), 0);
    let other = r.register_client().unwrap();
    r.attach(other, s).unwrap();
    assert_eq!(r.read_snapshot(other, s).unwrap().source, source);
}

#[test]
fn native_adoption_and_rehome_preserve_identity_history_and_saved_marker() {
    let (mut source, client, session) = setup();
    add(&mut source, client, session, 2);
    let io = source.project_io().clone();
    let dir = tempfile::tempdir().unwrap();
    let save = source
        .request_save_as(
            client,
            version(&source, client, session),
            &io,
            &dir.path().join("host.zipzi"),
            None,
            TaskBudget::default(),
        )
        .unwrap();
    run(&mut source);
    source.release_job(client, save).unwrap();
    let content = source
        .read_snapshot(client, session)
        .unwrap()
        .snapshot
        .content_id();
    let before = version(&source, client, session);
    let mut target = Runtime::default();
    let destination = target.register_client().unwrap();
    assert_eq!(
        source
            .transfer_session_to(client, session, &mut target, destination)
            .unwrap(),
        session
    );
    assert_eq!(
        code(source.read_snapshot(client, session)),
        "SESSION_NOT_FOUND"
    );
    let sdk = target.session_read(destination, session).unwrap();
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.snapshot().content_id(), content);
    assert_eq!(sdk.history_state().undo_steps, 1);
    assert!(!sdk.is_dirty());
    target.undo(destination, before).unwrap();
    assert!(target
        .session_read(destination, session)
        .unwrap()
        .is_dirty());
    let (loaded, _) =
        zipzi_sdk::AuthoringSession::open(&io, &dir.path().join("host.zipzi"), || true).unwrap();
    let mut fresh = Runtime::default();
    let c = fresh.register_client().unwrap();
    let key = loaded.version().session;
    assert_eq!(fresh.adopt_session(c, loaded).unwrap(), key);
    assert!(!fresh.session_read(c, key).unwrap().is_dirty());
}

#[test]
fn native_rehome_admission_preserves_both_hosts_and_view_stamp_ignores_drafts() {
    let (mut source, c, s) = setup();
    let mut target = Runtime::default();
    let destination = target.register_client().unwrap();
    let stamp = source.host_state_stamp();
    let d = draft(&mut source, c, s);
    assert_eq!(source.host_state_stamp(), stamp);
    assert_eq!(
        code(source.transfer_session_to(c, s, &mut target, destination)),
        "SESSION_BUSY"
    );
    source.cancel_draft(c, d.draft).unwrap();
    let other = source.register_client().unwrap();
    source.attach(other, s).unwrap();
    assert_eq!(
        code(source.transfer_session_to(c, s, &mut target, destination)),
        "SESSION_BUSY"
    );
    assert!(target.session_keys(destination).unwrap().is_empty());
    source.detach(other, s).unwrap();
    add(&mut source, c, s, 2);
    assert_ne!(source.host_state_stamp(), stamp);
    source
        .transfer_session_to(c, s, &mut target, destination)
        .unwrap();
    assert_eq!(target.session_keys(destination).unwrap(), vec![s]);
}

#[test]
fn targeted_dispatch_refuses_without_consuming_queued_work() {
    let (mut rt, c, s) = setup();
    rt.limits.running_jobs = 1;
    let source = QuerySource::Published(version(&rt, c, s));
    let first = rt
        .request_read(c, source, TaskBudget::default(), |_, _| Ok(vec![]))
        .unwrap();
    let second = rt
        .request_read(c, source, TaskBudget::default(), |_, _| Ok(vec![]))
        .unwrap();
    let running = rt.take_work(second).unwrap();
    assert_eq!(code(rt.take_work(first)), "JOB_BUSY");
    assert_eq!(rt.job(c, first).unwrap().state, JobState::Queued);
    rt.complete(running.run()).unwrap();
    let pending = rt.take_work(first).unwrap();
    rt.complete(pending.run()).unwrap();
    assert_eq!(rt.job(c, first).unwrap().state, JobState::Succeeded);
}

#[test]
fn immediate_save_admits_concurrency_before_acquiring_the_save_permit() {
    let (mut rt, client, session) = setup();
    rt.limits.running_jobs = 1;
    let expected = version(&rt, client, session);
    let draft = rt.begin_draft(client, expected).unwrap();
    let read = rt
        .request_read(
            client,
            QuerySource::Published(expected),
            TaskBudget::default(),
            |_, _| Ok(vec![]),
        )
        .unwrap();
    let running = rt.take_work(read).unwrap();
    let io = rt.project_io().clone();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.zipzi");
    let stamp = rt.host_state_stamp();
    assert_eq!(
        code(rt.start_save_as(client, expected, &io, &path, None, TaskBudget::default())),
        "JOB_BUSY"
    );
    assert_eq!(rt.host_state_stamp(), stamp);
    assert!(rt.reservations.is_empty());
    assert!(!path.exists());
    let (draft, ()) = rt
        .batch(client, draft, |commands| {
            commands.create_parameter(parameter(2))
        })
        .unwrap();
    rt.complete(running.run()).unwrap();
    rt.release_job(client, read).unwrap();

    let (job, work) = rt
        .start_save_as(client, expected, &io, &path, None, TaskBudget::default())
        .unwrap();
    assert_eq!(rt.job(client, job).unwrap().state, JobState::Running);
    assert_eq!(
        code(rt.publish(client, draft, "pending edit")),
        "PROJECT_BUSY"
    );
    assert!(rt.read_draft(client, draft).is_ok());
    rt.complete(work.run()).unwrap();
    assert!(matches!(result(&rt, client, job), Ok(JobValue::Saved(_))));
    rt.release_job(client, job).unwrap();
    assert!(path.exists());
    assert!(!rt.session_read(client, session).unwrap().is_dirty());
    rt.publish(client, draft, "pending edit").unwrap();
    assert!(rt.session_read(client, session).unwrap().is_dirty());
}

#[test]
fn timeline_preview_resets_from_baseline_and_preserves_last_valid_curve() {
    use zipzi_core::document::{
        MotionClip, MotionKey, MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
    };
    use zipzi_sdk::{MotionCurveEdit, MotionKeyMove, MotionKeyRef};
    let (mut rt, client, session) = setup();
    let clip = MotionClip {
        id: id(30),
        name: "gesture".into(),
        duration: 1.,
        fps: 30.,
        looping: false,
        restricted_beziers: false,
        fade_in: None,
        fade_out: None,
        track_order: vec![],
        tracks: vec![MotionTrack {
            id: id(31),
            target: MotionTrackTarget::Model {
                runtime_id: "Opacity".into(),
            },
            initial: MotionKey {
                id: id(32),
                time: 0.,
                value: 0.,
            },
            segments: Arc::new(vec![MotionSegment::Linear {
                end: MotionKey {
                    id: id(33),
                    time: 1.,
                    value: 1.,
                },
            }]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        }]
        .into(),
        events: Default::default(),
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    let d = draft(&mut rt, client, session);
    let (d, ()) = rt
        .batch(client, d, |c| c.create_motion(clip.clone()))
        .unwrap();
    rt.publish(client, d, "motion").unwrap();
    let base = version(&rt, client, session);
    let d = draft(&mut rt, client, session);
    let preview = |rt: &mut Runtime, d, time| {
        rt.preview(client, d, |commands| {
            commands.replace_motion(clip.clone())?;
            commands.edit_motion_curve(
                &clip.id,
                MotionCurveEdit::MoveKeys(vec![MotionKeyMove {
                    key: MotionKeyRef {
                        track_id: id(31),
                        key_id: id(32),
                    },
                    point: MotionPoint { time, value: 0.5 },
                }]),
            )
        })
    };
    let (d, ()) = preview(&mut rt, d, 0.2).unwrap();
    let captured = rt.read_draft(client, d).unwrap();
    assert!(preview(&mut rt, d, 1.).is_err());
    assert_eq!(rt.read_draft(client, d).unwrap().source, captured.source);
    assert_eq!(
        rt.read_draft(client, d)
            .unwrap()
            .snapshot
            .document()
            .motion(&clip.id)
            .unwrap()
            .tracks[0]
            .initial
            .time,
        0.2
    );
    let (d, ()) = preview(&mut rt, d, 0.3).unwrap();
    assert_eq!(
        rt.read_draft(client, d)
            .unwrap()
            .snapshot
            .document()
            .motion(&clip.id)
            .unwrap()
            .tracks[0]
            .initial
            .time,
        0.3
    );
    assert_eq!(
        captured
            .snapshot
            .document()
            .motion(&clip.id)
            .unwrap()
            .tracks[0]
            .initial
            .time,
        0.2
    );
    assert_eq!(version(&rt, client, session), base);
    let committed = rt.publish(client, d, "drag keys").unwrap();
    assert_eq!(committed.history.undo_steps, 2);
    rt.undo(client, committed.version).unwrap();
    assert_eq!(
        rt.read_snapshot(client, session)
            .unwrap()
            .snapshot
            .document()
            .motion(&clip.id)
            .unwrap()
            .tracks[0]
            .initial
            .time,
        0.
    );
}

#[test]
fn imported_content_is_prepared_on_worker_and_installed_only_at_its_source() {
    use zipzi_contract::kernel_wire::ImportFormat;
    let (mut rt, c, s) = setup();
    add(&mut rt, c, s, 2);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.psd");
    std::fs::write(
        &path,
        include_bytes!("../../zipzi-psd/tests/fixtures/layered.psd"),
    )
    .unwrap();
    let base = version(&rt, c, s);
    let job = rt
        .request_import(c, base, &path, ImportFormat::Psd, TaskBudget::default())
        .unwrap();
    let ptr = rt.session_read(c, s).unwrap() as *const AuthoringSession;
    let completion = rt
        .take_work(job)
        .unwrap()
        .run_with_stack("test-psd-import", 8 << 20);
    let prepared = completion.prepared_snapshot().unwrap().clone();
    assert!(!prepared.document().asset_order().is_empty());
    assert_eq!(
        ptr,
        rt.session_read(c, s).unwrap() as *const AuthoringSession
    );
    assert_eq!(version(&rt, c, s), base);
    add(&mut rt, c, s, 3);
    rt.complete(completion).unwrap();
    assert_eq!(error_code(&rt, c, job), "STALE_SOURCE");
    assert!(rt
        .read_snapshot(c, s)
        .unwrap()
        .snapshot
        .document()
        .parameter(&id(3))
        .is_some());
    assert_eq!(rt.session_read(c, s).unwrap().history_state().undo_steps, 2);

    let job = rt
        .request_import(
            c,
            version(&rt, c, s),
            &path,
            ImportFormat::Psd,
            TaskBudget::default(),
        )
        .unwrap();
    let completion = rt.take_work(job).unwrap().run();
    rt.cancel_job(c, job).unwrap();
    rt.complete(completion).unwrap();
    assert_eq!(rt.job(c, job).unwrap().state, JobState::Cancelled);
    assert_eq!(rt.session_read(c, s).unwrap().history_state().undo_steps, 2);

    let job = rt
        .request_import(
            c,
            version(&rt, c, s),
            &path,
            ImportFormat::Psd,
            TaskBudget::default(),
        )
        .unwrap();
    run(&mut rt);
    let JobValue::Imported(receipt) = result(&rt, c, job).as_ref().unwrap() else {
        panic!("import result")
    };
    assert_eq!(receipt.publication.version.generation, base.generation + 1);
    let sdk = rt.session_read(c, s).unwrap();
    assert_eq!(sdk.history_state().undo_steps, 0);
    assert!(sdk.is_dirty());
    assert!(sdk.project().is_none());
    let frozen = rt.read_snapshot(c, s).unwrap().snapshot;
    let asset = frozen
        .document()
        .asset(frozen.document().asset_order().first().unwrap())
        .unwrap();
    let pixels = frozen.resources().read_texture(asset).unwrap();
    let io = rt.project_io().clone();
    assert_eq!(
        code(rt.request_save_as(
            c,
            version(&rt, c, s),
            &io,
            &path,
            None,
            TaskBudget::default()
        )),
        "INVALID_DESTINATION"
    );
    let source = QuerySource::Published(version(&rt, c, s));
    let transfer = rt.transfer_snapshot(c, source).unwrap();
    assert_eq!(
        code(rt.transfer_snapshot(c, QuerySource::Published(base))),
        "STALE_SOURCE"
    );
    assert_eq!(
        code(
            rt.transfer_snapshot(c, source)
                .unwrap()
                .publish(zipzi_project::TransferFormat::Runtime, dir.path())
        ),
        "INVALID_DESTINATION"
    );
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        frozen.resources().read_texture(asset).unwrap().rgba,
        pixels.rgba
    );
    let save = rt
        .request_save_as(
            c,
            version(&rt, c, s),
            &io,
            &dir.path().join("native.zipzi"),
            None,
            TaskBudget::default(),
        )
        .unwrap();
    run(&mut rt);
    assert!(matches!(result(&rt, c, save), Ok(JobValue::Saved(_))));
    rt.close_session(c, s, false).unwrap();
    let destination = dir.path().join("export");
    transfer
        .publish(zipzi_project::TransferFormat::Runtime, &destination)
        .unwrap();
    assert!(destination.is_dir());
}

#[test]
fn collection_capabilities_keep_full_query_source_and_draft_sequence() {
    use zipzi_contract::{
        document::OrganizationCollection,
        kernel_wire::{CollectionTarget, Query},
    };
    let (mut r, c, s) = setup();
    let source = draft(&mut r, c, s);
    let (source, _) = r
        .batch(c, source, |cmd| {
            cmd.create_part(zipzi_core::Part {
                id: id(8),
                runtime_id: "part8".into(),
                ..Default::default()
            })
        })
        .unwrap();
    let published = r.publish(c, source, "part").unwrap().version;
    let query = Query::CollectionCapabilities {
        target: CollectionTarget::Organization(OrganizationCollection::PartMembers {
            part_id: Some(id(8)),
        }),
    };
    let before = r
        .query(c, QuerySource::Published(published), query.clone())
        .unwrap();
    let source = draft(&mut r, c, s);
    let (next, _) = r
        .batch(c, source, |cmd| {
            let mut state = zipzi_contract::document::EditorState::default();
            state.objects.entry(id(8)).or_default().locked = true;
            cmd.replace_editor_state(state)
        })
        .unwrap();
    assert_eq!(
        code(r.query(c, QuerySource::Draft(source), query.clone())),
        "STALE_SOURCE"
    );
    let preview = r.query(c, QuerySource::Draft(next), query.clone()).unwrap();
    assert_eq!(
        preview["source"],
        serde_json::to_value(QuerySource::Draft(next)).unwrap()
    );
    assert_ne!(preview["value"]["create"], before["value"]["create"]);
    assert_eq!(
        r.query(c, QuerySource::Published(published), query.clone())
            .unwrap(),
        before
    );
    let publication = r.publish(c, next, "lock").unwrap();
    assert_eq!(
        code(r.query(c, QuerySource::Published(published), query.clone())),
        "STALE_SOURCE"
    );
    assert_eq!(
        r.query(c, QuerySource::Published(publication.version), query)
            .unwrap()["value"],
        preview["value"]
    );
}

#[test]
fn immediate_open_and_import_refusal_preserves_owner_queue_and_reservations() {
    let (mut rt, client, session) = setup();
    rt.limits.running_jobs = 1;
    let expected = version(&rt, client, session);
    let draft = rt.begin_draft(client, expected).unwrap();
    let job = rt
        .request_read(
            client,
            QuerySource::Published(expected),
            TaskBudget::default(),
            |_, _| Ok(vec![]),
        )
        .unwrap();
    let running = rt.take_work(job).unwrap();
    let stamp = rt.host_state_stamp();
    let queue = rt.outstanding_job_ids();
    let io = rt.project_io().clone();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("model.zipzi");
    assert_eq!(
        code(rt.start_open(client, expected, &io, &target, TaskBudget::default())),
        "JOB_BUSY"
    );
    assert_eq!(
        code(rt.start_import(
            client,
            expected,
            &target,
            zipzi_contract::kernel_wire::ImportFormat::Psd,
            TaskBudget::default()
        )),
        "JOB_BUSY"
    );
    assert_eq!(rt.host_state_stamp(), stamp);
    assert_eq!(rt.outstanding_job_ids(), queue);
    assert!(rt.reservations.is_empty());
    rt.complete(running.run()).unwrap();
    rt.release_job(client, job).unwrap();
    // A refused native load has not retired an existing edit or changed history.
    let (draft, _) = rt
        .batch(client, draft, |c| c.create_parameter(parameter(2)))
        .unwrap();
    rt.publish(client, draft, "still usable").unwrap();
    assert_eq!(
        rt.session_read(client, session)
            .unwrap()
            .history_state()
            .undo_steps,
        1
    );
}

#[test]
fn host_is_send_sync_and_events_identify_the_publishing_client() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Runtime>();
    let (mut rt, owner, session) = setup();
    let subscriber = rt.register_client().unwrap();
    rt.attach(subscriber, session).unwrap();
    let cursor = version(&rt, subscriber, session);
    add(&mut rt, owner, session, 2);
    let events = rt.publication_events_since(subscriber, cursor).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].client, owner);
}

#[test]
fn bulk_upload_ownership_and_failed_image_batch_do_not_publish() {
    let (mut rt, client, session) = setup();
    let foreign = rt.register_client().unwrap();
    let blob = rt.create_blob(client, 4).unwrap();
    assert_eq!(
        rt.append_blob(foreign, blob, 0, &[0; 4]).unwrap_err().code,
        "NOT_OWNER"
    );
    rt.append_blob(client, blob, 0, &[1, 2, 3, 255]).unwrap();
    rt.seal_blob(client, blob).unwrap();
    let source = draft(&mut rt, client, session);
    let (source, ()) = rt
        .batch(client, source, |c| {
            c.create_empty_asset(id(2), "image".into())
        })
        .unwrap();
    assert_eq!(
        rt.replace_image_blob(client, source, id(2), blob, 2, 1)
            .unwrap_err()
            .code,
        "INVALID_IMAGE"
    );
    assert!(rt.read_draft(client, source).is_err());
    assert!(rt
        .session_read(client, session)
        .unwrap()
        .snapshot()
        .document()
        .asset(&id(2))
        .is_none());
    let source = draft(&mut rt, client, session);
    let (source, ()) = rt
        .batch(client, source, |c| {
            c.create_empty_asset(id(2), "image".into())
        })
        .unwrap();
    let source = rt
        .replace_image_blob(client, source, id(2), blob, 1, 1)
        .unwrap();
    rt.release_blob(client, blob).unwrap();
    let receipt = rt.publish(client, source, "image").unwrap();
    let result = rt
        .read_image_blob(client, QuerySource::Published(receipt.version), &id(2))
        .unwrap();
    let blob = serde_json::from_value(result["blob"].clone()).unwrap();
    let held = rt.blob(client, blob).unwrap();
    rt.release_client(client);
    assert_eq!(held.bytes(), &[1, 2, 3, 255]);
}

#[test]
fn sampled_animation_reports_requested_time_and_playback_curve_coordinate_without_publication() {
    use zipzi_core::document::{
        MotionClip, MotionKey, MotionSegment, MotionTrack, MotionTrackTarget,
    };
    let (mut rt, client, session) = setup();
    let d = draft(&mut rt, client, session);
    let (d, ()) = rt
        .batch(client, d, |commands| {
            commands.create_parameter(Parameter {
                id: id(2),
                minimum: -30.,
                maximum: 30.,
                ..Default::default()
            })?;
            commands.create_motion(MotionClip {
                id: id(30),
                name: "Linear".into(),
                duration: 1.,
                fps: 30.,
                looping: false,
                restricted_beziers: false,
                fade_in: Some(0.),
                fade_out: Some(0.),
                track_order: vec![],
                tracks: vec![MotionTrack {
                    id: id(31),
                    target: MotionTrackTarget::Parameter {
                        parameter_id: id(2),
                    },
                    initial: MotionKey {
                        id: id(32),
                        time: 0.,
                        value: -20.,
                    },
                    segments: Arc::new(vec![MotionSegment::Linear {
                        end: MotionKey {
                            id: id(33),
                            time: 1.,
                            value: 20.,
                        },
                    }]),
                    fade_in: Some(0.),
                    fade_out: Some(0.),
                    extensions: Default::default(),
                }]
                .into(),
                events: Default::default(),
                extensions: Default::default(),
                meta_extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            })
        })
        .unwrap();
    rt.publish(client, d, "motion").unwrap();
    let version_before = version(&rt, client, session);
    let history_before = rt.session_read(client, session).unwrap().history_state();
    let source = QuerySource::Published(version_before);
    let times = vec![0., 0.001, 0.25, 0.5, 0.75, 1.];
    let request = zipzi_contract::kernel_tasks::SampleRequest {
        input: zipzi_contract::kernel_tasks::SampleInput::Animation {
            times: times.clone(),
            motions: vec![(id(30), 0.)],
            expressions: vec![],
            inputs: vec![],
            physics: false,
            stabilized_start: false,
        },
        queries: vec![],
        quality: false,
    };
    let job = rt.request_samples(client, source, request).unwrap();
    while !rt.job(client, job).unwrap().state.is_terminal() {
        run(&mut rt);
    }
    let JobValue::Read(readback) = result(&rt, client, job).as_ref().unwrap() else {
        panic!("sample result");
    };
    let report: serde_json::Value = serde_json::from_slice(readback.bytes()).unwrap();
    assert_eq!(report["source"], serde_json::to_value(source).unwrap());
    for (frame, requested) in report["frames"].as_array().unwrap().iter().zip(times) {
        let animation = &frame["animation"];
        let local = (requested - 1. / 60.).max(0.);
        assert!((animation["time"].as_f64().unwrap() - f64::from(requested)).abs() < 1e-6);
        assert!(
            (animation["motion_positions"][0]["local_time"]
                .as_f64()
                .unwrap()
                - f64::from(local))
            .abs()
                < 1e-6
        );
        assert!(
            (animation["parameters"][id(2)].as_f64().unwrap() - f64::from(-20. + 40. * local))
                .abs()
                < 1e-5
        );
    }
    assert_eq!(version(&rt, client, session), version_before);
    let history_after = rt.session_read(client, session).unwrap().history_state();
    assert_eq!(history_after.undo_steps, history_before.undo_steps);
    assert_eq!(history_after.redo_steps, history_before.redo_steps);
}

#[test]
fn sampled_tasks_keep_frozen_provenance_across_publication_and_yield() {
    let (mut rt, client, session) = setup();
    add(&mut rt, client, session, 2);
    let source = QuerySource::Published(version(&rt, client, session));
    let input = zipzi_contract::kernel_tasks::SampleRequest {
        input: zipzi_contract::kernel_tasks::SampleInput::Parameters {
            values: vec![Default::default(), Default::default()],
        },
        queries: vec![],
        quality: true,
    };
    let job = rt.request_samples(client, source, input).unwrap();
    let work = rt.take_work(job).unwrap();
    add(&mut rt, client, session, 3);
    rt.complete(work.run()).unwrap();
    assert!(!rt.job(client, job).unwrap().state.is_terminal());
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    let JobValue::Read(result) = rt
        .job(client, job)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
    else {
        panic!("sample result")
    };
    assert_eq!(result.source, source);
    let value: serde_json::Value = serde_json::from_slice(result.bytes()).unwrap();
    assert_eq!(value["frames"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["frames"][0]["geometry"]["parameters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(value["frames"][0]["geometry"]["parameters"][0]["id"], id(2));
}

#[test]
fn exported_files_report_success_after_late_cancellation_without_changing_save_marker() {
    let (mut rt, client, session) = setup();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("layers.psd");
    std::fs::write(
        &input,
        include_bytes!("../../zipzi-psd/tests/fixtures/layered.psd"),
    )
    .unwrap();
    let expected = version(&rt, client, session);
    let import = rt
        .request_import(
            client,
            expected,
            &input,
            zipzi_contract::kernel_wire::ImportFormat::Psd,
            TaskBudget::default(),
        )
        .unwrap();
    let work = rt.take_work(import).unwrap();
    rt.complete(work.run()).unwrap();
    assert!(rt
        .job(client, import)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .is_ok());
    let source = QuerySource::Published(version(&rt, client, session));
    let path = directory.path().join("runtime");
    let job = rt
        .request_export(
            client,
            source,
            &path,
            zipzi_contract::kernel_tasks::ExportFormat::Runtime,
            false,
        )
        .unwrap();
    let work = rt.take_work(job).unwrap();
    let completed = work.run();
    rt.cancel_job(client, job).unwrap();
    rt.complete(completed).unwrap();
    let JobValue::Exported(receipt) = rt
        .job(client, job)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
    else {
        panic!("export result")
    };
    assert!(receipt.published);
    assert!(path.is_dir());
    assert!(rt
        .session_read(client, session)
        .unwrap()
        .project()
        .is_none());
}

#[cfg(feature = "gpu-observation")]
#[test]
fn observation_worker_returns_png_and_exact_frozen_source() {
    let (mut rt, client, session) = setup();
    let source = QuerySource::Published(version(&rt, client, session));
    let view = zipzi_contract::observation::ObservationView {
        texture_policy: Default::default(),
        width: 32,
        height: 32,
        roi: [-1., -1., 1., 1.],
        padding_model: 0.,
        topology: None,
        quality: false,
    };
    let input = zipzi_contract::kernel_tasks::SampleInput::Parameters {
        values: vec![Default::default()],
    };
    let job = rt.request_observation(client, source, input, view).unwrap();
    let work = rt.take_work(job).unwrap();
    add(&mut rt, client, session, 2);
    rt.complete(work.run()).unwrap();
    let JobValue::Read(result) = rt
        .job(client, job)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
    else {
        panic!("image")
    };
    assert_eq!(result.source, source);
    let length = u32::from_be_bytes(result.bytes()[..4].try_into().unwrap()) as usize;
    let header: serde_json::Value = serde_json::from_slice(&result.bytes()[4..4 + length]).unwrap();
    assert_eq!(header["source"], serde_json::to_value(source).unwrap());
    assert_eq!(
        &result.bytes()[4 + length..4 + length + 8],
        b"\x89PNG\r\n\x1a\n"
    );
}
