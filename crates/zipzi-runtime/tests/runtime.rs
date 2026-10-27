use std::path::Path;
use std::sync::{mpsc, Arc};
use zipzi_core::{Canvas, Parameter, Vec2};
use zipzi_runtime::*;
use zipzi_sdk::{AuthoringSession, ObjectKind};

fn id(n: u32) -> String {
    format!("50000000-0000-4000-8000-{n:012x}")
}
fn canvas(width: f32) -> Canvas {
    Canvas::new(width, 100., Vec2::default(), 1.)
}
fn setup() -> (Runtime, ClientId, ClientId, SessionId) {
    let mut host = Runtime::default();
    let a = host.register_client().unwrap();
    let b = host.register_client().unwrap();
    let s = host.create_session(a, &id(1), canvas(100.)).unwrap();
    (host, a, b, s)
}
fn resize(host: &mut Runtime, client: ClientId, session: SessionId, width: f32) {
    let edit = host.begin_edit(client, session, "resize", None).unwrap();
    host.apply_edit(client, edit, |e| e.replace_model_setup(canvas(width)))
        .unwrap();
    host.commit_edit(client, edit).unwrap();
}
#[derive(Default)]
struct Manual(Vec<Work>);
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push(work);
        Ok(())
    }
}
fn finish(host: &mut Runtime) {
    let mut executor = Manual::default();
    host.dispatch(&mut executor);
    for work in executor.0 {
        assert!(host.complete(work.run()));
    }
}

#[test]
fn a03_two_clients_share_one_history_and_noop_cancel_failure_preserve_redo() {
    let (mut host, a, b, s) = setup();
    resize(&mut host, a, s, 120.);
    resize(&mut host, b, s, 140.);
    assert_eq!(host.session(s).unwrap().history_lengths(), (2, 0));
    host.undo(a, s).unwrap();
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(120.)));
    let noop = host.begin_edit(b, s, "no-op", None).unwrap();
    assert!(!host.commit_edit(b, noop).unwrap().changed);
    let cancel = host.begin_edit(b, s, "cancel", None).unwrap();
    host.apply_edit(b, cancel, |e| e.replace_model_setup(canvas(200.)))
        .unwrap();
    host.release_edit(b, cancel).unwrap();
    host.release_edit(b, cancel).unwrap();
    let bad = host.begin_edit(b, s, "fail", None).unwrap();
    assert!(host
        .apply_edit(b, bad, |e| e.erase_object(&id(999)))
        .is_err());
    host.release_edit(b, bad).unwrap();
    assert_eq!(host.session(s).unwrap().history_lengths(), (1, 1));
    host.redo(b, s).unwrap();
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(140.)));
}

#[test]
fn a04_candidate_read_after_write_isolated_and_caught_error_aborts_whole_edit() {
    let (mut host, a, _, s) = setup();
    let version = host.session(s).unwrap().version();
    let cursor = host.subscribe(s).unwrap().cursor;
    let edit = host.begin_edit(a, s, "composed", Some(version)).unwrap();
    host.apply_edit(a, edit, |e| {
        e.create_parameter(Parameter {
            id: id(10),
            ..Default::default()
        })
    })
    .unwrap();
    assert!(host
        .edit_document(a, edit)
        .unwrap()
        .get_parameter(&id(10))
        .is_some());
    assert!(host.document(s).unwrap().get_parameter(&id(10)).is_none());
    let snap = host.edit_snapshot(a, edit).unwrap();
    assert!(matches!(snap.source, SourceRef::Edit { sequence: 1, .. }));
    assert!(snap.snapshot.document().get_parameter(&id(10)).is_some());
    assert_eq!(
        host.apply_edit(a, edit, |e| {
            let _caught = e.erase_object(&id(999)).unwrap_err();
            Ok(())
        }),
        Err(RuntimeError::Aborted)
    );
    assert_eq!(host.commit_edit(a, edit), Err(RuntimeError::Aborted));
    assert_eq!(host.session(s).unwrap().version(), version);
    assert_eq!(host.session(s).unwrap().history_lengths(), (0, 0));
    assert!(host.events_since(cursor).unwrap().is_empty());
    assert!(snap.snapshot.document().get_parameter(&id(10)).is_some());
}

#[test]
fn a05_stale_commit_uses_sdk_full_version_validation() {
    let (mut host, a, b, s) = setup();
    let stale = host.begin_edit(a, s, "old candidate", None).unwrap();
    host.apply_edit(a, stale, |e| e.replace_model_setup(canvas(300.)))
        .unwrap();
    resize(&mut host, b, s, 120.);
    let actual = host.session(s).unwrap().version();
    let RuntimeError::Sdk(error) = host.commit_edit(a, stale).unwrap_err() else {
        panic!("SDK must reject stale commit");
    };
    assert_eq!(error.code.as_ref(), "STALE_VERSION");
    assert_eq!(error.actual_version.as_deref(), Some(&actual));
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(120.)));
    assert_eq!(host.session(s).unwrap().history_lengths(), (1, 0));
}

