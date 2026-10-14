use std::sync::Arc;

use kasane_animation::{sample_motion_curve, CompiledCurve};
use kasane_core::document::{
    MotionClip, MotionEvent, MotionKey, MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
};
use kasane_core::{Canvas, Parameter, Vec2};
use kasane_project::{decode_project, encode_project};
use kasane_sdk::{
    AuthoringSession, MotionBezierHandle, MotionClipCandidate, MotionInterpolation, MotionKeyMove,
    MotionKeyRef, MotionPreview,
};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn key(n: u32, time: f32, value: f32) -> MotionKey {
    MotionKey {
        id: id(n),
        time,
        value,
    }
}

fn session() -> AuthoringSession {
    let mut s =
        AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
    s.edit("Fixture", None, |e| {
        e.create_parameter(Parameter {
            id: id(2),
            runtime_id: "ParamX".into(),
            minimum: -2.,
            maximum: 2.,
            ..Default::default()
        })?;
        e.create_motion(MotionClip {
            id: id(3),
            name: "Motion".into(),
            duration: 1.,
            fps: 30.,
            looping: false,
            restricted_beziers: false,
            fade_in: Some(0.),
            fade_out: Some(0.),
            tracks: (0..2)
                .map(|index| MotionTrack {
                    id: id(10 + index),
                    target: MotionTrackTarget::Parameter {
                        parameter_id: id(2),
                    },
                    initial: key(100 + index * 10, 0., 0.),
                    segments: Arc::new(
                        (1..=4)
                            .map(|i| MotionSegment::Linear {
                                end: key(100 + index * 10 + i, i as f32 * 0.25, (i % 2) as f32),
                            })
                            .collect(),
                    ),
                    fade_in: None,
                    fade_out: None,
                    extensions: Default::default(),
                })
                .collect(),
            events: vec![MotionEvent {
                id: id(20),
                time: 0.5,
                value: "cue".into(),
                extensions: Default::default(),
            }],
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
    })
    .unwrap();
    s
}

fn candidate(s: &AuthoringSession) -> MotionClipCandidate {
    MotionClipCandidate::new(s.document(), s.motion(&id(3)).unwrap()).unwrap()
}

fn reference(track: u32, key: u32) -> MotionKeyRef {
    MotionKeyRef {
        track_id: id(10 + track),
        key_id: id(100 + track * 10 + key),
    }
}

fn movement(track: u32, key: u32, time: f32, value: f32) -> MotionKeyMove {
    MotionKeyMove {
        key: reference(track, key),
        point: MotionPoint { time, value },
    }
}

fn bezier_session(restricted: bool) -> AuthoringSession {
    let mut s = session();
    let mut clip = s.motion(&id(3)).unwrap();
    clip.restricted_beziers = restricted;
    clip.tracks[0].segments = Arc::new(vec![MotionSegment::Bezier {
        control1: MotionPoint {
            time: 0.2,
            value: 1.8,
        },
        control2: MotionPoint {
            time: 0.9,
            value: -1.2,
        },
        end: key(104, 1., 1.),
    }]);
    s.edit("Bezier fixture", None, |e| e.replace_motion(clip))
        .unwrap();
    s
}

fn samples(clip: &MotionClip) -> Vec<f32> {
    let curve = CompiledCurve::from(&clip.tracks[0]);
    (0..=1000)
        .map(|i| sample_motion_curve(&curve, i as f32 / 1000., clip.restricted_beziers))
        .collect()
}

fn assert_samples_equal(a: &[f32], b: &[f32]) {
    for (i, (a, b)) in a.iter().zip(b).enumerate() {
        assert!((a - b).abs() < 0.0003, "at sample {i}: {a} != {b}");
    }
}

#[test]
fn simultaneous_moves_cross_original_positions_and_commit_one_history_step() {
    let mut s = session();
    let original = s.motion(&id(3)).unwrap();
    let history = s.history_lengths();
    s.edit("Move keys", None, |e| {
        e.move_motion_keys(&id(3), &[movement(0, 1, 0.6, 1.), movement(0, 2, 0.65, 0.)])
    })
    .unwrap();
    let changed = s.motion(&id(3)).unwrap();
    assert_eq!(changed.tracks[0].segments[0].end().time, 0.6);
    assert_eq!(changed.tracks[0].segments[1].end().time, 0.65);
    assert!(Arc::ptr_eq(
        &original.tracks[1].segments,
        &changed.tracks[1].segments
    ));
    assert_eq!(s.history_lengths(), (history.0 + 1, 0));
    s.undo().unwrap();
    assert_eq!(s.motion(&id(3)).unwrap(), original);
    s.redo().unwrap();
    assert_eq!(s.motion(&id(3)).unwrap(), changed);
}

#[test]
fn collisions_are_atomic_across_tracks_and_candidate_can_recover() {
    let mut s = session();
    let original = s.motion(&id(3)).unwrap();
    let version = s.version();
    let history = s.history_lengths();
    let moves = [movement(0, 1, 0.3, 0.9), movement(1, 1, 0.5, 0.8)];
    let mut c = candidate(&s);
    assert_eq!(
        c.move_keys(s.document(), &moves).unwrap_err().code.as_ref(),
        "MOTION_KEY_COLLISION"
    );
    assert_eq!(c.clip(), &original);
    c.move_keys(s.document(), &moves[..1]).unwrap();
    let last_legal = c.clone();
    assert!(c.move_keys(s.document(), &moves).is_err());
    assert_eq!(c, last_legal);
    assert!(s
        .edit("Collision", None, |e| e.move_motion_keys(&id(3), &moves))
        .is_err());
    assert_eq!(s.motion(&id(3)).unwrap(), original);
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
}

#[test]
fn no_op_commit_keeps_history_version_redo_and_shared_curves() {
    let mut s = session();
    s.edit("Move", None, |e| {
        e.move_motion_keys(&id(3), &[movement(0, 1, 0.3, 1.)])
    })
    .unwrap();
    s.undo().unwrap();
    let version = s.version();
    let history = s.history_lengths();
    let c = candidate(&s);
    s.edit("No change", None, |e| e.replace_motion_candidate(c))
        .unwrap();
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
}

#[test]
fn insertion_preserves_all_four_interpolations_including_step_boundaries() {
    for kind in [
        MotionInterpolation::Linear,
        MotionInterpolation::Bezier,
        MotionInterpolation::Stepped,
        MotionInterpolation::InverseStepped,
    ] {
        let s = session();
        let mut c = candidate(&s);
        c.set_interpolation(s.document(), &[reference(0, 0)], kind)
            .unwrap();
        let before = samples(c.clip());
        c.insert_key(s.document(), &id(10), &id(500), 0.1137, None)
            .unwrap();
        assert_samples_equal(&before, &samples(c.clip()));
        assert_eq!(c.clip().tracks[0].segments[0].end().id, id(500));
        assert_eq!(c.clip().tracks[0].segments[1].end().id, id(101));
        let curve = CompiledCurve::from(&c.clip().tracks[0]);
        assert_eq!(sample_motion_curve(&curve, 0.25, false), before[250]);
    }
}

#[test]
fn de_casteljau_preserves_restricted_and_ordinary_bezier_samples() {
    for restricted in [false, true] {
        for time in [0.0137, 0.137, 0.501, 0.89, 0.987] {
            let s = bezier_session(restricted);
            let mut c = candidate(&s);
            let before = samples(c.clip());
            c.insert_key(s.document(), &id(10), &id(500), time, None)
                .unwrap();
            assert_samples_equal(&before, &samples(c.clip()));
            assert_eq!(c.clip().tracks[0].segments[1].end().id, id(104));
            if restricted {
                let mut start = 0.;
                for segment in c.clip().tracks[0].segments.iter() {
                    let MotionSegment::Bezier {
                        control1,
                        control2,
                        end,
                    } = segment
                    else {
                        panic!()
                    };
                    assert!((control1.time - (start + (end.time - start) / 3.)).abs() < 0.000001);
                    start = end.time;
                    assert!(control2.time <= end.time);
                }
            }
        }
    }
}

#[test]
fn insertion_value_change_moves_only_adjoining_handles() {
    let s = bezier_session(false);
    let mut c = candidate(&s);
    c.insert_key(s.document(), &id(10), &id(500), 0.4, None)
        .unwrap();
    let split = c.clone();
    let old_value = c.clip().tracks[0].segments[0].end().value;
    let mut written = candidate(&s);
    written
        .insert_key(s.document(), &id(10), &id(500), 0.4, Some(0.8))
        .unwrap();
    let [MotionSegment::Bezier {
        control1: a1,
        control2: a2,
        ..
    }, MotionSegment::Bezier {
        control1: b1,
        control2: b2,
        ..
    }] = written.clip().tracks[0].segments.as_slice()
    else {
        panic!()
    };
    let [MotionSegment::Bezier {
        control1: x1,
        control2: x2,
        ..
    }, MotionSegment::Bezier {
        control1: y1,
        control2: y2,
        ..
    }] = split.clip().tracks[0].segments.as_slice()
    else {
        panic!()
    };
    assert_eq!(a1, x1);
    assert_eq!(b2, y2);
    assert!((a2.value - x2.value - (0.8 - old_value)).abs() < 0.000001);
    assert!((b1.value - y1.value - (0.8 - old_value)).abs() < 0.000001);
}

#[test]
fn endpoint_insertion_holds_endpoint_and_initial_promotion_preserves_ids() {
    let s = session();
    let mut c = candidate(&s);
    c.delete_keys(s.document(), &[reference(0, 0), reference(0, 4)])
        .unwrap();
    assert_eq!(c.clip().tracks[0].initial.id, id(101));
    c.insert_key(s.document(), &id(10), &id(500), 0.125, None)
        .unwrap();
    assert_eq!(c.clip().tracks[0].initial.id, id(500));
    assert_eq!(c.clip().tracks[0].initial.value, 1.);
    assert_eq!(c.clip().tracks[0].segments[0].end().id, id(101));
    c.insert_key(s.document(), &id(10), &id(501), 0.9, None)
        .unwrap();
    assert_eq!(c.clip().tracks[0].segments.last().unwrap().end().value, 1.);
}

#[test]
fn multi_delete_uses_final_survivors_and_removes_empty_track() {
    for kind in [
        MotionInterpolation::Linear,
        MotionInterpolation::Bezier,
        MotionInterpolation::Stepped,
        MotionInterpolation::InverseStepped,
    ] {
        let s = session();
        let mut c = candidate(&s);
        c.set_interpolation(
            s.document(),
            &(0..4).map(|i| reference(0, i)).collect::<Vec<_>>(),
            kind,
        )
        .unwrap();
        let mut reverse = c.clone();
        c.delete_keys(s.document(), &[reference(0, 1), reference(0, 2)])
            .unwrap();
        reverse
            .delete_keys(s.document(), &[reference(0, 2), reference(0, 1)])
            .unwrap();
        assert_eq!(c, reverse);
        assert_eq!(c.clip().tracks[0].segments[0].end().id, id(103));
        assert_eq!(
            std::mem::discriminant(&c.clip().tracks[0].segments[0]),
            std::mem::discriminant(&c.clip().tracks[0].segments[1])
        );
        c.delete_keys(
            s.document(),
            &[reference(0, 0), reference(0, 3), reference(0, 4)],
        )
        .unwrap();
        assert_eq!(c.clip().tracks.len(), 1);
        assert_eq!(c.clip().tracks[0].id, id(11));
    }
}

#[test]
fn mixed_deletion_connects_linearly_and_keeps_outer_bezier_values_when_merging() {
    let s = session();
    let mut c = candidate(&s);
    c.set_interpolation(
        s.document(),
        &[reference(0, 0)],
        MotionInterpolation::Bezier,
    )
    .unwrap();
    c.delete_keys(s.document(), &[reference(0, 1)]).unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0],
        MotionSegment::Linear { .. }
    ));
    let mut c = candidate(&s);
    c.set_interpolation(
        s.document(),
        &[reference(0, 0), reference(0, 1)],
        MotionInterpolation::Bezier,
    )
    .unwrap();
    let MotionSegment::Bezier { control1, .. } = c.clip().tracks[0].segments[0] else {
        panic!()
    };
    let MotionSegment::Bezier { control2, .. } = c.clip().tracks[0].segments[1] else {
        panic!()
    };
    c.delete_keys(s.document(), &[reference(0, 1)]).unwrap();
    let MotionSegment::Bezier {
        control1: first,
        control2: second,
        ..
    } = c.clip().tracks[0].segments[0]
    else {
        panic!()
    };
    assert_eq!(first, control1);
    assert_eq!(second, control2);
}

