use std::sync::Arc;
use zipzi_core::{Canvas, PreviewValues, Vec2};
use zipzi_sdk::AuthoringSession;

fn sdk() -> AuthoringSession {
    AuthoringSession::new(
        "61000000-0000-4000-8000-000000000001",
        Canvas::new(100., 100., Vec2::default(), 1.),
    )
    .unwrap()
}
#[test]
fn candidate_snapshot_keeps_new_memory_image_and_source_after_candidate_dropped() {
    let session = sdk();
    let asset_id = "61000000-0000-4000-8000-000000000002";
    let rgba: Arc<[u8]> = Arc::from([255, 0, 0, 255]);
    let asset = session
        .prepare_rgba_asset(asset_id, "draft", 1, 1, rgba.clone())
        .unwrap();
    let mut edit = session.begin_owned_edit("memory image", None).unwrap();
    edit.create_asset(asset).unwrap();
    let snapshot = session.candidate_snapshot(&edit).unwrap();
    assert_eq!(snapshot.version(), edit.base_version());
    assert!(session.read_snapshot().memory_image(asset_id).is_none());
    assert!(snapshot.document().get_asset(asset_id).is_some());
    let image = snapshot.memory_image(asset_id).unwrap();
    assert_eq!(image.read_asset().unwrap().rgba.as_slice(), rgba.as_ref());
    drop(edit);
    drop(session);
    assert_eq!(
        snapshot
            .memory_image(asset_id)
            .unwrap()
            .read_asset()
            .unwrap()
            .rgba
            .as_slice(),
        rgba.as_ref()
    );
    snapshot.evaluate(&PreviewValues::default()).unwrap();
}
#[test]
fn stale_revision_readable_but_cross_session_or_generation_snapshot_rejected() {
    let mut session = sdk();
    let other = sdk();
    let edit = session.begin_owned_edit("old", None).unwrap();
    session
        .edit("resize", None, |e| {
            e.replace_model_setup(Canvas::new(200., 100., Vec2::default(), 1.))
        })
        .unwrap();
    assert_eq!(
        session
            .candidate_snapshot(&edit)
            .unwrap()
            .document()
            .source_canvas()
            .unwrap()
            .width,
        100.
    );
    assert_eq!(
        other.candidate_snapshot(&edit).unwrap_err().code.as_ref(),
        "STALE_VERSION"
    );
    session
        .new_project(
            "61000000-0000-4000-8000-000000000003",
            Canvas::new(100., 100., Vec2::default(), 1.),
            None,
        )
        .unwrap();
    assert_eq!(
        session.candidate_snapshot(&edit).unwrap_err().code.as_ref(),
        "STALE_VERSION"
    );
}