#[test]
fn a05_handle_survives_rename_but_delete_undo_never_revives_it() {
    let (mut host, a, b, s) = setup();
    let edit = host.begin_edit(a, s, "create", None).unwrap();
    host.apply_edit(a, edit, |e| {
        e.create_parameter(Parameter {
            id: id(10),
            ..Default::default()
        })
    })
    .unwrap();
    host.commit_edit(a, edit).unwrap();
    let handle = host
        .session(s)
        .unwrap()
        .handle(ObjectKind::Parameter, &id(10))
        .unwrap();
    let edit = host.begin_edit(b, s, "rename", None).unwrap();
    host.apply_edit(b, edit, |e| {
        let mut parameter = e
            .candidate_document()
            .get_parameter(&id(10))
            .unwrap()
            .clone();
        parameter.name = "renamed".into();
        e.replace_parameter(parameter)
    })
    .unwrap();
    host.commit_edit(b, edit).unwrap();
    host.session(s).unwrap().resolve_handle(&handle).unwrap();
    let edit = host.begin_edit(a, s, "delete", None).unwrap();
    host.apply_edit(a, edit, |e| e.erase_object(&id(10)))
        .unwrap();
    host.commit_edit(a, edit).unwrap();
    host.undo(b, s).unwrap();
    assert_eq!(
        host.session(s)
            .unwrap()
            .resolve_handle(&handle)
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_HANDLE"
    );
    assert!(host.document(s).unwrap().get_parameter(&id(10)).is_some());
}

#[test]
fn ownership_ids_client_cleanup_and_close_are_explicit() {
    let (mut host, a, b, s) = setup();
    let edit = host.begin_edit(a, s, "a", None).unwrap();
    assert!(matches!(
        host.edit_document(b, edit),
        Err(RuntimeError::WrongOwner)
    ));
    assert_eq!(host.release_edit(b, edit), Err(RuntimeError::WrongOwner));
    let other = Runtime::default();
    assert_ne!(host.instance_id(), other.instance_id());
    assert!(matches!(
        other.session(s),
        Err(RuntimeError::InvalidReference)
    ));
    host.release_client(a); // leaves the formal unsaved document alive
    assert!(host.session(s).is_ok());
    host.attach_session(b, s).unwrap();
    host.detach_session(b, s).unwrap();
    resize(&mut host, b, s, 120.);
    assert_eq!(host.close_session(b, s, false), Err(RuntimeError::Modified));
    let closed = host.close_session(b, s, true).unwrap();
    assert_eq!(closed.state, SessionState::Closed);
    assert!(host.list_sessions().is_empty());
    assert!(matches!(
        host.session(s),
        Err(RuntimeError::InvalidReference)
    ));
}

#[test]
fn close_checks_other_users_and_pending_candidates() {
    let (mut host, a, b, s) = setup();
    host.attach_session(b, s).unwrap();
    assert_eq!(host.close_session(a, s, true), Err(RuntimeError::Busy));
    host.detach_session(b, s).unwrap();
    let edit = host.begin_edit(b, s, "candidate", None).unwrap();
    assert_eq!(host.close_session(a, s, true), Err(RuntimeError::Busy));
    host.release_edit(b, edit).unwrap();
    assert_eq!(
        host.close_session(a, s, true).unwrap().state,
        SessionState::Closed
    );
}

#[test]
fn a10_atomic_snapshot_cursor_receipts_once_and_resync() {
    let limits = RuntimeLimits {
        session_events: 2,
        registry_events: 1,
        ..Default::default()
    };
    let mut host = Runtime::new(limits);
    let a = host.register_client().unwrap();
    let registry = host.subscribe_registry();
    let s = host.create_session(a, &id(1), canvas(100.)).unwrap();
    let subscription = host.subscribe(s).unwrap();
    resize(&mut host, a, s, 120.);
    let events = host.events_since(subscription.cursor).unwrap();
    assert_eq!(events.len(), 1);
    let EventKind::Published(receipt) = &events[0].kind else {
        panic!()
    };
    assert_eq!(receipt.before, subscription.snapshot.snapshot.version());
    assert_eq!(events[0].client, Some(a));
    assert_eq!(events[0].operation.runtime_instance(), host.instance_id());
    let cursor = events[0].cursor;
    resize(&mut host, a, s, 140.);
    resize(&mut host, a, s, 160.);
    assert!(matches!(
        host.events_since(subscription.cursor),
        Err(RuntimeError::ResyncRequired)
    ));
    assert_eq!(host.events_since(cursor).unwrap().len(), 2);
    let fresh = host.subscribe(s).unwrap();
    assert!(host.events_since(fresh.cursor).unwrap().is_empty());
    assert_eq!(
        fresh.snapshot.snapshot.document().source_canvas(),
        Some(canvas(160.))
    );
    host.create_session(a, &id(2), canvas(100.)).unwrap();
    assert!(matches!(
        host.events_since(registry.cursor),
        Err(RuntimeError::ResyncRequired)
    ));
    let mut future = fresh.cursor;
    future.sequence += 1;
    assert!(matches!(
        host.events_since(future),
        Err(RuntimeError::ResyncRequired)
    ));
    let foreign = Runtime::default().subscribe_registry().cursor;
    assert!(matches!(
        host.events_since(foreign),
        Err(RuntimeError::ResyncRequired)
    ));
}

