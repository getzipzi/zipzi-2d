use super::*;
use zipzi_core::document::MotionCurve;
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
                        .map(|i| {
                            MotionSegment::new(
                                key(100 + index * 10 + i, i as f32 * 0.25, (i % 2) as f32),
                                MotionCurve::Linear {},
                            )
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
    clip.tracks[0].segments = Arc::new(vec![MotionSegment::new(
        key(104, 1., 1.),
        MotionCurve::Bezier {
            control1: MotionPoint {
                time: 0.2,
                value: 1.8,
            },
            control2: MotionPoint {
                time: 0.9,
                value: -1.2,
            },
            preset: None,
        },
    )]);
    s.replace_authored_motion(clip).unwrap();
    s
}

fn samples(clip: &MotionClip) -> Vec<f32> {
    let curve = CompiledCurve::from_track(&clip.tracks[0], clip.fps);
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
        let curve = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
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
                    let MotionSegment {
                        end,
                        curve:
                            MotionCurve::Bezier {
                                control1, control2, ..
                            },
                        ..
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
    let [MotionSegment {
        curve:
            MotionCurve::Bezier {
                control1: a1,
                control2: a2,
                ..
            },
        ..
    }, MotionSegment {
        curve:
            MotionCurve::Bezier {
                control1: b1,
                control2: b2,
                ..
            },
        ..
    }] = written.clip().tracks[0].segments.as_slice()
    else {
        panic!()
    };
    let [MotionSegment {
        curve:
            MotionCurve::Bezier {
                control1: x1,
                control2: x2,
                ..
            },
        ..
    }, MotionSegment {
        curve:
            MotionCurve::Bezier {
                control1: y1,
                control2: y2,
                ..
            },
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
            std::mem::discriminant(&c.clip().tracks[0].segments[0].curve),
            std::mem::discriminant(&c.clip().tracks[0].segments[1].curve)
        );
        c.delete_keys(&s, &[reference(0, 0), reference(0, 3), reference(0, 4)])
            .unwrap();
        assert_eq!(c.clip().tracks.len(), 1);
        assert_eq!(c.clip().tracks[0].id, id(11));
    }
}

#[test]
fn deletion_inherits_the_left_curve_and_keeps_outer_bezier_values_when_merging() {
    let s = session();
    let mut c = candidate(&s);
    c.set_interpolation(&s, &[reference(0, 0)], MotionInterpolation::Bezier)
        .unwrap();
    c.delete_keys(&s, &[reference(0, 1)]).unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0],
        MotionSegment {
            curve: MotionCurve::Bezier { .. },
            ..
        }
    ));
    let mut c = candidate(&s);
    c.set_interpolation(
        &s,
        &[reference(0, 0), reference(0, 1)],
        MotionInterpolation::Bezier,
    )
    .unwrap();
    let MotionSegment {
        curve: MotionCurve::Bezier { control1, .. },
        ..
    } = c.clip().tracks[0].segments[0]
    else {
        panic!()
    };
    let MotionSegment {
        curve: MotionCurve::Bezier { control2, .. },
        ..
    } = c.clip().tracks[0].segments[1]
    else {
        panic!()
    };
    c.delete_keys(&s, &[reference(0, 1)]).unwrap();
    let MotionSegment {
        curve:
            MotionCurve::Bezier {
                control1: first,
                control2: second,
                ..
            },
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
    let MotionSegment {
        curve: MotionCurve::Bezier {
            control1, control2, ..
        },
        ..
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
    let MotionSegment {
        curve: MotionCurve::Bezier {
            control1, control2, ..
        },
        ..
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
    let MotionSegment {
        curve: MotionCurve::Bezier {
            control1, control2, ..
        },
        ..
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
    let MotionSegment {
        curve: MotionCurve::Bezier {
            control1, control2, ..
        },
        ..
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
        let MotionSegment {
            end,
            curve: MotionCurve::Bezier {
                control1, control2, ..
            },
            ..
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
        let before = CompiledCurve::from_track(&original.clip().tracks[0], original.clip().fps);
        let inserted = c.insert_key(&s, &id(10), &id(500), start + span * 0.4, None);
        match inserted {
            Ok(()) => {
                let after = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
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

#[test]
fn authoring_presets_map_per_interval_retain_steps_and_clear_on_manual_edits() {
    let s = session();
    for preset in MotionEasingPreset::ALL {
        let mut c = candidate(&s);
        let keys = [reference(0, 0), reference(0, 1), reference(1, 0)];
        c.move_keys(&s, &[movement(1, 1, 0.25, 0.)]).unwrap();
        c.set_frame_step(&s, &keys, MotionFrameStep::from_frames(3).unwrap())
            .unwrap();
        c.set_easing_preset(&s, &keys, preset).unwrap();
        for (track, index) in [(0, 0), (0, 1), (1, 0)] {
            let track = &c.clip().tracks[track];
            let segment = &track.segments[index];
            let MotionCurve::Bezier {
                control1,
                control2,
                preset: Some(source),
            } = segment.curve
            else {
                panic!()
            };
            assert_eq!(source, preset);
            assert_eq!(
                [control1, control2],
                preset
                    .mapped_controls(
                        track.keys().nth(index).unwrap().point(),
                        segment.end.point()
                    )
                    .unwrap()
            );
            assert_eq!(
                segment.frame_step,
                MotionFrameStep::EveryFrames { frames: 3 }
            );
        }
        let MotionCurve::Bezier { control1, .. } = c.clip().tracks[0].segments[0].curve else {
            panic!()
        };
        c.set_handle(&s, &reference(0, 1), MotionBezierHandle::Control1, control1)
            .unwrap();
        assert!(matches!(
            c.clip().tracks[0].segments[0].curve,
            MotionCurve::Bezier { preset: None, .. }
        ));
        c.set_interpolation(&s, &keys, MotionInterpolation::Stepped)
            .unwrap();
        c.set_interpolation(&s, &keys, MotionInterpolation::Bezier)
            .unwrap();
        assert!(matches!(
            c.clip().tracks[0].segments[0].curve,
            MotionCurve::Bezier { preset: None, .. }
        ));
        assert_eq!(
            c.clip().tracks[0].segments[0].frame_step,
            MotionFrameStep::EveryFrames { frames: 3 }
        );
    }
}

#[test]
fn authoring_automatic_groups_recompute_and_materialize_the_original_handles() {
    let s = session();
    let mut c = candidate(&s);
    let keys: Vec<_> = (0..=4).map(|i| reference(0, i)).collect();
    c.move_keys(&s, &[movement(0, 1, 0.2, 0.8), movement(0, 2, 0.5, 0.9)])
        .unwrap();
    c.set_interpolation(&s, &keys, MotionInterpolation::AutoSmooth)
        .unwrap();
    let before = samples(c.clip());
    let compiled = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
    c.set_interpolation(&s, &keys, MotionInterpolation::Bezier)
        .unwrap();
    assert_samples_equal(&before, &samples(c.clip()));
    for (source, compiled) in c.clip().tracks[0]
        .segments
        .iter()
        .zip(compiled.segments.iter())
    {
        let MotionCurve::Bezier {
            control1,
            control2,
            preset: None,
        } = source.curve
        else {
            panic!()
        };
        let CompiledSegment::Bezier {
            control1: first,
            control2: second,
            ..
        } = compiled
        else {
            panic!()
        };
        assert_eq!((control1, control2), (*first, *second));
    }
    c.set_interpolation(&s, &keys, MotionInterpolation::AutoSmooth)
        .unwrap();
    let compiled = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
    let CompiledSegment::Bezier { control1, .. } = compiled.segments[1] else {
        panic!()
    };
    c.set_handle(&s, &reference(0, 2), MotionBezierHandle::Control1, control1)
        .unwrap();
    let after = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
    assert_eq!(after.segments[1], compiled.segments[1]);
    assert!(matches!(
        c.clip().tracks[0].segments[1].curve,
        MotionCurve::Bezier { preset: None, .. }
    ));
    assert!(matches!(
        c.clip().tracks[0].segments[2].curve,
        MotionCurve::AutoSmooth {}
    ));
}

#[test]
fn authoring_frame_step_inherits_through_structure_edits_and_normalizes_one() {
    let s = session();
    let mut c = candidate(&s);
    c.set_easing_preset(&s, &[reference(0, 0)], MotionEasingPreset::CubicOut)
        .unwrap();
    c.set_frame_step(
        &s,
        &[reference(0, 0)],
        MotionFrameStep::from_frames(2).unwrap(),
    )
    .unwrap();
    let before = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
    c.insert_key(&s, &id(10), &id(500), 0.1, None).unwrap();
    let after = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
    for i in 0..=100 {
        let t = i as f32 / 100.;
        assert!(
            (sample_motion_curve_base(&before, t, false)
                - sample_motion_curve_base(&after, t, false))
            .abs()
                < 0.0003
        );
    }
    assert_ne!(
        sample_motion_curve(&before, 0.12, false),
        sample_motion_curve(&after, 0.12, false)
    );
    for segment in &c.clip().tracks[0].segments[..2] {
        assert_eq!(
            segment.frame_step,
            MotionFrameStep::EveryFrames { frames: 2 }
        );
        assert!(matches!(
            segment.curve,
            MotionCurve::Bezier { preset: None, .. }
        ));
    }
    let inserted = MotionKeyRef {
        track_id: id(10),
        key_id: id(500),
    };
    c.set_frame_step(
        &s,
        &[reference(0, 0)],
        MotionFrameStep::from_frames(7).unwrap(),
    )
    .unwrap();
    c.set_interpolation(&s, &[reference(0, 0)], MotionInterpolation::AutoSmooth)
        .unwrap();
    c.delete_keys(&s, &[inserted]).unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0].curve,
        MotionCurve::AutoSmooth {}
    ));
    assert_eq!(
        c.clip().tracks[0].segments[0].frame_step,
        MotionFrameStep::EveryFrames { frames: 7 }
    );
    c.set_frame_step(
        &s,
        &[reference(0, 0), reference(0, 4)],
        MotionFrameStep::EveryFrames { frames: 1 },
    )
    .unwrap();
    assert_eq!(
        c.clip().tracks[0].segments[0].frame_step,
        MotionFrameStep::Off {}
    );
    let old = c.clone();
    assert_eq!(
        c.set_frame_step(
            &s,
            &[reference(0, 0)],
            MotionFrameStep::EveryFrames { frames: 0 }
        )
        .unwrap_err()
        .code,
        "INVALID_MOTION_FRAME_STEP"
    );
    assert_eq!(c, old);
    c.delete_keys(&s, &(1..=4).map(|i| reference(0, i)).collect::<Vec<_>>())
        .unwrap();
    c.insert_key(&s, &id(10), &id(501), 0.5, Some(1.)).unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0].curve,
        MotionCurve::AutoSmooth {}
    ));
}

#[test]
fn authoring_restricted_conversion_preserves_unrelated_track_samples() {
    let s = bezier_session(true);
    let mut c = candidate(&s);
    c.set_frame_step(
        &s,
        &[reference(0, 0)],
        MotionFrameStep::from_frames(7).unwrap(),
    )
    .unwrap();
    let before = samples(c.clip());
    c.set_easing_preset(&s, &[reference(1, 0)], MotionEasingPreset::CubicOut)
        .unwrap();
    assert!(c.clip().restricted_beziers);
    c.set_easing_preset(&s, &[reference(1, 0)], MotionEasingPreset::CubicInOut)
        .unwrap();
    assert!(!c.clip().restricted_beziers);
    assert_samples_equal(&before, &samples(c.clip()));
    assert_eq!(
        c.clip().tracks[0].segments[0].frame_step,
        MotionFrameStep::EveryFrames { frames: 7 }
    );
    let MotionCurve::Bezier {
        control1, control2, ..
    } = c.clip().tracks[0].segments[0].curve
    else {
        panic!()
    };
    assert_eq!((control1.time, control2.time), thirds(0., 1.));
}

#[test]
fn authoring_preset_translation_and_time_scaling_keep_source_but_single_key_moves_clear_it() {
    let s = session();
    let mut c = candidate(&s);
    c.set_easing_preset(&s, &[reference(0, 0)], MotionEasingPreset::CubicInOut)
        .unwrap();
    c.move_keys(&s, &[movement(0, 0, 0.05, 0.1), movement(0, 1, 0.3, 1.1)])
        .unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0].curve,
        MotionCurve::Bezier {
            preset: Some(MotionEasingPreset::CubicInOut),
            ..
        }
    ));
    c.scale_time(&s, 2.).unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0].curve,
        MotionCurve::Bezier {
            preset: Some(MotionEasingPreset::CubicInOut),
            ..
        }
    ));
    c.move_keys(&s, &[movement(0, 1, 0.65, 1.1)]).unwrap();
    assert!(matches!(
        c.clip().tracks[0].segments[0].curve,
        MotionCurve::Bezier { preset: None, .. }
    ));
}

