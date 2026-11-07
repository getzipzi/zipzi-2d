use std::{fs, path::Path};
use zipzi_contract::kernel_tasks::ExportFormat;
use zipzi_core::Parameter;
use zipzi_runtime::*;
use zipzi_sdk::{AuthoringSession, HistoryLimits, ProjectIo, Publication};

fn id(n: u32) -> String {
    format!("79000000-0000-4000-8000-{n:012x}")
}

fn create(rt: &mut Runtime, client: ClientId, n: u32) -> Version {
    let session = rt
        .create_session(client, &id(n), Default::default())
        .unwrap();
    rt.subscribe(client, session).unwrap().cursor
}

fn setup() -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let version = create(&mut rt, client, 1);
    (rt, client, version)
}

fn add_parameter(rt: &mut Runtime, client: ClientId, version: Version, n: u32) -> Publication {
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, ()) = rt
        .batch(client, draft, |commands| {
            commands.create_parameter(Parameter {
                id: id(n),
                name: format!("Param{n}"),
                ..Default::default()
            })
        })
        .unwrap();
    rt.publish(client, draft, "Add parameter").unwrap()
}

fn finish(rt: &mut Runtime, client: ClientId, job: JobId) -> Result<JobValue> {
    let work = rt.take_work(job).unwrap();
    rt.complete(work.run()).unwrap();
    rt.job(client, job).unwrap().result.clone().unwrap()
}

fn save(rt: &mut Runtime, client: ClientId, version: Version, io: &ProjectIo, path: &Path) {
    let job = rt
        .request_save_as(client, version, io, path, None, Default::default())
        .unwrap();
    assert!(
        matches!(finish(rt, client, job).unwrap(), JobValue::Saved(receipt) if receipt.published)
    );
}

#[test]
fn two_clients_share_history_and_noop_cancel_and_failure_preserve_redo() {
    let (mut rt, a, base) = setup();
    let b = rt.register_client().unwrap();
    rt.attach(b, base.session).unwrap();
    let edit = add_parameter(&mut rt, a, base, 2);
    let undone = rt.undo(b, edit.version).unwrap();
    let draft = rt.begin_draft(a, undone.version).unwrap();
    assert_eq!(
        rt.publish(a, draft, "No change").unwrap().version,
        undone.version
    );
    let draft = rt.begin_draft(a, undone.version).unwrap();
    rt.cancel_draft(a, draft.draft).unwrap();
    let draft = rt.begin_draft(b, undone.version).unwrap();
    assert!(rt
        .batch(b, draft, |commands| {
            commands.create_parameter(Parameter {
                id: "invalid".into(),
                ..Default::default()
            })
        })
        .is_err());
    let redone = rt.redo(a, undone.version).unwrap();
    assert_eq!(redone.content, edit.content);
    assert!(rt
        .read_snapshot(b, base.session)
        .unwrap()
        .snapshot
        .document()
        .parameter(&id(2))
        .is_some());
    let events = rt.publication_events_since(b, base).unwrap();
    assert_eq!(
        events.iter().map(|event| event.client).collect::<Vec<_>>(),
        [a, b, a]
    );
}

#[test]
fn closing_requires_exclusive_membership_and_no_outstanding_jobs() {
    let (mut rt, a, base) = setup();
    let b = rt.register_client().unwrap();
    rt.attach(b, base.session).unwrap();
    let draft = rt.begin_draft(b, base).unwrap();
    assert_eq!(
        rt.close_session(a, base.session, true).unwrap_err().code,
        "SESSION_IN_USE"
    );
    rt.release_client(b);
    assert!(rt.read_draft(b, draft).is_err());
    let job = rt
        .request_read(
            a,
            QuerySource::Published(base),
            Default::default(),
            |_, _| Ok(Vec::new()),
        )
        .unwrap();
    assert_eq!(
        rt.close_session(a, base.session, true).unwrap_err().code,
        "SESSION_BUSY"
    );
    finish(&mut rt, a, job).unwrap();
    assert_eq!(
        rt.close_session(a, base.session, false).unwrap_err().code,
        "UNSAVED_CHANGES"
    );
    let draft = rt.begin_draft(a, base).unwrap();
    rt.close_session(a, base.session, true).unwrap();
    assert_eq!(rt.read_draft(a, draft).unwrap_err().code, "DRAFT_ENDED");
    assert!(rt.session_catalog(a).unwrap().is_empty());
}