#[test]
fn a10_oversized_event_requires_resync_instead_of_silent_loss() {
    let mut host = Runtime::new(RuntimeLimits {
        session_event_bytes: 1,
        ..Default::default()
    });
    let a = host.register_client().unwrap();
    let s = host.create_session(a, &id(1), canvas(100.)).unwrap();
    let before = host.subscribe(s).unwrap().cursor;
    resize(&mut host, a, s, 120.);
    assert!(matches!(
        host.events_since(before),
        Err(RuntimeError::ResyncRequired)
    ));
    assert!(host
        .events_since(host.subscribe(s).unwrap().cursor)
        .unwrap()
        .is_empty());
}

#[test]
fn candidate_budgets_apply_across_clients_and_fail_without_formal_publication() {
    let (mut host, a, b, s) = setup();
    let edit = host.begin_edit(a, s, "first", None).unwrap();
    assert!(matches!(
        host.begin_edit(a, s, "duplicate", None),
        Err(RuntimeError::Budget(_))
    ));
    host.begin_edit(b, s, "other", None).unwrap();
    host.release_edit(a, edit).unwrap();
    let baseline = host
        .session(s)
        .unwrap()
        .begin_owned_edit("x", None)
        .unwrap()
        .estimated_bytes();
    let mut host = Runtime::new(RuntimeLimits {
        candidate_bytes: baseline + 128,
        ..Default::default()
    });
    let a = host.register_client().unwrap();
    let b = host.register_client().unwrap();
    let s = host.create_session(a, &id(1), canvas(100.)).unwrap();
    let edit = host.begin_edit(a, s, "x", None).unwrap();
    assert!(matches!(
        host.begin_edit(b, s, "x", None),
        Err(RuntimeError::Budget(_))
    ));
    let version = host.session(s).unwrap().version();
    assert!(matches!(
        host.apply_edit(a, edit, |e| e.create_parameter(Parameter {
            id: id(10),
            name: "x".repeat(4096),
            ..Default::default()
        })),
        Err(RuntimeError::Budget(_))
    ));
    assert_eq!(host.edit_state(a, edit).unwrap(), EditState::Aborted);
    assert_eq!(host.session(s).unwrap().version(), version);
    assert_eq!(host.session(s).unwrap().history_lengths(), (0, 0));
}

#[test]
fn frozen_and_job_budgets_do_not_consume_candidate_on_busy_or_admission_failure() {
    let mut host = Runtime::new(RuntimeLimits {
        frozen_bytes: 0,
        ..Default::default()
    });
    let a = host.register_client().unwrap();
    let s = host.create_session(a, &id(1), canvas(100.)).unwrap();
    let edit = host.begin_edit(a, s, "candidate", None).unwrap();
    assert!(matches!(
        host.request_commit(a, edit),
        Err(RuntimeError::Budget(_))
    ));
    assert_eq!(host.edit_state(a, edit).unwrap(), EditState::Editing);
    host.commit_edit(a, edit).unwrap();
    assert_eq!(host.session_info(s).unwrap().state, SessionState::Ready);
}

#[test]
fn a02_path_aliases_reuse_session_and_copies_have_distinct_identity() {
    let (mut host, a, b, s) = setup();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("project");
    let save = host.request_save(a, s, &target).unwrap();
    finish(&mut host);
    assert_eq!(host.job_info(a, save).unwrap().state, JobState::Succeeded);
    assert_eq!(
        host.request_open(b, &target).unwrap(),
        OpenRequest::Existing(s)
    );
    assert_eq!(
        host.request_open(b, &target.join("project.zipzi.json"))
            .unwrap(),
        OpenRequest::Existing(s)
    );
    let alias = dir.path().join("alias");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert_eq!(
            host.request_open(b, &alias).unwrap(),
            OpenRequest::Existing(s)
        );
    }
    let copy = dir.path().join("copy");
    std::fs::create_dir(&copy).unwrap();
    std::fs::copy(
        target.join("project.zipzi.json"),
        copy.join("project.zipzi.json"),
    )
    .unwrap();
    let OpenRequest::Pending(job) = host.request_open(b, &copy).unwrap() else {
        panic!()
    };
    finish(&mut host);
    let JobValue::Opened(other) = host.job_info(b, job).unwrap().result.unwrap().unwrap() else {
        panic!()
    };
    assert_ne!(other, s);
    assert_eq!(host.list_sessions().len(), 2);
}

#[test]
fn a02_cross_session_save_as_and_open_reservations_release_only_on_terminal() {
    let (mut host, a, b, s) = setup();
    let dir = tempfile::tempdir().unwrap();
    let t = host.create_session(b, &id(2), canvas(200.)).unwrap();
    let old = dir.path().join("old");
    host.request_save(a, s, &old).unwrap();
    finish(&mut host);
    resize(&mut host, a, s, 120.);
    let target = dir.path().join("new");
    let save = host.request_save(a, s, &target).unwrap();
    assert_eq!(host.request_save(b, t, &target), Err(RuntimeError::Busy));
    assert_eq!(host.request_open(b, &target), Err(RuntimeError::Busy));
    assert_eq!(host.request_save(b, t, &old), Err(RuntimeError::Busy));
    assert!(!target.exists());
    host.cancel_job(a, save).unwrap();
    assert_eq!(host.request_save(b, t, &target), Err(RuntimeError::Busy));
    assert_eq!(host.close_session(a, s, true), Err(RuntimeError::Busy));
    finish(&mut host);
    assert_eq!(host.job_info(a, save).unwrap().state, JobState::Cancelled);
    assert_eq!(
        host.session_info(s).unwrap().path.as_deref(),
        Some(zipzi_project::project_identity(&old).unwrap().as_path())
    );
    assert!(host.session(s).unwrap().modified());
    let second = host.request_save(b, t, &target).unwrap();
    finish(&mut host);
    assert_eq!(host.job_info(b, second).unwrap().state, JobState::Succeeded);
    let manifest = std::fs::read(target.join("project.zipzi.json")).unwrap();
    assert_eq!(host.request_save(a, s, &target), Err(RuntimeError::Busy));
    assert_eq!(
        std::fs::read(target.join("project.zipzi.json")).unwrap(),
        manifest
    );
    assert_eq!(host.session(t).unwrap().source_canvas(), Some(canvas(200.)));
}