#[test]
fn restricted_handle_time_is_locked_and_endpoint_edits_normalize_only_touched_segments() {
    let s = bezier_session(true);
    let mut c = candidate(&s);
    let original = c.clone();
    assert_eq!(
        c.set_handle(
            s.document(),
            &reference(0, 4),
            MotionBezierHandle::Control1,
            MotionPoint {
                time: 0.2,
                value: 4.
            }
        )
        .unwrap_err()
        .code
        .as_ref(),
        "RESTRICTED_BEZIER_TIME"
    );
    assert_eq!(c, original);
    let expected = (1f64 / 3.) as f32;
    c.set_handle(
        s.document(),
        &reference(0, 4),
        MotionBezierHandle::Control1,
        MotionPoint {
            time: expected,
            value: 4.,
        },
    )
    .unwrap();
    let MotionSegment::Bezier {
        control1, control2, ..
    } = c.clip().tracks[0].segments[0]
    else {
        panic!()
    };
    assert_eq!(control1.value, 4.); // Values can overshoot the model range.
    assert_eq!(control1.time, expected);
    assert_eq!(control2.time, (2f64 / 3.) as f32);
    assert!(Arc::ptr_eq(
        &c.clip().tracks[1].segments,
        &original.clip().tracks[1].segments
    ));
}