#[test]
fn authoring_aligned_handles_materialize_both_auto_intervals_and_tidy_clears_source() {
    let s = session();
    let mut c = candidate(&s);
    c.set_interpolation(
        &s,
        &[reference(0, 0), reference(0, 1)],
        MotionInterpolation::AutoSmooth,
    )
    .unwrap();
    let before = CompiledCurve::from_track(&c.clip().tracks[0], c.clip().fps);
    let CompiledSegment::Bezier { control2: old, .. } = before.segments[0] else {
        panic!()
    };
    let point = MotionPoint {
        time: 0.35,
        value: 1.1,
    };
    c.set_handle_aligned(&s, &reference(0, 2), MotionBezierHandle::Control1, point)
        .unwrap();
    let MotionCurve::Bezier {
        control2: linked,
        preset: None,
        ..
    } = c.clip().tracks[0].segments[0].curve
    else {
        panic!()
    };
    assert!(matches!(
        c.clip().tracks[0].segments[1].curve,
        MotionCurve::Bezier { preset: None, .. }
    ));
    let anchor = c.clip().tracks[0].segments[0].end.point();
    let cross = (linked.time - anchor.time) * (point.value - anchor.value)
        - (linked.value - anchor.value) * (point.time - anchor.time);
    assert!(cross.abs() < 0.000001);
    let length = |p: MotionPoint| (p.time - anchor.time).hypot(p.value - anchor.value);
    assert!((length(old) - length(linked)).abs() < 0.000001);
    c.set_easing_preset(&s, &[reference(0, 0)], MotionEasingPreset::CircOut)
        .unwrap();
    c.tidy_handles(&s, &[reference(0, 0)]).unwrap();
    let MotionCurve::Bezier {
        control1,
        control2,
        preset: None,
    } = c.clip().tracks[0].segments[0].curve
    else {
        panic!()
    };
    assert_eq!((control1.time, control2.time), thirds(0., 0.25));
    assert!((control1.value - 1. / 3.).abs() < 0.000001);
}