#[test]
fn draft_slots_are_shared_across_owners_and_reclaimed_on_disconnect() {
    let mut rt = Runtime::new(RuntimeLimits {
        drafts: 2,
        ..Default::default()
    });
    let a = rt.register_client().unwrap();
    let b = rt.register_client().unwrap();
    let base = create(&mut rt, a, 1);
    rt.attach(b, base.session).unwrap();
    let first = rt.begin_draft(a, base).unwrap();
    let second = rt.begin_draft(a, base).unwrap();
    assert_ne!(first.draft, second.draft);
    assert_eq!(rt.begin_draft(b, base).unwrap_err().code, "RESOURCE_LIMIT");
    assert_eq!(
        rt.cancel_draft(b, first.draft).unwrap_err().code,
        "NOT_OWNER"
    );
    rt.release_client(a);
    for _ in 0..64 {
        let draft = rt.begin_draft(b, base).unwrap();
        rt.cancel_draft(b, draft.draft).unwrap();
    }
    assert_eq!(rt.task_bytes(), 0);
    assert_eq!(rt.subscribe(b, base.session).unwrap().cursor, base);
}

#[test]
fn normalized_project_aliases_resolve_to_one_session_and_copies_remain_distinct() {
    let (mut rt, a, base) = setup();
    let b = rt.register_client().unwrap();
    let other = create(&mut rt, b, 2);
    let io = rt.project_io().clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.zipzi");
    save(&mut rt, a, base, &io, &path);
    let child = root.path().join("child");
    fs::create_dir(&child).unwrap();
    let alias = child.join("..").join("source.zipzi");
    assert_eq!(rt.project_session(b, &alias).unwrap(), Some(base.session));
    assert_eq!(
        rt.request_open(b, other, &io, &alias, Default::default())
            .unwrap_err()
            .code,
        "PROJECT_BUSY"
    );

    let copy = root.path().join("copy.zipzi");
    fs::copy(&path, &copy).unwrap();
    let job = rt
        .request_open(b, other, &io, &copy, Default::default())
        .unwrap();
    let JobValue::Opened(opened) = finish(&mut rt, b, job).unwrap() else {
        panic!("expected project open");
    };
    assert_eq!(opened.publication.version.session, other.session);
    assert_eq!(rt.project_session(b, &copy).unwrap(), Some(other.session));
    let catalog = rt.session_catalog(b).unwrap();
    assert_eq!(catalog.len(), 2);
    assert_eq!(catalog[0].document_id, catalog[1].document_id);
}

#[test]
fn supported_exports_preserve_the_resident_archive_and_its_aliases() {
    let (mut rt, a, base) = setup();
    let io = rt.project_io().clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.zipzi");
    save(&mut rt, a, base, &io, &path);
    let bytes = fs::read(&path).unwrap();
    let child = root.path().join("child");
    fs::create_dir(&child).unwrap();
    for format in [ExportFormat::Runtime, ExportFormat::Cubism] {
        for dest in [&path, &child.join("..").join("source.zipzi")] {
            assert_eq!(
                rt.request_export(a, QuerySource::Published(base), dest, format, false)
                    .unwrap_err()
                    .code,
                "PROJECT_BUSY"
            );
        }
    }
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(rt.subscribe(a, base.session).unwrap().cursor, base);
    assert!(!rt.session_read(a, base.session).unwrap().is_dirty());
}