#[test]
fn common_endpoint_translation_translates_entire_ordinary_curve() {
    let mut s = bezier_session(false);
    let mut clip = s.motion(&id(3)).unwrap();
    clip.duration = 2.;
    s.edit("Extend", None, |e| e.replace_motion(clip)).unwrap();
    let mut c = candidate(&s);
    c.move_keys(
        s.document(),
        &[movement(0, 0, 0.5, 0.2), movement(0, 4, 1.5, 1.2)],
    )
    .unwrap();
    let MotionSegment::Bezier {
        control1, control2, ..
    } = c.clip().tracks[0].segments[0]
    else {
        panic!()
    };
    assert!((control1.time - 0.7).abs() < 0.000001);
    assert!((control2.time - 1.4).abs() < 0.000001);
    assert!((control1.value - 2.).abs() < 0.000001);
    assert!((control2.value + 1.).abs() < 0.000001);
}

#[test]
fn non_monotonic_bezier_supports_value_edits_and_rejects_splits_and_time_edits() {
    let mut s = bezier_session(false);
    let mut clip = s.motion(&id(3)).unwrap();
    let MotionSegment::Bezier {
        control1, control2, ..
    } = &mut Arc::make_mut(&mut clip.tracks[0].segments)[0]
    else {
        panic!()
    };
    control1.time = 1.4;
    control2.time = -0.4;
    s.edit("Nonmonotonic", None, |e| e.replace_motion(clip))
        .unwrap();
    let mut c = candidate(&s);
    assert_eq!(
        c.insert_key(s.document(), &id(10), &id(500), 0.4, None)
            .unwrap_err()
            .code
            .as_ref(),
        "NON_MONOTONIC_MOTION_BEZIER"
    );
    assert_eq!(
        c.move_keys(s.document(), &[movement(0, 4, 0.9, 1.)])
            .unwrap_err()
            .code
            .as_ref(),
        "NON_MONOTONIC_MOTION_BEZIER"
    );
    c.move_keys(s.document(), &[movement(0, 4, 1., 0.8)])
        .unwrap();
    let MotionSegment::Bezier {
        control1, control2, ..
    } = c.clip().tracks[0].segments[0]
    else {
        panic!()
    };
    assert_eq!(control1.time, 1.4);
    assert_eq!(control2.time, -0.4);
}