#[test]
fn authoring_curve_commands_have_atomic_locks_history_and_canonical_frame_steps() {
    use crate::{AuthoringSession, ResourceView, Snapshot};
    use zipzi_core::document::ObjectEditorState;
    let mut b = session();
    let mut editor = b.editor_state().clone();
    editor.objects.insert(
        id(104),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    b.set_editor_state(editor).unwrap();
    let mut s = AuthoringSession::new(
        Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
    );
    let initial = s.snapshot().content_id();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |commands| {
            commands.edit_motion_curve(
                &id(3),
                MotionCurveEdit::FrameStep {
                    keys: vec![reference(0, 0)],
                    frame_step: MotionFrameStep::from_frames(3).unwrap(),
                },
            )
        })
        .unwrap();
    s.publish(draft.prepare("Step on threes").unwrap()).unwrap();
    assert_eq!(
        s.snapshot().document().motion(&id(3)).unwrap().tracks[0].segments[0].frame_step,
        MotionFrameStep::EveryFrames { frames: 3 }
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), initial);
    s.redo(s.version()).unwrap();
    let before = s.snapshot().content_id();
    let mut draft = s.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |commands| commands.edit_motion_curve(
                &id(3),
                MotionCurveEdit::EasingPreset {
                    keys: vec![reference(0, 0), reference(0, 3)],
                    preset: MotionEasingPreset::CubicOut
                }
            ))
            .unwrap_err()
            .code,
        "OBJECT_LOCKED"
    );
    assert_eq!(s.snapshot().content_id(), before);
    let mut clip = s.snapshot().document().motion(&id(3)).unwrap().clone();
    Arc::make_mut(&mut clip.tracks[1].segments)[0].frame_step =
        MotionFrameStep::EveryFrames { frames: 1 };
    let mut b = s.snapshot().document().builder();
    b.replace_motion(clip).unwrap();
    assert_eq!(
        b.freeze().unwrap().state.motion(&id(3)).unwrap().tracks[1].segments[0].frame_step,
        MotionFrameStep::Off {}
    );
}

