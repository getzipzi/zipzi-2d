use super::*;
use zipzi_core::document::MotionEvent;
use zipzi_core::{state::DocumentState, Canvas, Parameter, Vec2};
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
fn session() -> DocumentBuilder {
    let mut s = DocumentState::new(id(1), Canvas::new(100., 100., Vec2::default(), 1.).into())
        .unwrap()
        .builder();
    s.create_parameter(Parameter {
        id: id(2),
        runtime_id: "ParamX".into(),
        minimum: -2.,
        maximum: 2.,
        ..Default::default()
    })
    .unwrap();
    s.create_authored_motion(MotionClip {
        track_order: Vec::new(),
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
        }]
        .into(),
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    })
    .unwrap();
    s.validate_candidate().unwrap();
    s
}
fn candidate(s: &DocumentBuilder) -> CurveWorkspace {
    CurveWorkspace::new(s, s.motion(&id(3)).unwrap().clone()).unwrap()
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

fn bezier_session(restricted: bool) -> DocumentBuilder {
    let mut s = session();
    let mut clip = s.motion(&id(3)).unwrap().clone();
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
    s.replace_authored_motion(clip).unwrap();
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
fn insertion_preserves_all_four_interpolations_including_step_boundaries() {
    for kind in [
        MotionInterpolation::Linear,
        MotionInterpolation::Bezier,
        MotionInterpolation::Stepped,
        MotionInterpolation::InverseStepped,
    ] {
        let s = session();
        let mut c = candidate(&s);
        c.set_interpolation(&s, &[reference(0, 0)], kind).unwrap();
        let before = samples(c.clip());
        c.insert_key(&s, &id(10), &id(500), 0.1137, None).unwrap();
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
            c.insert_key(&s, &id(10), &id(500), time, None).unwrap();
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
    c.insert_key(&s, &id(10), &id(500), 0.4, None).unwrap();
    let split = c.clone();
    let old_value = c.clip().tracks[0].segments[0].end().value;
    let mut written = candidate(&s);
    written
        .insert_key(&s, &id(10), &id(500), 0.4, Some(0.8))
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
    c.delete_keys(&s, &[reference(0, 0), reference(0, 4)])
        .unwrap();
    assert_eq!(c.clip().tracks[0].initial.id, id(101));
    c.insert_key(&s, &id(10), &id(500), 0.125, None).unwrap();
    assert_eq!(c.clip().tracks[0].initial.id, id(500));
    assert_eq!(c.clip().tracks[0].initial.value, 1.);
    assert_eq!(c.clip().tracks[0].segments[0].end().id, id(101));
    c.insert_key(&s, &id(10), &id(501), 0.9, None).unwrap();
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
            &s,
            &(0..4).map(|i| reference(0, i)).collect::<Vec<_>>(),
            kind,
        )
        .unwrap();
        let mut reverse = c.clone();
        c.delete_keys(&s, &[reference(0, 1), reference(0, 2)])
            .unwrap();
        reverse
            .delete_keys(&s, &[reference(0, 2), reference(0, 1)])
            .unwrap();
        assert_eq!(c, reverse);
        assert_eq!(c.clip().tracks[0].segments[0].end().id, id(103));
        assert_eq!(
            std::mem::discriminant(&c.clip().tracks[0].segments[0]),
            std::mem::discriminant(&c.clip().tracks[0].segments[1])
        );
        c.delete_keys(&s, &[reference(0, 0), reference(0, 3), reference(0, 4)])
            .unwrap();
        assert_eq!(c.clip().tracks.len(), 1);
        assert_eq!(c.clip().tracks[0].id, id(11));
    }
}

#[test]
fn mixed_deletion_connects_linearly_and_keeps_outer_bezier_values_when_merging() {
    let s = session();
    let mut c = candidate(&s);
    c.set_interpolation(&s, &[reference(0, 0)], MotionInterpolation::Bezier)
        .unwrap();
    c.delete_keys(&s, &[reference(0, 1)]).unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0],
        MotionSegment::Linear { .. }
    ));
    let mut c = candidate(&s);
    c.set_interpolation(
        &s,
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
    c.delete_keys(&s, &[reference(0, 1)]).unwrap();
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
            &s,
            &reference(0, 4),
            MotionBezierHandle::Control1,
            MotionPoint {
                time: 0.2,
                value: 4.
            }
        )
        .unwrap_err()
        .code
        .as_str(),
        "RESTRICTED_BEZIER_TIME"
    );
    assert_eq!(c, original);
    let expected = (1f64 / 3.) as f32;
    c.set_handle(
        &s,
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
    let mut clip = s.motion(&id(3)).unwrap().clone();
    clip.duration = 2.;
    s.replace_authored_motion(clip).unwrap();
    let mut c = candidate(&s);
    c.move_keys(&s, &[movement(0, 0, 0.5, 0.2), movement(0, 4, 1.5, 1.2)])
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
    let mut clip = s.motion(&id(3)).unwrap().clone();
    let MotionSegment::Bezier {
        control1, control2, ..
    } = &mut Arc::make_mut(&mut clip.tracks[0].segments)[0]
    else {
        panic!()
    };
    control1.time = 1.4;
    control2.time = -0.4;
    s.replace_authored_motion(clip).unwrap();
    let mut c = candidate(&s);
    assert_eq!(
        c.insert_key(&s, &id(10), &id(500), 0.4, None)
            .unwrap_err()
            .code
            .as_str(),
        "NON_MONOTONIC_MOTION_BEZIER"
    );
    assert_eq!(
        c.move_keys(&s, &[movement(0, 4, 0.9, 1.)])
            .unwrap_err()
            .code
            .as_str(),
        "NON_MONOTONIC_MOTION_BEZIER"
    );
    c.move_keys(&s, &[movement(0, 4, 1., 0.8)]).unwrap();
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
fn ill_conditioned_f32_bezier_splits_preserve_samples_or_reject_atomically() {
    for (start, span) in [(0., 0.001), (9., 0.01), (1000., 1.), (100_000., 1.)] {
        let mut s = bezier_session(false);
        let mut clip = s.motion(&id(3)).unwrap().clone();
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
        s.replace_authored_motion(clip).unwrap();
        let mut c = candidate(&s);
        let original = c.clone();
        let before = CompiledCurve::from(&original.clip().tracks[0]);
        let inserted = c.insert_key(&s, &id(10), &id(500), start + span * 0.4, None);
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
                assert_eq!(error.code.as_str(), "MOTION_BEZIER_SPLIT_PRECISION");
                assert_eq!(c, original);
            }
        }
    }
}

