use std::sync::Arc;
use zipzi_core::{state::DocumentState, Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, QuerySource, ResourceStore, ResourceView, Snapshot};
fn sdk() -> AuthoringSession {
    AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(
                "61000000-0000-4000-8000-000000000001",
                Canvas::new(100., 100., Vec2::default(), 1.).into(),
            )
            .unwrap(),
            ResourceView::default(),
        )
        .unwrap(),
    )
}
#[test]
fn candidate_snapshot_keeps_new_image_and_source_after_draft_and_session_drop() {
    let session = sdk();
    let asset_id = "61000000-0000-4000-8000-000000000002";
    let rgba: Arc<[u8]> = Arc::from([255, 0, 0, 255]);
    let image = ResourceStore::default()
        .prepare_rgba(1, 1, rgba.clone())
        .unwrap();
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_empty_asset(asset_id.into(), "draft".into())?;
            c.replace_image(asset_id, image)
        })
        .unwrap();
    let snapshot = draft.read_snapshot().unwrap();
    assert_eq!(snapshot.source, QuerySource::Draft(draft.source()));
    assert!(session.snapshot().document().asset(asset_id).is_none());
    let captured = snapshot
        .snapshot
        .resources()
        .read_asset(snapshot.snapshot.document().asset(asset_id).unwrap())
        .unwrap();
    assert_eq!(captured.rgba.as_slice(), rgba.as_ref());
    drop(draft);
    drop(session);
    assert_eq!(
        snapshot
            .snapshot
            .resources()
            .read_asset(snapshot.snapshot.document().asset(asset_id).unwrap())
            .unwrap()
            .rgba,
        captured.rgba
    );
    assert!(zipzi_core::evaluate_frame(
        snapshot.snapshot.document(),
        &Default::default(),
        &mut Default::default()
    )
    .is_ok());
}
#[test]
fn stale_and_cross_session_candidates_remain_readable_but_cannot_publish() {
    let mut session = sdk();
    let mut other = sdk();
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_empty_asset(
                "61000000-0000-4000-8000-000000000002".into(),
                "draft".into(),
            )
        })
        .unwrap();
    let captured = draft.read_snapshot().unwrap();
    let old = session.snapshot().clone();
    // Replacing even with the same snapshot advances generation and invalidates old writers.
    session.replace(session.version(), old).unwrap();
    assert_eq!(
        captured
            .snapshot
            .document()
            .setup()
            .source_canvas
            .unwrap()
            .width,
        100.
    );
    assert_eq!(
        session
            .publish(draft.prepare("stale").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let mut foreign = session.begin_draft();
    foreign
        .batch(foreign.source(), |c| {
            c.create_empty_asset(
                "61000000-0000-4000-8000-000000000003".into(),
                "foreign".into(),
            )
        })
        .unwrap();
    assert_eq!(
        other
            .publish(foreign.prepare("foreign").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert!(other.snapshot().document().asset_order().is_empty());
    assert!(session.snapshot().document().asset_order().is_empty());
    assert_eq!(captured.snapshot.document().asset_order().len(), 1);
}
