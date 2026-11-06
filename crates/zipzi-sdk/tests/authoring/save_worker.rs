//! The split save path must retain the synchronous SDK's publication guarantees.
use zipzi_core::{state::DocumentState, ModelSetup, Parameter};
use zipzi_sdk::{AuthoringSession, PreparedSave, ProjectIo, ResourceStore, ResourceView, Snapshot};
fn session() -> AuthoringSession {
    AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(
                "00000000-0000-4000-8000-000000000001",
                ModelSetup::default(),
            )
            .unwrap(),
            ResourceView::default(),
        )
        .unwrap(),
    )
}
fn change(s: &AuthoringSession) -> zipzi_sdk::PreparedChange {
    let mut d = s.begin_draft();
    d.batch(d.source(), |c| {
        c.create_parameter(Parameter {
            id: "00000000-0000-4000-8000-000000000002".into(),
            ..Default::default()
        })
    })
    .unwrap();
    d.prepare("add").unwrap()
}
fn code<T>(r: Result<T, zipzi_sdk::Status>) -> String {
    match r {
        Ok(_) => panic!("expected error"),
        Err(e) => e.code,
    }
}
#[test]
fn worker_save_blocks_publication_but_retains_read_access_until_metadata_install() {
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let mut s = session();
    let v = s.version();
    let snapshot = s.snapshot().clone();
    let content = snapshot.content_id();
    let task = s
        .prepare_save_as(&io, &root.path().join("model.zipzi"), None)
        .unwrap();
    assert_eq!(task.source(), v);
    assert_eq!(s.read_snapshot().snapshot.content_id(), content);
    assert_eq!(code(s.publish(change(&s))), "PROJECT_BUSY");
    assert_eq!(code(s.undo(v)), "PROJECT_BUSY");
    assert_eq!(code(s.redo(v)), "PROJECT_BUSY");
    assert_eq!(code(s.replace(v, snapshot)), "PROJECT_BUSY");
    assert_eq!(
        code(s.prepare_save_as(&io, &root.path().join("other.zipzi"), None)),
        "PROJECT_BUSY"
    );
    let completed = std::thread::spawn(move || task.write(|| true))
        .join()
        .unwrap()
        .unwrap();
    assert!(completed.receipt().published);
    assert_eq!(code(s.publish(change(&s))), "PROJECT_BUSY");
    assert!(s.is_dirty());
    let mut wrong = session();
    assert_eq!(code(wrong.complete_save(&completed)), "STALE_SOURCE");
    assert!(wrong.project().is_none());
    s.complete_save(&completed).unwrap();
    assert!(!s.is_dirty());
    assert_eq!(s.saved_marker().unwrap().content, content);
    assert_eq!(s.version(), v);
    assert_eq!(code(s.complete_save(&completed)), "STALE_SOURCE");
    s.publish(change(&s)).unwrap();
    assert!(s.is_dirty());
    s.undo(s.version()).unwrap();
    assert!(!s.is_dirty());
}
#[test]
fn failed_unentered_and_panicking_workers_release_the_save_permit() {
    fn send<T: Send>() {}
    send::<PreparedSave>();
    send::<zipzi_sdk::CompletedSave>();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let mut s = session();
    let path = root.path().join("model.zipzi");
    drop(s.prepare_save_as(&io, &path, None).unwrap());
    let task = s.prepare_save_as(&io, &path, None).unwrap();
    assert_eq!(code(task.write(|| false)), "CANCELLED");
    assert!(!path.exists());
    let task = s.prepare_save_as(&io, &path, None).unwrap();
    assert!(
        std::thread::spawn(move || task.write(|| panic!("injected decoder panic")))
            .join()
            .is_err()
    );
    assert!(s.project().is_none());
    s.publish(change(&s)).unwrap();
    s.save_as(&io, &path, None, || true).unwrap();
    assert!(!s.is_dirty());
}
#[test]
fn lost_published_completion_cannot_silently_unlock_the_session() {
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let mut s = session();
    let path = root.path().join("model.zipzi");
    let completed = s
        .prepare_save_as(&io, &path, None)
        .unwrap()
        .write(|| true)
        .unwrap();
    drop(completed);
    assert!(path.exists());
    assert!(s.is_dirty());
    assert_eq!(code(s.publish(change(&s))), "PROJECT_BUSY");
}