#[test]
fn failed_open_releases_the_destination_for_a_different_session() {
    let (mut rt, a, base) = setup();
    let b = rt.register_client().unwrap();
    let other = create(&mut rt, b, 2);
    let io = rt.project_io().clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("pending.zipzi");
    let job = rt
        .request_open(a, base, &io, &path, Default::default())
        .unwrap();
    let work = rt.take_work(job).unwrap();
    assert_eq!(
        rt.request_save_as(b, other, &io, &path, None, Default::default())
            .unwrap_err()
            .code,
        "PROJECT_BUSY"
    );
    let completion = work.run();
    assert_eq!(
        rt.request_save_as(b, other, &io, &path, None, Default::default())
            .unwrap_err()
            .code,
        "PROJECT_BUSY"
    );
    rt.complete(completion).unwrap();
    assert_eq!(rt.job(a, job).unwrap().state, JobState::Failed);
    assert_eq!(rt.subscribe(a, base.session).unwrap().cursor, base);
    save(&mut rt, b, other, &io, &path);
}

#[test]
fn default_budget_admits_large_decode_alongside_other_reservations() {
    let (mut rt, client, version) = setup();
    let source = QuerySource::Published(version);
    // Admission only: do not allocate GiBs in CI. These reservations model a
    // ~1.2 GiB native decoder alongside resident resources/other work.
    let mut jobs = Vec::new();
    for bytes in [384 << 20, 1200 << 20] {
        jobs.push(
            rt.request_read(
                client,
                source,
                TaskBudget {
                    workspace_bytes: bytes,
                    result_bytes: 0,
                },
                |_, _| Ok(Vec::new()),
            )
            .unwrap(),
        );
    }
    let reserved = rt.task_bytes();
    let limit = rt.limits().task_bytes;
    assert!(rt
        .request_read(
            client,
            source,
            TaskBudget {
                workspace_bytes: limit,
                result_bytes: 0
            },
            |_, _| Ok(Vec::new()),
        )
        .is_err());
    assert_eq!(
        rt.task_bytes(),
        reserved,
        "refusal must release partial reservations"
    );
    for job in jobs {
        finish(&mut rt, client, job).unwrap();
        rt.release_job(client, job).unwrap();
    }
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn native_open_checks_expanded_archive_budget_before_replacing_content() {
    let mut rt = Runtime::new(RuntimeLimits {
        task_bytes: 96 << 20,
        ..Default::default()
    });
    let a = rt.register_client().unwrap();
    let base = create(&mut rt, a, 1);
    let draft = rt.begin_draft(a, base).unwrap();
    let (draft, ()) = rt
        .batch(a, draft, |commands| {
            commands.create_empty_asset(id(2), "a".repeat(1 << 20))
        })
        .unwrap();
    let before = rt.publish(a, draft, "Large metadata").unwrap();
    let io = rt.project_io().clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("compressed.zipzi");
    save(&mut rt, a, before.version, &io, &path);
    assert!(fs::metadata(&path).unwrap().len() < 10 << 10);
    let edited = add_parameter(&mut rt, a, before.version, 3);
    // Leave room for archive admission, but too little for expanded JSON.
    let pressure_bytes = 92 << 20;
    let pressure = rt
        .request_read(
            a,
            QuerySource::Published(edited.version),
            TaskBudget {
                workspace_bytes: pressure_bytes,
                result_bytes: 0,
            },
            |_, _| Ok(Vec::new()),
        )
        .unwrap();
    let job = rt
        .request_open(
            a,
            edited.version,
            &io,
            &path,
            TaskBudget {
                workspace_bytes: 128 << 10,
                result_bytes: 0,
            },
        )
        .unwrap();
    assert_eq!(finish(&mut rt, a, job).unwrap_err().code, "BUDGET_EXCEEDED");
    assert_eq!(
        rt.subscribe(a, base.session).unwrap().cursor,
        edited.version
    );
    assert_eq!(rt.task_bytes(), pressure_bytes);
    finish(&mut rt, a, pressure).unwrap();
    let job = rt
        .request_open(
            a,
            edited.version,
            &io,
            &path,
            TaskBudget {
                workspace_bytes: 32 << 20,
                result_bytes: 0,
            },
        )
        .unwrap();
    assert!(matches!(
        finish(&mut rt, a, job).unwrap(),
        JobValue::Opened(_)
    ));
    let session = rt.session_read(a, base.session).unwrap();
    assert!(session.snapshot().document().parameter(&id(3)).is_none());
    assert_eq!(session.history_state().undo_steps, 0);
    assert_eq!(rt.task_bytes(), 0);
}

#[test]
fn confirmed_replacement_respects_path_ownership_and_keeps_history() {
    let (mut rt, a, base) = setup();
    let b = rt.register_client().unwrap();
    let owner = create(&mut rt, b, 2);
    let edited = add_parameter(&mut rt, a, base, 3);
    let io = rt.project_io().clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("replace.zipzi");
    save(&mut rt, b, owner, &io, &path);
    let original = fs::read(&path).unwrap();
    let fingerprint = io.fingerprint(&path, || true).unwrap();
    assert_eq!(
        rt.request_save_as(
            a,
            edited.version,
            &io,
            &path,
            Some(&fingerprint),
            Default::default()
        )
        .unwrap_err()
        .code,
        "PROJECT_BUSY"
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    rt.close_session(b, owner.session, false).unwrap();
    let other = create(&mut rt, b, 4);

    fs::write(&path, b"externally replaced").unwrap();
    let refusal = rt.request_save_as(
        a,
        edited.version,
        &io,
        &path,
        Some(&fingerprint),
        Default::default(),
    );
    let error = match refusal {
        Ok(job) => finish(&mut rt, a, job).unwrap_err(),
        Err(error) => error,
    };
    assert_eq!(error.code, "PROJECT_CONFLICT");
    let session = rt.session_read(a, base.session).unwrap();
    assert_eq!(session.version(), edited.version);
    assert_eq!(session.history_state().undo_steps, 1);
    assert!(session.project().is_none());
    assert!(session.is_dirty());
    assert_eq!(fs::read(&path).unwrap(), b"externally replaced");

    let confirmed = io.fingerprint(&path, || true).unwrap();
    let job = rt
        .request_save_as(
            a,
            edited.version,
            &io,
            &path,
            Some(&confirmed),
            Default::default(),
        )
        .unwrap();
    assert_eq!(
        rt.request_open(b, other, &io, &path, Default::default())
            .unwrap_err()
            .code,
        "PROJECT_BUSY"
    );
    assert!(
        matches!(finish(&mut rt, a, job).unwrap(), JobValue::Saved(receipt) if receipt.published)
    );
    assert_eq!(rt.project_session(b, &path).unwrap(), Some(base.session));
    let session = rt.session_read(a, base.session).unwrap();
    assert_eq!(session.version(), edited.version);
    assert_eq!(session.history_state().undo_steps, 1);
    assert!(!session.is_dirty());
    rt.undo(a, edited.version).unwrap();
    assert!(rt.session_read(a, base.session).unwrap().is_dirty());
}

#[test]
fn zero_history_limit_still_allows_a_complete_publication() {
    let (mut rt, a, base) = setup();
    let mut session = AuthoringSession::new(rt.read_snapshot(a, base.session).unwrap().snapshot);
    session.set_history_limits(HistoryLimits {
        max_steps: 0,
        target_bytes: 0,
    });
    let key = rt.adopt_session(a, session).unwrap();
    let version = rt.subscribe(a, key).unwrap().cursor;
    let edited = add_parameter(&mut rt, a, version, 2);
    let session = rt.session_read(a, key).unwrap();
    assert_eq!(session.version(), edited.version);
    assert_eq!(session.history_state().undo_steps, 0);
    assert!(session.snapshot().document().parameter(&id(2)).is_some());
}
