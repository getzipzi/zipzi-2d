use super::*;
use zipzi_core::{state::DocumentState, Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, NameTarget, ResourceStore, Snapshot};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn fixture() -> AuthoringSession {
    let image = ResourceStore::default()
        .prepare_rgba(2, 2, vec![255; 16].into())
        .unwrap();
    let state =
        DocumentState::new(id(1), Canvas::new(100., 100., Vec2::default(), 10.).into()).unwrap();
    let mut session = AuthoringSession::new(Snapshot::new(state, Default::default()).unwrap());
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_empty_asset(id(2), "image".into())?;
            c.replace_image(&id(2), image)?;
            c.create_mesh(zipzi_sdk::rectangle_mesh(
                &id(3),
                "mesh",
                &id(2),
                Vec2::new(0., 0.),
                Vec2::new(10., 10.),
            )?)
        })
        .unwrap();
    session.publish(draft.prepare("fixture").unwrap()).unwrap();
    session
}
fn input(s: &SourcedSnapshot) -> ObservationInput {
    ObservationInput::capture_from_snapshot(s, &Default::default()).unwrap()
}
fn resolve(i: ObservationInput) -> ResolvedObservation {
    ResolvedObservation::capture_many_with_control(vec![i], &Default::default())
        .unwrap()
        .remove(0)
}

#[test]
fn undo_restores_content_but_cannot_revalidate_old_animation_source_or_mix_batches() {
    let mut session = fixture();
    let before = session.read_snapshot();
    let mut preview = MotionPreview::new(before.snapshot.document());
    preview.bind_source(before.source);
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.rename(NameTarget::Mesh(&id(3)), "changed".into())
        })
        .unwrap();
    session.publish(draft.prepare("rename").unwrap()).unwrap();
    session.undo(session.version()).unwrap();
    let after = session.read_snapshot();
    assert_eq!(before.snapshot.content_id(), after.snapshot.content_id());
    assert_ne!(before.source, after.source);
    assert_eq!(
        ObservationInput::capture_motion_from_snapshot(&after, &preview)
            .unwrap_err()
            .code,
        "STALE_ANIMATION_PREVIEW"
    );
    assert_eq!(
        ResolvedObservation::capture_many_with_control(
            vec![input(&before), input(&after)],
            &Default::default()
        )
        .unwrap_err()
        .code,
        "MIXED_DOCUMENT_SNAPSHOTS"
    );
    // Old readers still work against their exact captured source.
    ObservationInput::capture_motion_from_snapshot(&before, &preview).unwrap();
}

#[test]
fn resource_versions_survive_session_close_and_same_pixels_have_stable_digests() {
    let mut session = fixture();
    let before = session.read_snapshot();
    let original = input(&before);
    let same_pixels = ResourceStore::default()
        .prepare_rgba(2, 2, vec![255; 16].into())
        .unwrap();
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| c.replace_image(&id(2), same_pixels))
        .unwrap();
    session
        .publish(draft.prepare("same pixels, new resource").unwrap())
        .unwrap();
    let same = input(&session.read_snapshot());
    assert_ne!(original.content_id, same.content_id);
    let changed_pixels = ResourceStore::default()
        .prepare_rgba(2, 2, vec![0; 16].into())
        .unwrap();
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| c.replace_image(&id(2), changed_pixels))
        .unwrap();
    session
        .publish(draft.prepare("new pixels").unwrap())
        .unwrap();
    let changed = input(&session.read_snapshot());
    drop(session);
    drop(before);
    let original = resolve(original);
    let same = resolve(same);
    let changed = resolve(changed);
    assert_eq!(original.scene_digest(), same.scene_digest());
    assert_ne!(original.scene_digest(), changed.scene_digest());
    assert_eq!(&*original.textures[0].data.rgba, &[255; 16]);
    assert_eq!(&*changed.textures[0].data.rgba, &[0; 16]);
}

#[test]
fn draft_queries_remain_distinct_even_before_any_write() {
    let session = fixture();
    let left = session.begin_draft();
    let right = session.begin_draft();
    let a = input(&left.read_snapshot().unwrap());
    let b = input(&right.read_snapshot().unwrap());
    assert_eq!(a.content_id, b.content_id);
    assert_eq!(
        ResolvedObservation::capture_many_with_control(vec![a, b], &Default::default())
            .unwrap_err()
            .code,
        "MIXED_DOCUMENT_SNAPSHOTS"
    );
}