#[test]
fn curve_commands_publish_once_share_untouched_curves_and_abort_failed_batches() {
    use crate::{AuthoringSession, NameTarget, ResourceView, Snapshot};
    let state = session().freeze().unwrap().state;
    let original = state.motion(&id(3)).unwrap().clone();
    let mut s = AuthoringSession::new(Snapshot::new(state, ResourceView::default()).unwrap());
    let baseline = s.snapshot().content_id();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.edit_motion_curve(
                &id(3),
                MotionCurveEdit::MoveKeys(vec![movement(0, 1, 0.6, 1.), movement(0, 2, 0.65, 0.)]),
            )?;
            c.edit_motion_curve(&id(3), MotionCurveEdit::ScaleTime { duration: 2. })
        })
        .unwrap();
    s.publish(draft.prepare("Move and scale").unwrap()).unwrap();
    let changed = s.snapshot().document().motion(&id(3)).unwrap();
    assert_eq!(changed.tracks[0].segments[0].end().time, 1.2);
    assert_eq!(changed.tracks[0].segments[1].end().time, 1.3);
    assert_eq!(changed.events[0].time, 1.);
    assert_eq!(changed.events[0].id, id(20));
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), baseline);
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.edit_motion_curve(
                &id(3),
                MotionCurveEdit::MoveKeys(vec![movement(0, 1, 0.3, 1.)]),
            )
        })
        .unwrap();
    s.publish(draft.prepare("One track").unwrap()).unwrap();
    let changed = s.snapshot().document().motion(&id(3)).unwrap();
    assert!(Arc::ptr_eq(
        &original.tracks[1].segments,
        &changed.tracks[1].segments
    ));
    s.undo(s.version()).unwrap();
    let version = s.version();
    let history = s.history_state();
    let mut draft = s.begin_draft();
    let failure = draft
        .batch(draft.source(), |c| {
            c.rename(NameTarget::Parameter(&id(2)), "Discarded".into())?;
            // Ignoring a failed curve edit still aborts every preceding command.
            let _ = c.edit_motion_curve(
                &id(3),
                MotionCurveEdit::MoveKeys(vec![movement(0, 1, 0.5, 1.)]),
            );
            Ok(())
        })
        .unwrap_err();
    assert_eq!(failure.code, "MOTION_KEY_COLLISION");
    assert!(draft.prepare("Invalid").is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert_eq!(s.snapshot().content_id(), baseline);
}

#[test]
fn curve_command_honors_owned_key_locks_and_deletion_cleans_editor_flags() {
    use crate::{AuthoringSession, ResourceView, Snapshot};
    use zipzi_core::document::ObjectEditorState;
    for locked in [true, false] {
        let mut b = session();
        let mut editor = b.editor_state().clone();
        editor.objects.insert(
            id(101),
            ObjectEditorState {
                locked,
                hidden_in_editor: true,
            },
        );
        b.set_editor_state(editor).unwrap();
        let mut s = AuthoringSession::new(
            Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
        );
        let before = s.snapshot().content_id();
        let mut draft = s.begin_draft();
        let result = draft.batch(draft.source(), |c| {
            c.edit_motion_curve(&id(3), MotionCurveEdit::DeleteKeys(vec![reference(0, 1)]))
        });
        if locked {
            assert_eq!(result.unwrap_err().code, "OBJECT_LOCKED");
            assert!(draft.prepare("Locked").is_err());
            assert_eq!(s.snapshot().content_id(), before);
        } else {
            result.unwrap();
            s.publish(draft.prepare("Delete key").unwrap()).unwrap();
            assert!(!s
                .snapshot()
                .document()
                .editor_state()
                .objects
                .contains_key(&id(101)));
            s.undo(s.version()).unwrap();
            assert_eq!(s.snapshot().content_id(), before);
            assert!(s
                .snapshot()
                .document()
                .editor_state()
                .objects
                .contains_key(&id(101)));
        }
    }
}