#[test]
fn a02_inflight_open_blocks_save_as_and_failure_allows_retry() {
    let (mut host, a, b, s) = setup();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("missing");
    let OpenRequest::Pending(job) = host.request_open(b, &target).unwrap() else {
        panic!()
    };
    assert_eq!(
        host.request_open(a, &target.join("project.zipzi.json")),
        Err(RuntimeError::Busy)
    );
    assert_eq!(host.request_save(a, s, &target), Err(RuntimeError::Busy));
    assert_eq!(host.subscribe_registry().sessions.len(), 1);
    finish(&mut host);
    assert_eq!(host.job_info(b, job).unwrap().state, JobState::Failed);
    assert_eq!(host.list_sessions().len(), 1);
    let save = host.request_save(a, s, &target).unwrap();
    finish(&mut host);
    assert_eq!(host.job_info(a, save).unwrap().state, JobState::Succeeded);
    assert_eq!(
        host.request_open(b, &target).unwrap(),
        OpenRequest::Existing(s)
    );
}

#[test]
fn a08_background_commit_busy_preserves_other_candidate_and_freezes_reads() {
    let (mut host, a, b, s) = setup();
    let first = host.begin_edit(a, s, "first", None).unwrap();
    host.apply_edit(a, first, |e| e.replace_model_setup(canvas(120.)))
        .unwrap();
    let second = host.begin_edit(b, s, "second", None).unwrap();
    let job = host.request_commit(a, first).unwrap();
    assert_eq!(host.commit_edit(b, second), Err(RuntimeError::Busy));
    assert_eq!(host.edit_state(b, second).unwrap(), EditState::Editing);
    assert!(matches!(host.document(s), Err(RuntimeError::Busy)));
    let frozen = host.source_snapshot(s).unwrap();
    assert_eq!(frozen.state, SessionState::Busy(job));
    assert_eq!(
        frozen.snapshot.document().source_canvas(),
        Some(canvas(100.))
    );
    finish(&mut host);
    assert_eq!(host.job_info(a, job).unwrap().state, JobState::Succeeded);
    assert!(matches!(
        host.commit_edit(b, second),
        Err(RuntimeError::Sdk(_))
    ));
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(120.)));
    assert_eq!(host.session(s).unwrap().history_lengths(), (1, 0));
}

#[test]
fn a08_normal_sdk_save_failure_returns_same_session_and_history_then_retry() {
    let (mut host, a, _, s) = setup();
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    host.request_save(a, s, &old).unwrap();
    finish(&mut host);
    resize(&mut host, a, s, 120.);
    let version = host.session(s).unwrap().version();
    let parent = dir.path().join("blocked");
    std::fs::create_dir_all(parent.join("project.zipzi.json")).unwrap();
    let job = host.request_save(a, s, &parent).unwrap();
    finish(&mut host);
    assert_eq!(host.job_info(a, job).unwrap().state, JobState::Failed);
    assert_eq!(host.session_info(s).unwrap().state, SessionState::Ready);
    assert_eq!(host.session(s).unwrap().version(), version);
    assert_eq!(
        host.session_info(s).unwrap().path,
        Some(zipzi_project::project_identity(&old).unwrap())
    );
    assert!(host.session(s).unwrap().modified());
    assert_eq!(host.session(s).unwrap().history_lengths(), (1, 0));
    std::fs::remove_dir(parent.join("project.zipzi.json")).unwrap();
    host.request_save(a, s, &parent).unwrap();
    finish(&mut host);
    assert!(!host.session(s).unwrap().modified());
    host.undo(a, s).unwrap();
    assert!(host.session(s).unwrap().modified());
}