#[test]
fn candidate_snapshots_preview_independently_without_authoring_mutation() {
    let s = session();
    let original = s.read_snapshot();
    let history = s.history_lengths();
    let mut c = candidate(&s);
    c.move_keys(
        s.document(),
        &[movement(0, 1, 0.25, 0.5), movement(1, 1, 0.25, 0.5)],
    )
    .unwrap();
    let frozen = original.with_motion_candidate(&c).unwrap();
    c.move_keys(s.document(), &[movement(0, 1, 0.25, 0.8)])
        .unwrap();
    assert_eq!(
        frozen.document().get_motion(&id(3)).unwrap().tracks[0].segments[0]
            .end()
            .value,
        0.5
    );
    assert_eq!(
        original.document().get_motion(&id(3)).unwrap().tracks[0].segments[0]
            .end()
            .value,
        1.
    );
    assert_eq!(original.version(), frozen.version());
    let mut preview = MotionPreview::new(frozen.document());
    preview.schedule_motion(&id(3), 0.).unwrap();
    preview.advance(0.).unwrap();
    assert!((preview.advance(0.25).unwrap().parameters[&id(2)] - 0.5).abs() < 0.00001);
    assert_eq!(s.history_lengths(), history);
    assert_eq!(s.version(), original.version());
    assert_eq!(
        s.motion(&id(3)).unwrap(),
        *original.document().get_motion(&id(3)).unwrap()
    );
}

