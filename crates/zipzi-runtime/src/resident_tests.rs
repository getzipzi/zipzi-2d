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