struct PanicFilesystem;
impl zipzi_sdk::FileSystem for PanicFilesystem {
    fn write_new(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
        panic!("injected worker failure")
    }
    fn sync_directory(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn rename(&self, _: &Path, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn a08_real_thread_worker_panic_faults_only_its_session_and_retains_frozen_source() {
    struct ThreadExecutor(mpsc::Sender<Completion>);
    impl Executor for ThreadExecutor {
        fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
            let tx = self.0.clone();
            std::thread::spawn(move || tx.send(work.run()).unwrap());
            Ok(())
        }
    }
    let (mut host, a, b, other) = setup();
    let dir = tempfile::tempdir().unwrap();
    let sdk =
        AuthoringSession::with_filesystem(&id(2), canvas(200.), Arc::new(PanicFilesystem)).unwrap();
    let s = host.adopt_session(a, sdk).unwrap();
    let job = host
        .request_save(a, s, &dir.path().join("project"))
        .unwrap();
    let (tx, rx) = mpsc::channel();
    host.dispatch(&mut ThreadExecutor(tx));
    resize(&mut host, b, other, 120.);
    assert!(host.complete(rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap()));
    let info = host.job_info(a, job).unwrap();
    assert_eq!(info.state, JobState::Failed);
    assert!(matches!(
        info.result,
        Some(Err(RuntimeError::WorkerPanic(_)))
    ));
    assert_eq!(
        host.session_info(s).unwrap().state,
        SessionState::Faulted(job)
    );
    assert!(matches!(
        host.begin_edit(a, s, "no", None),
        Err(RuntimeError::Faulted)
    ));
    assert_eq!(
        host.request_save(a, s, dir.path()),
        Err(RuntimeError::Faulted)
    );
    assert_eq!(host.undo(a, s), Err(RuntimeError::Faulted));
    let frozen = host.source_snapshot(s).unwrap();
    assert_eq!(frozen.state, SessionState::Faulted(job));
    assert_eq!(
        frozen.snapshot.document().source_canvas(),
        Some(canvas(200.))
    );
    host.cancel_job(a, job).unwrap();
    assert_eq!(
        host.session_info(s).unwrap().state,
        SessionState::Faulted(job)
    );
    host.release_client(a);
    assert!(matches!(
        host.session_info(s).unwrap().fault,
        Some(RuntimeError::WorkerPanic(_))
    ));
    host.close_session(b, s, true).unwrap();
    assert_eq!(
        host.session(other).unwrap().source_canvas(),
        Some(canvas(120.))
    );
}

#[test]
fn a08_background_save_result_survives_client_and_view_detach_and_success_wins_cancel() {
    let (mut host, a, b, s) = setup();
    let dir = tempfile::tempdir().unwrap();
    resize(&mut host, a, s, 120.);
    let cursor = host.subscribe(s).unwrap().cursor;
    let job = host.request_save(a, s, dir.path()).unwrap();
    let mut executor = Manual::default();
    host.dispatch(&mut executor);
    // Session detach cannot abandon a live Job. Client expiry (or closing the
    // initiating view) still must not discard an already executed save.
    assert_eq!(host.detach_session(a, s), Err(RuntimeError::Busy));
    host.detach_editor_view(a, s).unwrap();
    let completion = executor.0.pop().unwrap().run(); // publication already happened
    host.cancel_job(a, job).unwrap();
    host.release_client(a);
    assert!(host.complete(completion));
    assert!(!host.session(s).unwrap().modified());
    assert_eq!(host.session(s).unwrap().history_lengths(), (1, 0));
    host.attach_session(b, s).unwrap();
    let events = host.events_since(cursor).unwrap();
    assert!(events.iter().all(|e| e.operation == job.into()));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Saved(_)))
            .count(),
        1
    );
    assert_eq!(
        host.request_open(b, dir.path()).unwrap(),
        OpenRequest::Existing(s)
    );
}

#[test]
fn executor_rejection_and_concurrency_limit_keep_ownership_and_queued_work() {
    struct Reject;
    impl Executor for Reject {
        fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
            Err(Box::new(work))
        }
    }
    let mut host = Runtime::new(RuntimeLimits {
        running_jobs: 1,
        ..Default::default()
    });
    let a = host.register_client().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let s = host.create_session(a, &id(1), canvas(100.)).unwrap();
    let t = host.create_session(a, &id(2), canvas(200.)).unwrap();
    let first = host.request_save(a, s, &dir.path().join("s")).unwrap();
    let second = host.request_save(a, t, &dir.path().join("t")).unwrap();
    host.dispatch(&mut Reject);
    assert_eq!(host.job_info(a, first).unwrap().state, JobState::Queued);
    let mut executor = Manual::default();
    host.dispatch(&mut executor);
    assert_eq!(executor.0.len(), 1);
    assert_eq!(host.job_info(a, first).unwrap().state, JobState::Running);
    assert_eq!(host.job_info(a, second).unwrap().state, JobState::Queued);
    assert_eq!(host.release_job(a, first), Err(RuntimeError::Busy));
    assert!(host.complete(executor.0.pop().unwrap().run()));
    host.dispatch(&mut executor);
    assert_eq!(executor.0.len(), 1);
    assert!(host.complete(executor.0.pop().unwrap().run()));
    host.release_job(a, first).unwrap();
    host.release_job(a, first).unwrap();
    assert_eq!(host.session_info(s).unwrap().state, SessionState::Ready);
    assert_eq!(host.session_info(t).unwrap().state, SessionState::Ready);
}

#[test]
fn save_is_exclusive_with_candidates_and_success_switches_path_without_content_revision() {
    let (mut host, a, b, s) = setup();
    let dir = tempfile::tempdir().unwrap();
    let edit = host.begin_edit(b, s, "candidate", None).unwrap();
    assert_eq!(host.request_save(a, s, dir.path()), Err(RuntimeError::Busy));
    host.release_edit(b, edit).unwrap();
    let version = host.session(s).unwrap().version();
    host.request_save(a, s, &dir.path().join("old")).unwrap();
    finish(&mut host);
    host.request_save(a, s, &dir.path().join("new")).unwrap();
    finish(&mut host);
    assert_eq!(host.session(s).unwrap().version(), version);
    assert_eq!(
        host.session_info(s).unwrap().path,
        Some(zipzi_project::project_identity(&dir.path().join("new")).unwrap())
    );
    // The previous path is available to a new session only after successful Save As.
    let OpenRequest::Pending(job) = host.request_open(b, &dir.path().join("old")).unwrap() else {
        panic!()
    };
    finish(&mut host);
    assert_eq!(host.job_info(b, job).unwrap().state, JobState::Succeeded);
}

