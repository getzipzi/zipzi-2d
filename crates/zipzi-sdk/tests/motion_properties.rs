use zipzi_core::{document::MotionClip, preview::PreviewState, state::PropertyPatch, ModelSetup};
use zipzi_sdk::AuthoringSession;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn fixture(source_range: bool) -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(&id(1), (ModelSetup::default()).into()).unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let mut clip: MotionClip = serde_json::from_value(serde_json::json!({
        "id": id(2), "name": "Motion", "duration": 1., "fps": 30.,
        "looping": false, "restricted_beziers": true, "fade_in": null, "fade_out": null,
        "track_order": [], "tracks": [{"id": id(3), "target": {"kind": "model", "runtime_id": "Opacity"},
            "initial": {"id": id(4), "time": 0., "value": 0.},
            "segments": [{"kind": "linear", "end": {"id": id(5), "time": 1.01, "value": 1.}}],
            "fade_in": null, "fade_out": null, "extensions": {}}],
        "events": [{"id": id(6), "time": 0.75, "value": "cue", "extensions": {}}],
        "extensions": {}, "meta_extensions": {}, "opaque_source_ids": null
    }))
    .unwrap();
    if source_range {
        clip.meta_extensions.insert(
            "ZipziSourceKeyRange".into(),
            serde_json::json!({"start": -1., "end": 2.}),
        );
        clip.tracks[0].initial.time = -0.5;
    }
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.create_motion(clip))
            .and_then(|value| {
                let prepared = draft.prepare("fixture")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    sdk
}

fn patch(duration: f32, fps: f32, fade_in: Option<f32>) -> PropertyPatch {
    PropertyPatch::Motion {
        id: id(2),
        duration,
        fps,
        fade_in,
        fade_out: None,
    }
}

#[test]
fn scalar_validation_matches_full_motion_validation_at_time_boundaries() {
    for source_range in [false, true] {
        let sdk = fixture(source_range);
        let state = sdk.snapshot().document();
        let identity = state.content_id();
        for duration in [0., 0.4, 0.8, 1., 1.01, 2., f32::INFINITY] {
            for fps in [0., 1., 30., 120., f32::NAN] {
                let mut scalar = state.builder();
                let mut full = state.builder();
                let mut clip = state.motion(&id(2)).unwrap().clone();
                clip.duration = duration;
                clip.fps = fps;
                clip.fade_in = Some(0.4);
                let expected = full.replace_motion(clip).and_then(|_| full.freeze());
                let actual = scalar
                    .patch_property(patch(duration, fps, Some(0.4)))
                    .and_then(|_| scalar.freeze());
                assert_eq!(
                    actual.is_ok(),
                    expected.is_ok(),
                    "duration={duration}, fps={fps}, source={source_range}"
                );
                match (actual, expected) {
                    (Ok(actual), Ok(expected)) => {
                        assert_eq!(actual.state.motion(&id(2)), expected.state.motion(&id(2)))
                    }
                    (Err(actual), Err(expected)) => assert_eq!(actual.code, expected.code),
                    _ => unreachable!(),
                }
                assert_eq!(state.content_id(), identity);
            }
        }
    }
}

#[test]
fn fade_previews_share_tracks_and_events_preserve_failures_and_publish_once() {
    let mut sdk = fixture(false);
    let before = sdk.read_snapshot();
    let original = before.snapshot.document().motion(&id(2)).unwrap();
    let mut draft = sdk.begin_draft();
    let mut preview = PreviewState::default();
    for fade in [0.2, 0.4, 0.8] {
        let (prepared, ()) = draft
            .prepare_preview(draft.source(), |c| {
                c.patch_property(patch(1., 30., Some(fade)))
            })
            .unwrap();
        draft.install_preview(prepared).unwrap();
        let current = draft.document().unwrap().motion(&id(2)).unwrap();
        assert_eq!(original.tracks.as_ptr(), current.tracks.as_ptr());
        assert_eq!(original.events.as_ptr(), current.events.as_ptr());
        assert_eq!(current.fade_in, Some(fade));
        assert_eq!(original.fade_in, None);
    }
    let snapshot = draft.read_snapshot().unwrap();
    let frame = preview.frame(snapshot.snapshot.document(), 0).unwrap();
    let source = draft.source();
    for rejected in [
        patch(0.5, 30., None),
        patch(1., 120., None),
        patch(1., 30., Some(-1.)),
    ] {
        assert!(draft
            .prepare_preview(draft.source(), |c| c.patch_property(rejected))
            .is_err());
        assert_eq!(draft.source(), source);
        let current = draft.read_snapshot().unwrap();
        assert_eq!(
            current.snapshot.content_id(),
            snapshot.snapshot.content_id()
        );
        assert_eq!(
            current.snapshot.document().motion(&id(2)).unwrap().fade_in,
            Some(0.8)
        );
        assert!(std::sync::Arc::ptr_eq(
            &frame,
            &preview.frame(current.snapshot.document(), 0).unwrap()
        ));
    }
    let (prepared, ()) = draft
        .prepare_preview(draft.source(), |c| {
            c.patch_property(patch(1., 30., Some(0.6)))
        })
        .unwrap();
    draft.install_preview(prepared).unwrap();
    let version = sdk.version();
    let receipt = sdk.publish(draft.prepare("Fade").unwrap()).unwrap();
    assert_eq!(receipt.version.revision, version.revision + 1);
    assert_eq!(
        sdk.snapshot().document().motion(&id(2)).unwrap().fade_in,
        Some(0.6)
    );
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(sdk.snapshot().document().motion(&id(2)).unwrap(), original);
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot().document().motion(&id(2)).unwrap().fade_in,
        Some(0.6)
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("motion.zipzi");
    let io = zipzi_sdk::ProjectIo::new(root.path(), zipzi_sdk::ResourceStore::default());
    io.save(
        sdk.snapshot().document(),
        sdk.snapshot().resources(),
        &path,
        None,
        || true,
    )
    .unwrap();
    let loaded = io.load(&path, || true).unwrap();
    assert_eq!(
        loaded.document.motion(&id(2)),
        sdk.snapshot().document().motion(&id(2))
    );
}