#[test]
fn id_validation_rejects_missing_duplicate_and_cross_clip_keys() {
    let mut s = session();
    for bad in [String::new(), id(2), id(101), id(110), id(20)] {
        let mut clip = s.motion(&id(3)).unwrap();
        clip.tracks[0].initial.id = bad;
        assert!(s
            .edit("Invalid identity", None, |e| e.replace_motion(clip))
            .is_err());
    }
    let mut second = s.motion(&id(3)).unwrap();
    second.id = id(4);
    second.name = "Other".into();
    second.events.clear();
    second.tracks.truncate(1);
    second.tracks[0].id = id(12);
    assert_eq!(
        s.edit("Reused keys", None, |e| e.create_motion(second))
            .unwrap_err()
            .code
            .as_ref(),
        "DUPLICATE_ID"
    );
    let mut c = candidate(&s);
    let mut unknown = movement(0, 1, 0.3, 1.);
    unknown.key.key_id = id(999);
    assert_eq!(
        c.move_keys(s.document(), &[unknown])
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_MOTION_KEY"
    );
    let same = movement(0, 1, 0.3, 1.);
    assert_eq!(
        c.move_keys(s.document(), &[same.clone(), same])
            .unwrap_err()
            .code
            .as_ref(),
        "DUPLICATE_MOTION_KEY_EDIT"
    );
}