#[test]
fn r0_enabled_defaults_match_frozen_configuration() {
    let defaults: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/runtime/defaults.json")).unwrap();
    let limits = RuntimeLimits::default();
    for (actual, expected) in [
        (
            limits.clients,
            defaults["objects"]["clients"].as_u64().unwrap(),
        ),
        (
            limits.sessions,
            defaults["objects"]["sessions"].as_u64().unwrap(),
        ),
        (
            limits.edits_total,
            defaults["objects"]["edits_total"].as_u64().unwrap(),
        ),
        (
            limits.candidate_bytes,
            defaults["bytes"]["candidate_total"].as_u64().unwrap(),
        ),
        (
            limits.running_jobs,
            defaults["jobs"]["running_heavy_total"].as_u64().unwrap(),
        ),
        (
            limits.session_events,
            defaults["transport"]["session_events"].as_u64().unwrap(),
        ),
    ] {
        assert_eq!(actual as u64, expected);
    }
}

#[test]
fn caught_host_apply_panic_cannot_publish_partial_candidate() {
    let (mut host, a, _, s) = setup();
    let edit = host.begin_edit(a, s, "panic", None).unwrap();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<()> = host.apply_edit(a, edit, |e| {
            e.replace_model_setup(canvas(300.))?;
            panic!("caller panicked during candidate work");
        });
    }));
    assert!(failed.is_err());
    assert_eq!(host.commit_edit(a, edit), Err(RuntimeError::Aborted));
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(100.)));
    assert_eq!(host.session(s).unwrap().history_lengths(), (0, 0));
}

#[test]
fn independent_candidate_computation_continues_while_formal_job_is_busy() {
    let (mut host, a, b, s) = setup();
    let first = host.begin_edit(a, s, "first", None).unwrap();
    let second = host.begin_edit(b, s, "second", None).unwrap();
    host.request_commit(a, first).unwrap();
    host.apply_edit(b, second, |e| e.replace_model_setup(canvas(200.)))
        .unwrap();
    assert_eq!(
        host.edit_document(b, second).unwrap().source_canvas(),
        Some(canvas(200.))
    );
    let frozen = host.source_snapshot(s).unwrap();
    assert!(Arc::ptr_eq(
        &frozen.snapshot,
        &host.source_snapshot(s).unwrap().snapshot
    ));
    finish(&mut host); // no-op leaves the formal Version unchanged
    host.commit_edit(b, second).unwrap();
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(200.)));
}

#[test]
fn in_flight_publications_reserve_shared_budget_until_confirmed_terminal() {
    let sdk = AuthoringSession::new(&id(1), canvas(100.)).unwrap();
    let growth = (sdk.estimated_content_bytes()
        + sdk.begin_owned_edit("x", None).unwrap().estimated_bytes())
        * 2;
    let mut host = Runtime::new(RuntimeLimits {
        history_bytes: growth + 1,
        ..Default::default()
    });
    let a = host.register_client().unwrap();
    let s = host.create_session(a, &id(1), canvas(100.)).unwrap();
    let t = host.create_session(a, &id(2), canvas(100.)).unwrap();
    let first = host.begin_edit(a, s, "x", None).unwrap();
    let second = host.begin_edit(a, t, "x", None).unwrap();
    let job = host.request_commit(a, first).unwrap();
    assert!(matches!(
        host.request_commit(a, second),
        Err(RuntimeError::Budget(_))
    ));
    assert_eq!(host.edit_state(a, second).unwrap(), EditState::Editing);
    host.cancel_job(a, job).unwrap();
    assert!(matches!(
        host.request_commit(a, second),
        Err(RuntimeError::Budget(_))
    ));
    finish(&mut host);
    host.request_commit(a, second).unwrap();
    finish(&mut host);
    assert_eq!(host.session_info(t).unwrap().state, SessionState::Ready);
}

#[test]
fn sdk_commit_budget_failure_never_publishes_a_partially_validated_candidate() {
    let mut host = Runtime::default();
    let a = host.register_client().unwrap();
    let sdk = AuthoringSession::with_history_limits(
        &id(1),
        canvas(100.),
        zipzi_sdk::HistoryLimits {
            max_steps: 0,
            max_bytes: 256 << 20,
        },
    )
    .unwrap();
    let s = host.adopt_session(a, sdk).unwrap();
    let subscription = host.subscribe(s).unwrap();
    let edit = host.begin_edit(a, s, "cannot publish", None).unwrap();
    host.apply_edit(a, edit, |e| e.replace_model_setup(canvas(200.)))
        .unwrap();
    let diagnostic = host.edit_snapshot(a, edit).unwrap();
    let RuntimeError::Sdk(error) = host.commit_edit(a, edit).unwrap_err() else {
        panic!("SDK must check its own commit budget");
    };
    assert_eq!(error.code.as_ref(), "HISTORY_LIMIT_EXCEEDED");
    assert_eq!(
        host.session(s).unwrap().version(),
        subscription.info.version
    );
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(100.)));
    assert_eq!(host.session(s).unwrap().history_lengths(), (0, 0));
    assert!(host.events_since(subscription.cursor).unwrap().is_empty());
    assert_eq!(
        diagnostic.snapshot.document().source_canvas(),
        Some(canvas(200.))
    );
}