#[test]
fn authoring_raw_preview_recompiles_on_fps_and_result_events_use_real_time() {
    use zipzi_animation::{MotionEditingPreview, MotionPreview};
    let mut b = session();
    let mut clip = b.motion(&id(3)).unwrap().clone();
    clip.tracks.truncate(1);
    b.replace_authored_motion(clip).unwrap();
    let mut c = candidate(&b);
    c.set_easing_preset(&b, &[reference(0, 0)], MotionEasingPreset::CubicOut)
        .unwrap();
    c.set_frame_step(
        &b,
        &(0..4).map(|i| reference(0, i)).collect::<Vec<_>>(),
        MotionFrameStep::from_frames(3).unwrap(),
    )
    .unwrap();
    let mut clip = c.into_clip();
    b.replace_authored_motion(clip.clone()).unwrap();
    let state = b.freeze().unwrap().state;
    let mut raw = MotionEditingPreview::new(&state);
    assert_eq!(
        raw.sample_clip(Some(&clip), 0.05).unwrap().parameters[&id(2)],
        0.
    );
    // Keep the exact same segment Arc; FPS alone invalidates the compiled cache.
    clip.fps = 60.;
    let value = raw.sample_clip(Some(&clip), 0.05).unwrap().parameters[&id(2)];
    assert!((value - 0.488).abs() < 0.00002);
    let mut b = state.builder();
    b.replace_authored_motion(clip).unwrap();
    let state = b.freeze().unwrap().state;
    let mut playback = MotionPreview::new(&state);
    playback.schedule_motion(&id(3), 0.).unwrap();
    playback.advance(0.).unwrap();
    let sampled = playback.advance(0.55).unwrap();
    assert!((sampled.time - 0.55).abs() < 0.000001);
    assert_eq!(sampled.fired_events.len(), 1);
    assert_eq!(sampled.fired_events[0].value, "cue");
    assert!(playback.advance(0.05).unwrap().fired_events.is_empty());
}