#[test]
fn edited_times_and_values_are_bounded_without_quantizing_imported_tail() {
    let mut s = session();
    let mut clip = s.motion(&id(3)).unwrap();
    Arc::make_mut(&mut clip.tracks[0].segments)[3]
        .end_mut()
        .time = 1.01;
    Arc::make_mut(&mut clip.tracks[0].segments)[3]
        .end_mut()
        .value = 4.;
    s.edit("Imported tail", None, |e| e.replace_motion(clip))
        .unwrap();
    let mut c = candidate(&s);
    c.move_keys(s.document(), &[movement(0, 1, 0.25137, 0.8)])
        .unwrap();
    assert_eq!(c.clip().tracks[0].segments[0].end().time, 0.25137);
    assert_eq!(c.clip().tracks[0].segments[3].end().time, 1.01);
    assert_eq!(
        c.move_keys(s.document(), &[movement(0, 4, 1.005, 4.)])
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_MOTION_KEY_TIME"
    );
    c.move_keys(s.document(), &[movement(0, 4, 1., 1.)])
        .unwrap();
    for value in [f32::NAN, f32::INFINITY, 2.01] {
        assert!(c
            .move_keys(s.document(), &[movement(0, 1, 0.25137, value)])
            .is_err());
    }
    let rounded = (0.5f64 + 0.000000001) as f32;
    assert_eq!(
        c.move_keys(s.document(), &[movement(0, 1, rounded, 0.8)])
            .unwrap_err()
            .code
            .as_ref(),
        "MOTION_KEY_COLLISION"
    );
    let before = s.motion(&id(3)).unwrap();
    assert!(s
        .edit("FPS", None, |e| e.set_motion_timing(
            &id(3),
            1.,
            120.,
            false,
            Some(0.),
            Some(0.)
        ))
        .is_err());
    assert_eq!(s.motion(&id(3)).unwrap(), before);
}