#[test]
fn a05_stale_plan_and_field_address_keep_sdk_rejection_semantics() {
    use zipzi_sdk::{DocumentAuthoringEdit, DocumentField, DocumentValue};
    let (mut host, a, b, s) = setup();
    let address = host
        .session(s)
        .unwrap()
        .document_field_address(DocumentField::ExportSettings)
        .unwrap();
    let settings = zipzi_core::ExportSettings {
        pixels_per_unit: 2.,
        ..Default::default()
    };
    let plan = host
        .session(s)
        .unwrap()
        .plan_document_field_patch(&address, DocumentValue::ExportSettings(settings))
        .unwrap();
    let first = host.begin_edit(a, s, "settings", None).unwrap();
    host.apply_edit(a, first, |e| e.apply_document_plan(&plan))
        .unwrap();
    host.commit_edit(a, first).unwrap();
    host.undo(b, s).unwrap(); // restoring the value must not revive its old address
    assert_eq!(
        host.session(s)
            .unwrap()
            .plan_document_field_patch(&address, DocumentValue::ExportSettings(settings))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );
    let stale = host.begin_edit(b, s, "stale plan", None).unwrap();
    let RuntimeError::Sdk(error) = host
        .apply_edit(b, stale, |e| e.apply_document_plan(&plan))
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(error.code.as_ref(), "STALE_PLAN");
    assert_eq!(host.session(s).unwrap().history_lengths(), (0, 1));
    host.release_edit(b, stale).unwrap();
    // Query/planning also works from the hosted SDK's immutable typed borrow.
    host.session(s)
        .unwrap()
        .plan_document_edit(DocumentAuthoringEdit::Patch {
            expected: DocumentValue::ExportSettings(Default::default()),
            value: DocumentValue::ExportSettings(settings),
        })
        .unwrap();
}

#[test]
fn prepared_edits_are_bounded_but_only_one_writable_per_client() {
    let (mut host, a, b, s) = setup();
    let active = host.begin_edit(a, s, "gesture", None).unwrap();
    host.begin_interaction(a, s).unwrap();
    let prepared = host.prepare_edit(a, s, "validate next", None).unwrap();
    assert!(matches!(
        host.promote_edit(a, prepared),
        Err(RuntimeError::Budget(_))
    ));
    let external = host.begin_edit(b, s, "agent", None).unwrap();
    assert_eq!(host.commit_edit(b, external), Err(RuntimeError::Busy));
    assert!(host.candidate(b, external).is_ok());
    host.release_edit(a, active).unwrap();
    host.promote_edit(a, prepared).unwrap();
    let mut work = host.lease_candidate(a, prepared).unwrap();
    assert_eq!(
        host.lease_candidate(a, prepared).err(),
        Some(RuntimeError::Busy)
    );
    work.apply(|edit| edit.replace_model_setup(canvas(150.)))
        .unwrap();
    host.return_candidate(work, false, false).unwrap();
    host.commit_edit(a, prepared).unwrap();
    host.end_interaction(a, s).unwrap();
    assert!(matches!(
        host.commit_edit(b, external),
        Err(RuntimeError::Sdk(_))
    ));
    assert_eq!(host.session(s).unwrap().history_lengths(), (1, 0));
}

#[test]
fn candidate_lease_failure_and_repeated_lifecycle_leave_no_workspaces() {
    let (mut host, a, _, s) = setup();
    let before = host.accounted_bytes();
    for _ in 0..100 {
        let id = host.prepare_edit(a, s, "cancel", None).unwrap();
        let mut work = host.lease_candidate(a, id).unwrap();
        work.apply(|edit| edit.replace_model_setup(canvas(180.)))
            .unwrap();
        host.return_candidate(work, true, false).unwrap();
        assert_eq!(host.commit_edit(a, id), Err(RuntimeError::Aborted));
        host.release_edit(a, id).unwrap();
    }
    assert_eq!(host.accounted_bytes(), before);
    assert_eq!(host.session(s).unwrap().history_lengths(), (0, 0));
}

#[test]
fn typed_plan_admission_failure_does_not_leak_an_unreachable_candidate() {
    use zipzi_sdk::{DocumentAuthoringEdit, DocumentValue};
    let mut host = Runtime::new(RuntimeLimits {
        history_bytes: 1,
        ..Default::default()
    });
    let client = host.register_client().unwrap();
    let session = host.create_session(client, &id(1), canvas(100.)).unwrap();
    let mut settings = host.document(session).unwrap().export_settings();
    settings.pixels_per_unit = 2.;
    let plan = host
        .session(session)
        .unwrap()
        .plan_document_edit(DocumentAuthoringEdit::Patch {
            expected: DocumentValue::ExportSettings(Default::default()),
            value: DocumentValue::ExportSettings(settings),
        })
        .unwrap();
    let before = host.accounted_bytes();
    for _ in 0..100 {
        assert!(matches!(
            host.execute_document_plan(client, session, &plan),
            Err(RuntimeError::Budget(_))
        ));
        assert_eq!(host.accounted_bytes(), before);
    }
    assert_eq!(host.session(session).unwrap().history_lengths(), (0, 0));
}