#[test]
fn authoring_time_scaling_preserves_source_range_and_does_not_scale_frame_count() {
    let mut s = session();
    let mut clip = s.motion(&id(3)).unwrap().clone();
    clip.meta_extensions.insert(
        "ZipziSourceKeyRange".into(),
        serde_json::json!({"start":-1.,"end":2.}),
    );
    Arc::make_mut(&mut clip.tracks[0].segments)[0].frame_step =
        MotionFrameStep::EveryFrames { frames: 3 };
    s.replace_authored_motion(clip).unwrap();
    let mut c = candidate(&s);
    c.scale_time(&s, 2.).unwrap();
    let range = c.clip().source_key_range().unwrap();
    assert_eq!((range.start, range.end), (-2., 4.));
    assert_eq!(
        c.clip().tracks[0].segments[0].frame_step,
        MotionFrameStep::EveryFrames { frames: 3 }
    );
}

#[test]
fn authoring_transfer_copies_complete_interval_settings_and_move_merges_the_source_left() {
    use zipzi_core::document::MotionEdit;
    let mut b = session();
    let mut clip = b.motion(&id(3)).unwrap().clone();
    clip.tracks[1].segments = Arc::new(vec![]);
    clip.tracks[1].initial.time = 0.8;
    b.replace_authored_motion(clip).unwrap();
    let mut c = candidate(&b);
    c.set_easing_preset(&b, &[reference(0, 0)], MotionEasingPreset::CubicOut)
        .unwrap();
    c.set_frame_step(
        &b,
        &[reference(0, 0)],
        MotionFrameStep::EveryFrames { frames: 3 },
    )
    .unwrap();
    b.replace_authored_motion(c.into_clip()).unwrap();
    let original = b.motion(&id(3)).unwrap().tracks[0].clone();
    let mut copied = b.clone();
    copied
        .edit_motion(MotionEdit::TransferKeys {
            ids: vec![id(100), id(101)],
            destination_track: id(11),
            time_offset: 0.1,
            copy: true,
        })
        .unwrap();
    let clip = copied.motion(&id(3)).unwrap();
    assert_eq!(clip.tracks[0], original);
    let target = &clip.tracks[1];
    assert_ne!(target.initial.id, original.initial.id);
    assert_eq!(target.initial.time, 0.1);
    assert_ne!(target.segments[0].end.id, original.segments[0].end.id);
    assert!(matches!(
        target.segments[0].curve,
        MotionCurve::Bezier {
            preset: Some(MotionEasingPreset::CubicOut),
            ..
        }
    ));
    assert_eq!(
        target.segments[0].frame_step,
        MotionFrameStep::EveryFrames { frames: 3 }
    );
    copied.validate_candidate().unwrap();
    b.edit_motion(MotionEdit::TransferKeys {
        ids: vec![id(101)],
        destination_track: id(11),
        time_offset: 0.4,
        copy: false,
    })
    .unwrap();
    let source = &b.motion(&id(3)).unwrap().tracks[0];
    assert_eq!(source.segments[0].end.id, id(102));
    assert!(matches!(
        source.segments[0].curve,
        MotionCurve::Bezier { preset: None, .. }
    ));
    assert_eq!(
        source.segments[0].frame_step,
        MotionFrameStep::EveryFrames { frames: 3 }
    );
    b.validate_candidate().unwrap();
}

