use zipzi_core::{state::DocumentState, ModelSetup};
use zipzi_sdk::{AuthoringSession, Snapshot};
#[test]
fn image_replacement_reads_current_candidate_and_retains_captured_version() {
    let mut s = AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(
                "79500000-0000-4000-8000-000000000001",
                ModelSetup::default().into(),
            )
            .unwrap(),
            Default::default(),
        )
        .unwrap(),
    );
    let store = zipzi_project::ResourceStore::default();
    let a = store
        .prepare_rgba(1, 1, std::sync::Arc::from([12, 34, 56, 255]))
        .unwrap();
    let b = store
        .prepare_rgba(1, 1, std::sync::Arc::from([11, 22, 33, 255]))
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_empty_asset(id.clone(), "Texture".into())?;
            c.replace_image(&id, a)
        })
        .unwrap();
    let old = draft.read_snapshot().unwrap();
    draft
        .batch(draft.source(), |c| c.replace_image(&id, b))
        .unwrap();
    let new = draft.read_snapshot().unwrap();
    assert_eq!(
        new.snapshot
            .resources()
            .read_texture(new.snapshot.document().asset(&id).unwrap())
            .unwrap()
            .rgba
            .as_ref(),
        &[11, 22, 33, 255]
    );
    assert_eq!(
        old.snapshot
            .resources()
            .read_texture(old.snapshot.document().asset(&id).unwrap())
            .unwrap()
            .rgba
            .as_ref(),
        &[12, 34, 56, 255]
    );
    s.publish(draft.prepare("image").unwrap()).unwrap();
    assert_eq!(s.snapshot().document().asset(&id).unwrap().name, "Texture");
    assert_eq!(s.history_state().undo_steps, 1);
}