#[test]
fn typed_resource_writes_obey_budgets_without_consuming_loads() {
    use zipzi_core::document::{AttachmentEdit, ResourceEdit};
    let image = zipzi_sdk::prepare_image_load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../zipzi-sdk/tests/fixtures/asymmetric-2x2.png"),
    )
    .unwrap();
    for budget in ["history", "candidate", "slots", "allowed"] {
        let mut sdk = AuthoringSession::new(&id(1), canvas(100.)).unwrap();
        let plan = sdk
            .plan_attachment_edit(AttachmentEdit::Create {
                id: id(20),
                path: "voice.wav".into(),
            })
            .unwrap();
        sdk.execute_attachment_plan(&plan).unwrap();
        let plan = sdk
            .plan_resource_edit(ResourceEdit::Create {
                id: id(10),
                name: "texture".into(),
                before: None,
            })
            .unwrap();
        sdk.execute_resource_plan(&plan).unwrap();
        // Keep a redo branch as well as history to detect rejected publication.
        sdk.edit("resize", None, |e| e.replace_model_setup(canvas(200.)))
            .unwrap();
        sdk.undo().unwrap();
        let mut limits = RuntimeLimits::default();
        match budget {
            "history" => limits.history_bytes = sdk.history_state().estimated_bytes + 1,
            "candidate" => limits.candidate_bytes = 1,
            "slots" => limits.edits_total = 0,
            _ => {}
        }
        let mut runtime = Runtime::new(limits);
        let client = runtime.register_client().unwrap();
        let session = runtime.adopt_session(client, sdk).unwrap();
        let version = runtime.session(session).unwrap().version();
        let history = runtime.session(session).unwrap().history_lengths();
        let cursor = runtime.event_cursor(session).unwrap();
        let image_token = runtime
            .session(session)
            .unwrap()
            .image_load_token(&id(10))
            .unwrap();
        let attachment_token = runtime
            .session(session)
            .unwrap()
            .attachment_load_token(&id(20))
            .unwrap();
        let results = [
            runtime.import_prepared_image(client, session, id(11), "new".into(), &image),
            runtime.replace_prepared_image_content(client, session, &image_token, &image),
            runtime.replace_attachment_content(
                client,
                session,
                &attachment_token,
                Some(Arc::new(vec![1, 2, 3])),
            ),
        ];
        if budget == "allowed" {
            assert!(results.iter().all(Result::is_ok));
            assert_eq!(
                runtime
                    .session(session)
                    .unwrap()
                    .read_texture(&id(10))
                    .unwrap()
                    .width,
                2
            );
            assert_eq!(
                runtime
                    .session(session)
                    .unwrap()
                    .read_texture(&id(11))
                    .unwrap()
                    .width,
                2
            );
            assert_eq!(
                runtime
                    .session(session)
                    .unwrap()
                    .document()
                    .get_attachment(&id(20))
                    .unwrap()
                    .bytes
                    .as_deref()
                    .unwrap(),
                &[1, 2, 3]
            );
        } else {
            for result in results {
                assert!(
                    matches!(result, Err(RuntimeError::Budget(_))),
                    "{budget}: {result:?}"
                );
            }
            assert_eq!(runtime.session(session).unwrap().version(), version);
            assert_eq!(runtime.session(session).unwrap().history_lengths(), history);
            assert_eq!(runtime.event_cursor(session).unwrap(), cursor);
            // A rejected load's token remains valid for SDK staging/retry.
            runtime
                .session(session)
                .unwrap()
                .begin_prepared_image_content_edit(&image_token, &image)
                .unwrap();
            runtime
                .session(session)
                .unwrap()
                .begin_attachment_content_edit(&attachment_token, Some(Arc::new(vec![4])))
                .unwrap();
        }
    }
}

#[test]
fn confirmed_replacement_respects_open_session_ownership_and_retains_history() {
    let (mut host, a, b, s) = setup();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("replace.zipzi");
    let manifest = target.join("project.zipzi.json");
    let owner = host.create_session(b, &id(2), canvas(200.)).unwrap();
    host.request_save(b, owner, &target).unwrap();
    finish(&mut host);
    let bytes = std::fs::read(&manifest).unwrap();
    let hash = zipzi_project::content_sha256(&bytes);
    assert_eq!(
        host.request_save_replacing(a, s, &target, &hash),
        Err(RuntimeError::Busy)
    );
    assert_eq!(std::fs::read(&manifest).unwrap(), bytes);
    host.close_session(b, owner, true).unwrap();

    resize(&mut host, a, s, 120.);
    let version = host.session(s).unwrap().version();
    let job = host.request_save_replacing(a, s, &target, &hash).unwrap();
    assert_eq!(host.request_open(b, &target), Err(RuntimeError::Busy));
    finish(&mut host);
    assert_eq!(host.job_info(a, job).unwrap().state, JobState::Succeeded);
    assert_eq!(host.session(s).unwrap().version(), version);
    assert_eq!(host.session(s).unwrap().history_lengths(), (1, 0));
    assert!(!host.session(s).unwrap().modified());
    assert_eq!(
        host.request_open(b, &target).unwrap(),
        OpenRequest::Existing(s)
    );
    host.undo(a, s).unwrap();
    assert_eq!(host.session(s).unwrap().source_canvas(), Some(canvas(100.)));
    assert!(host.session(s).unwrap().modified());
}