#[test]
fn retiming_preset_endpoints_rejects_f32_collisions_atomically() {
    use zipzi_core::document::MotionEdit;
    for (start, scale) in [(0.7000001_f32, true), (0.25_f32, false)] {
        let mut b = session();
        let mut clip = b.motion(&id(3)).unwrap().clone();
        let track = &mut clip.tracks[0];
        track.initial.time = start;
        track.segments = Arc::new(vec![MotionSegment::new(
            key(101, start.next_up(), 1.),
            MotionCurve::Linear {},
        )]);
        b.replace_authored_motion(clip).unwrap();
        let mut c = candidate(&b);
        c.set_easing_preset(&b, &[reference(0, 0)], MotionEasingPreset::CubicOut)
            .unwrap();
        let before = c.clip().clone();
        if scale {
            assert_eq!(
                c.scale_time(&b, 0.75).unwrap_err().code,
                "MOTION_KEY_COLLISION"
            );
            assert_eq!(c.clip(), &before);
        } else {
            b.replace_authored_motion(before.clone()).unwrap();
            for copy in [false, true] {
                let mut attempt = b.clone();
                let error = attempt
                    .edit_motion(MotionEdit::TransferKeys {
                        ids: vec![id(100), id(101)],
                        destination_track: id(11),
                        time_offset: 0.5,
                        copy,
                    })
                    .unwrap_err();
                assert_eq!(error.code, "MOTION_KEY_COLLISION");
                assert_eq!(attempt.motion(&id(3)), Some(&before));
            }
        }
    }
}

#[test]
fn locked_interval_checks_include_its_start_and_already_matching_batch_targets() {
    let mut b = session();
    let mut c = candidate(&b);
    let step = MotionFrameStep::from_frames(3).unwrap();
    c.set_frame_step(&b, &[reference(0, 1)], step).unwrap();
    b.replace_authored_motion(c.into_clip()).unwrap();
    let mut flags = b.editor_state().clone();
    flags.objects.entry(id(102)).or_default().locked = true;
    b.set_editor_state(flags).unwrap();
    let before = b.motion(&id(3)).unwrap().clone();
    for point in [movement(0, 1, 0.3, 1.), movement(0, 1, 0.25, 0.5)] {
        let mut c = candidate(&b);
        c.move_keys(&b, &[point]).unwrap();
        let mut attempt = b.clone();
        assert_eq!(
            attempt
                .replace_authored_motion(c.into_clip())
                .unwrap_err()
                .code,
            "OBJECT_LOCKED"
        );
        assert_eq!(attempt.motion(&id(3)), Some(&before));
    }
    let mut c = candidate(&b);
    let err = c
        .set_frame_step(&b, &[reference(1, 0), reference(0, 1)], step)
        .unwrap_err();
    assert_eq!(err.code, "OBJECT_LOCKED");
    assert_eq!(c.clip(), &before);
}