#[test]
fn scale_time_transforms_controls_events_and_keys_preserving_ids() {
    let s = bezier_session(true);
    let original = s.motion(&id(3)).unwrap();
    let mut c = candidate(&s);
    c.scale_time(s.document(), 2.5).unwrap();
    assert_eq!(c.clip().duration, 2.5);
    assert_eq!(c.clip().events[0].time, 1.25);
    for (a, b) in original.tracks.iter().zip(&c.clip().tracks) {
        for (a, b) in a.keys().zip(b.keys()) {
            assert_eq!(a.id, b.id);
            assert_eq!(b.time, a.time * 2.5);
        }
    }
    let MotionSegment::Bezier {
        control1, control2, ..
    } = c.clip().tracks[0].segments[0]
    else {
        panic!()
    };
    assert_eq!(control1.time, (2.5f64 / 3.) as f32);
    assert_eq!(control2.time, (5f64 / 3.) as f32);
    let reopened = decode_project(
        &encode_project(
            &s.read_snapshot()
                .with_motion_candidate(&c)
                .unwrap()
                .document(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.get_motion(&id(3)), Some(c.clip()));
}

#[test]
fn opaque_and_unknown_targets_are_read_only_in_curve_candidates() {
    let mut s = session();
    let mut clip = s.motion(&id(3)).unwrap();
    clip.tracks[0].target = MotionTrackTarget::Model {
        runtime_id: "Future".into(),
    };
    s.edit("Unknown", None, |e| e.replace_motion(clip)).unwrap();
    let mut c = candidate(&s);
    assert_eq!(
        c.move_keys(s.document(), &[movement(0, 1, 0.3, 1.)])
            .unwrap_err()
            .code
            .as_ref(),
        "READ_ONLY_MOTION_TRACK"
    );
    c.move_keys(s.document(), &[movement(1, 1, 0.3, 1.)])
        .unwrap();
    let mut clip = s.motion(&id(3)).unwrap();
    clip.extensions
        .insert("Future".into(), serde_json::json!(true));
    s.edit("Opaque", None, |e| e.replace_motion(clip)).unwrap();
    assert_eq!(
        MotionClipCandidate::new(s.document(), s.motion(&id(3)).unwrap())
            .unwrap_err()
            .code
            .as_ref(),
        "OPAQUE_EDIT_REQUIRES_IMPORT"
    );
}

#[test]
fn stale_candidate_commit_is_rejected_after_another_authoring_edit() {
    let mut s = session();
    let base = s.version();
    let c = candidate(&s);
    s.edit("Intervening", None, |e| {
        e.move_motion_keys(&id(3), &[movement(0, 1, 0.3, 1.)])
    })
    .unwrap();
    assert_eq!(
        s.edit("Stale", Some(base), |e| e.replace_motion_candidate(c))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_VERSION"
    );
}

#[test]
fn single_key_track_is_constant_without_an_implicit_zero_key_and_opacity_is_bounded() {
    let mut s = session();
    s.edit("Single opacity key", None, |e| {
        e.create_motion_track(
            &id(3),
            MotionTrack {
                id: id(12),
                target: MotionTrackTarget::Model {
                    runtime_id: "Opacity".into(),
                },
                initial: key(120, 0.6137, 0.4),
                segments: Arc::new(vec![]),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            },
        )
    })
    .unwrap();
    let mut c = candidate(&s);
    let track = &c.clip().tracks[2];
    assert_eq!(track.keys().count(), 1);
    let curve = CompiledCurve::from(track);
    for time in [0., 0.3, 0.6137, 1.] {
        assert_eq!(sample_motion_curve(&curve, time, false), 0.4);
    }
    let key_ref = MotionKeyRef {
        track_id: id(12),
        key_id: id(120),
    };
    assert_eq!(
        c.move_keys(
            s.document(),
            &[MotionKeyMove {
                key: key_ref.clone(),
                point: MotionPoint {
                    time: 0.6137,
                    value: 1.01
                }
            }]
        )
        .unwrap_err()
        .code
        .as_ref(),
        "MOTION_VALUE_OUT_OF_RANGE"
    );
    c.move_keys(
        s.document(),
        &[MotionKeyMove {
            key: key_ref,
            point: MotionPoint {
                time: 0.6137,
                value: 1.,
            },
        }],
    )
    .unwrap();
    c.delete_keys(
        s.document(),
        &[MotionKeyRef {
            track_id: id(12),
            key_id: id(120),
        }],
    )
    .unwrap();
    assert_eq!(c.clip().tracks.len(), 2);
}

#[test]
fn ill_conditioned_f32_bezier_splits_preserve_samples_or_reject_atomically() {
    for (start, span) in [(0., 0.001), (9., 0.01), (1000., 1.), (100_000., 1.)] {
        let mut s = bezier_session(false);
        let mut clip = s.motion(&id(3)).unwrap();
        clip.duration = start + span + 1.;
        clip.tracks[0].initial.time = start;
        let MotionSegment::Bezier {
            control1,
            control2,
            end,
        } = &mut Arc::make_mut(&mut clip.tracks[0].segments)[0]
        else {
            panic!()
        };
        control1.time = start + span * 0.2;
        control2.time = start + span * 0.9;
        end.time = start + span;
        s.edit("Offset curve", None, |e| e.replace_motion(clip))
            .unwrap();
        let mut c = candidate(&s);
        let original = c.clone();
        let before = CompiledCurve::from(&original.clip().tracks[0]);
        let inserted = c.insert_key(s.document(), &id(10), &id(500), start + span * 0.4, None);
        match inserted {
            Ok(()) => {
                let after = CompiledCurve::from(&c.clip().tracks[0]);
                for i in 0..=200 {
                    let time = (f64::from(start) + f64::from(span) * f64::from(i) / 200.) as f32;
                    let (a, b) = (
                        sample_motion_curve(&before, time, false),
                        sample_motion_curve(&after, time, false),
                    );
                    assert!(
                        (a - b).abs() < 0.0003,
                        "start={start}, span={span}, time={time}: {a} != {b}"
                    );
                }
            }
            Err(error) => {
                assert_eq!(error.code.as_ref(), "MOTION_BEZIER_SPLIT_PRECISION");
                assert_eq!(c, original);
            }
        }
    }
}
