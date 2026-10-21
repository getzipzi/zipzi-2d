use kasane_core::{
    document::{MotionClip, PropertyPatch},
    preview::PreviewState,
    ModelSetup,
};
use kasane_sdk::AuthoringSession;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn fixture(source_range: bool) -> AuthoringSession {
    let mut sdk = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
    let mut clip: MotionClip = serde_json::from_value(serde_json::json!({
        "id": id(2), "name": "Motion", "duration": 1., "fps": 30.,
        "looping": false, "restricted_beziers": true, "fade_in": null, "fade_out": null,
        "tracks": [{"id": id(3), "target": {"kind": "model", "runtime_id": "Opacity"},
            "initial": {"id": id(4), "time": 0., "value": 0.},
            "segments": [{"kind": "linear", "end": {"id": id(5), "time": 1.01, "value": 1.}}],
            "fade_in": null, "fade_out": null, "extensions": {}}],
        "events": [{"id": id(6), "time": 0.75, "value": "cue", "extensions": {}}],
        "extensions": {}, "meta_extensions": {}, "opaque_source_ids": null
    }))
    .unwrap();
    if source_range {
        clip.meta_extensions.insert(
            "KasaneSourceKeyRange".into(),
            serde_json::json!({"start": -1., "end": 2.}),
        );
        clip.tracks[0].initial.time = -0.5;
    }
    sdk.edit("fixture", None, |edit| edit.create_motion(clip))
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
        for duration in [0., 0.4, 0.8, 1., 1.01, 2., f32::INFINITY] {
            for fps in [0., 1., 30., 120., f32::NAN] {
                let mut scalar = sdk.document().fork_candidate();
                let mut full = sdk.document().fork_candidate();
                let mut clip = full.get_motion(&id(2)).unwrap().clone();
                clip.duration = duration;
                clip.fps = fps;
                clip.fade_in = Some(0.4);
                let expected = full.replace_motion(clip);
                let actual = scalar.patch_property(&patch(duration, fps, Some(0.4)));
                assert_eq!(
                    actual.status.code, expected.status.code,
                    "duration={duration}, fps={fps}, source={source_range}"
                );
                if actual.status.is_ok() {
                    assert!(scalar.same_content(&full));
                } else {
                    assert!(scalar.same_content(sdk.document()));
                }
            }
        }
    }
}

#[test]
fn fade_previews_share_tracks_and_events_preserve_failures_and_commit_once() {
    let mut sdk = fixture(false);
    let before = sdk.read_snapshot();
    let original = before.document().get_motion(&id(2)).unwrap();
    let mut edit = sdk.begin_owned_edit("Fade", None).unwrap();
    let mut preview = PreviewState::default();
    for fade in [0.2, 0.4, 0.8] {
        #[cfg(feature = "bench-instrumentation")]
        let capture = kasane_core::benchmark::Capture::start();
        edit.preview_property(
            &patch(1., 30., Some(fade)),
            &mut preview,
            Default::default(),
        )
        .unwrap();
        #[cfg(feature = "bench-instrumentation")]
        {
            let stages = capture.finish();
            assert!(!stages.contains_key("core.validate_motion"));
            assert!(!stages.contains_key("core.validate_motion_timing"));
        }
        let current = edit.candidate_document().get_motion(&id(2)).unwrap();
        assert_eq!(original.tracks.as_ptr(), current.tracks.as_ptr());
        assert_eq!(original.events.as_ptr(), current.events.as_ptr());
        assert_eq!(current.fade_in, Some(fade));
        assert_eq!(original.fade_in, None);
    }
    let frame = preview.frame(edit.candidate_document(), 0).unwrap();
    let revision = edit.candidate_document().revision();
    for rejected in [
        patch(0.5, 30., None),
        patch(1., 120., None),
        patch(1., 30., Some(-1.)),
    ] {
        assert!(edit
            .preview_property(&rejected, &mut preview, Default::default())
            .is_err());
        assert_eq!(edit.candidate_document().revision(), revision);
        assert_eq!(
            edit.candidate_document()
                .get_motion(&id(2))
                .unwrap()
                .fade_in,
            Some(0.8)
        );
        assert!(std::sync::Arc::ptr_eq(
            &frame,
            &preview.frame(edit.candidate_document(), 0).unwrap()
        ));
    }
    edit.preview_property(&patch(1., 30., Some(0.6)), &mut preview, Default::default())
        .unwrap();
    let receipt = edit.commit_to(&mut sdk).unwrap();
    assert_eq!(receipt.after.revision, receipt.before.revision + 1);
    assert_eq!(
        sdk.document().get_motion(&id(2)).unwrap().fade_in,
        Some(0.6)
    );
    sdk.undo().unwrap();
    assert_eq!(sdk.document().get_motion(&id(2)).unwrap(), original);
    sdk.redo().unwrap();
    assert_eq!(
        sdk.document().get_motion(&id(2)).unwrap().fade_in,
        Some(0.6)
    );
    let encoded = kasane_project::encode_project(sdk.document()).unwrap();
    let restored = kasane_project::decode_project(&encoded).unwrap();
    assert!(restored.same_content(sdk.document()));
}