#[test]
fn restricted_conversion_checks_unselected_short_intervals_before_publishing() {
    let mut b = bezier_session(true);
    let mut clip = b.motion(&id(3)).unwrap().clone();
    let track = &mut clip.tracks[0];
    track.initial.time = 0.5;
    let end = f32::from_bits(0.5_f32.to_bits() + 4);
    let segment = &mut Arc::make_mut(&mut track.segments)[0];
    segment.end.time = end;
    segment.curve = MotionCurve::Bezier {
        control1: MotionPoint {
            time: 0.5,
            value: 1.,
        },
        control2: MotionPoint {
            time: end,
            value: 1.,
        },
        preset: None,
    };
    b.replace_authored_motion(clip.clone()).unwrap();
    let mut c = candidate(&b);
    assert_eq!(
        c.set_easing_preset(&b, &[reference(1, 0)], MotionEasingPreset::Ease)
            .unwrap_err()
            .code,
        "MOTION_BEZIER_CONVERSION_PRECISION"
    );
    assert_eq!(c.clip(), &clip);
    // A target being explicitly reshaped need not preserve its previous controls.
    c.set_easing_preset(&b, &[reference(0, 0)], MotionEasingPreset::Ease)
        .unwrap();
    assert!(!c.clip().restricted_beziers);

    // With a single interior f32 time both rounded controls land at its midpoint.
    // The conservative error bound is large, but the sole interior sample is exact.
    let track = &mut clip.tracks[0];
    let middle = track.initial.time.next_up();
    let segment = &mut Arc::make_mut(&mut track.segments)[0];
    segment.end.time = middle.next_up();
    if let MotionCurve::Bezier { control2, .. } = &mut segment.curve {
        control2.time = segment.end.time;
    }
    b.replace_authored_motion(clip.clone()).unwrap();
    let mut c = candidate(&b);
    c.set_easing_preset(&b, &[reference(1, 0)], MotionEasingPreset::Ease)
        .unwrap();
    let sample = |clip: &MotionClip| {
        sample_motion_curve(
            &CompiledCurve::from_track(&clip.tracks[0], clip.fps),
            middle,
            clip.restricted_beziers,
        )
    };
    assert_eq!(sample(c.clip()), sample(&clip));
}

#[test]
fn retiming_transforms_mixed_selection_and_rebuilds_only_affected_curves() {
    let mut doc = session();
    let mut clip = doc.motion(&id(3)).unwrap().clone();
    clip.fps = 29.97;
    clip.events.extend([
        MotionEvent {
            id: id(21),
            time: 0.6875,
            value: "coincident".into(),
            extensions: Default::default(),
        },
        MotionEvent {
            id: id(22),
            time: 0.75,
            value: "move with key".into(),
            extensions: Default::default(),
        },
    ]);
    let segments = Arc::make_mut(&mut clip.tracks[0].segments);
    segments[0].curve = MotionCurve::Bezier {
        control1: MotionPoint {
            time: 0.0625,
            value: 0.7,
        },
        control2: MotionPoint {
            time: 0.1875,
            value: 0.9,
        },
        preset: None,
    };
    segments[1].curve = MotionCurve::AutoSmooth {};
    segments[1].frame_step = MotionFrameStep::EveryFrames { frames: 3 };
    segments[2].curve = MotionCurve::InverseStepped {};
    segments[3].curve = MotionCurve::Stepped {};
    doc.replace_authored_motion(clip).unwrap();
    let mut c = candidate(&doc);
    c.retime_selection(
        &doc,
        &[reference(0, 1), reference(0, 2), reference(0, 3)],
        &[id(20), id(22)],
        MotionRetimeFixedEnd::Latest,
        0.5,
    )
    .unwrap();
    let before = doc.motion(&id(3)).unwrap();
    let after = c.clip();
    assert_eq!(after.duration, before.duration);
    assert_eq!(after.fps, before.fps);
    assert_eq!(after.tracks[1], before.tracks[1]);
    assert_eq!(
        after.tracks[0]
            .keys()
            .map(|key| key.time)
            .collect::<Vec<_>>(),
        [0., 0.5, 0.625, 0.75, 1.]
    );
    assert_eq!(
        after.tracks[0].segments[0].curve,
        MotionCurve::Bezier {
            control1: MotionPoint {
                time: 0.125,
                value: 0.7
            },
            control2: MotionPoint {
                time: 0.375,
                value: 0.9
            },
            preset: None,
        }
    );
    assert_eq!(
        after.tracks[0].segments[1].curve,
        MotionCurve::AutoSmooth {}
    );
    assert_eq!(
        after.tracks[0].segments[1].frame_step,
        MotionFrameStep::EveryFrames { frames: 3 }
    );
    assert!(matches!(
        after.tracks[0].segments[2].curve,
        MotionCurve::InverseStepped {}
    ));
    assert!(matches!(
        after.tracks[0].segments[3].curve,
        MotionCurve::Stepped {}
    ));
    assert_eq!(
        after
            .events
            .iter()
            .map(|event| event.time)
            .collect::<Vec<_>>(),
        [0.625, 0.6875, 0.75]
    );

    let mut events = candidate(&doc);
    events
        .retime_selection(
            &doc,
            &[],
            &[id(20), id(22)],
            MotionRetimeFixedEnd::Earliest,
            0.6875,
        )
        .unwrap();
    assert_eq!(
        events
            .clip()
            .events
            .iter()
            .map(|e| (e.time, e.value.as_str()))
            .collect::<Vec<_>>(),
        [
            (0.5, "cue"),
            (0.6875, "coincident"),
            (0.6875, "move with key")
        ]
    );
    assert_eq!(events.clip().tracks, before.tracks);
}

