//! Stable IDs are model addresses; write validity comes from the complete Draft source.
use zipzi_core::{state::DocumentState, Mesh, ModelSetup};
use zipzi_sdk::{AuthoringSession, NameTarget, ResourceView, Snapshot};
const MESH: &str = "00000000-0000-4000-8000-000000000502";
const MISSING: &str = "00000000-0000-4000-8000-000000000599";
fn session() -> AuthoringSession {
    let mut b = DocumentState::new(
        "00000000-0000-4000-8000-000000000501",
        ModelSetup::default(),
    )
    .unwrap()
    .builder();
    b.create_mesh(Mesh {
        id: MESH.into(),
        runtime_id: "mesh".into(),
        name: "mesh".into(),
        ..Default::default()
    })
    .unwrap();
    AuthoringSession::new(
        Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
    )
}
#[test]
fn retained_snapshots_read_old_records_but_foreign_and_undo_sources_cannot_write() {
    let mut s = session();
    let mut foreign = session();
    let old = s.read_snapshot();
    let mut d = s.begin_draft();
    d.batch(d.source(), |c| {
        c.rename(NameTarget::Mesh(MESH), "pending".into())
    })
    .unwrap();
    assert_eq!(
        foreign
            .publish(d.prepare("foreign").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let stale = s.begin_draft().prepare("old").unwrap();
    let mut d = s.begin_draft();
    d.batch(d.source(), |c| {
        c.rename(NameTarget::Mesh(MESH), "new".into())
    })
    .unwrap();
    s.publish(d.prepare("rename").unwrap()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
    assert_eq!(old.snapshot.document().mesh(MESH).unwrap().name, "mesh");
    assert_ne!(old.source, s.read_snapshot().source);
}
#[test]
fn missing_record_errors_identify_the_object_and_abort_the_whole_batch() {
    for owned in [false, true] {
        let mut s = session();
        let version = s.version();
        let history = s.history_state();
        s.take_events();
        let mut d = s.begin_draft();
        let error = d
            .batch(d.source(), |c| {
                c.rename(NameTarget::Mesh(MESH), "candidate only".into())?;
                if owned {
                    let mut mesh = c.read_document()?.mesh(MESH).unwrap().clone();
                    mesh.id = MISSING.into();
                    c.replace_mesh(mesh)
                } else {
                    c.rename(NameTarget::Mesh(MISSING), "missing".into())
                }
            })
            .unwrap_err();
        assert_eq!(error.code, "MISSING_OBJECT");
        assert!(error.message.contains(MISSING));
        assert!(!d.is_active());
        assert_eq!(d.prepare("failed").unwrap_err().code, "DRAFT_ENDED");
        assert_eq!(s.version(), version);
        assert_eq!(s.history_state().undo_steps, history.undo_steps);
        assert!(s.take_events().is_empty());
        assert_eq!(s.snapshot().document().mesh(MESH).unwrap().name, "mesh");
    }
}