#[test]
fn retiming_preserves_restricted_preset_and_noop_and_rejects_f32_collisions() {
    let mut doc = bezier_session(true);
    let mut initial = candidate(&doc);
    initial
        .set_easing_preset(&doc, &[reference(0, 0)], MotionEasingPreset::CubicOut)
        .unwrap();
    doc.replace_authored_motion(initial.into_clip()).unwrap();
    let mut c = candidate(&doc);
    let keys = [reference(0, 0), reference(0, 4)];
    c.retime_selection(&doc, &keys, &[], MotionRetimeFixedEnd::Earliest, 0.8)
        .unwrap();
    let track = &c.clip().tracks[0];
    let [control1, control2] = MotionEasingPreset::CubicOut
        .mapped_controls(track.initial.point(), track.segments[0].end().point())
        .unwrap();
    assert_eq!(
        track.segments[0].curve,
        MotionCurve::Bezier {
            control1,
            control2,
            preset: Some(MotionEasingPreset::CubicOut),
        }
    );
    let before = c.clip().clone();
    c.retime_selection(
        &doc,
        &keys,
        &[],
        MotionRetimeFixedEnd::Earliest,
        f64::from(0.8f32),
    )
    .unwrap();
    assert_eq!(*c.clip(), before);

    let mut c = candidate(&doc);
    let before = c.clip().clone();
    let retime = c.retime_selection(
        &doc,
        &[reference(1, 2), reference(1, 3), reference(1, 4)],
        &[],
        MotionRetimeFixedEnd::Earliest,
        f64::from(0.5f32.next_up()),
    );
    assert_eq!(retime.unwrap_err().code, "MOTION_KEY_COLLISION");
    assert_eq!(*c.clip(), before);
    assert_eq!(
        c.retime_selection(
            &doc,
            &[reference(1, 2)],
            &[id(20)],
            MotionRetimeFixedEnd::Earliest,
            0.8
        )
        .unwrap_err()
        .code,
        "MOTION_RETIME_SPAN",
    );
}

#[test]
fn retiming_rejects_protected_neighbor_and_event_atomically_through_the_command() {
    use crate::{AuthoringSession, ResourceView, Snapshot};
    use zipzi_core::document::ObjectEditorState;
    for (locked, events) in [(id(103), vec![]), (id(20), vec![id(20)])] {
        let mut doc = session();
        let mut flags = doc.editor_state().clone();
        flags.objects.insert(
            locked.clone(),
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        doc.set_editor_state(flags).unwrap();
        let session = AuthoringSession::new(
            Snapshot::new(doc.freeze().unwrap().state, ResourceView::default()).unwrap(),
        );
        let before = session.snapshot().content_id();
        let mut draft = session.begin_draft();
        let keys = if events.is_empty() {
            vec![reference(0, 1), reference(0, 2)]
        } else {
            vec![reference(0, 1)]
        };
        let status = draft
            .batch(draft.source(), |commands| {
                commands.edit_motion_curve(
                    &id(3),
                    MotionCurveEdit::RetimeSelection {
                        keys,
                        events,
                        fixed_end: MotionRetimeFixedEnd::Earliest,
                        moving_time: 0.625,
                    },
                )
            })
            .unwrap_err();
        assert_eq!(status.code, "OBJECT_LOCKED");
        assert!(status.to_string().contains(&locked));
        assert_eq!(session.snapshot().content_id(), before);
    }
}
