use super::*;
use std::sync::Arc;
use zipzi_animation::{
    motion_frame_step_boundary, sample_motion_curve, CompiledCurve, MotionPreview,
};
use zipzi_core::document::{
    MotionClip, MotionCurve, MotionEasingPreset, MotionEvent, MotionFrameStep, MotionKey,
    MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
};
use zipzi_project::{export_motion3_with_summary, preflight_motion3, MotionExportLimits};

fn key(time: f32, value: f32) -> MotionKey {
    MotionKey {
        id: uuid::Uuid::new_v4().to_string(),
        time,
        value,
    }
}

fn clip(initial: [f32; 2], segments: &[(f32, f32, MotionCurve, u32)], fps: f32) -> MotionClip {
    let track = MotionTrack {
        id: uuid::Uuid::new_v4().to_string(),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(initial[0], initial[1]),
        segments: Arc::new(
            segments
                .iter()
                .map(|(time, value, curve, frames)| MotionSegment {
                    end: key(*time, *value),
                    curve: curve.clone(),
                    frame_step: MotionFrameStep::from_frames(*frames).unwrap(),
                })
                .collect(),
        ),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    };
    MotionClip {
        id: MOTION.into(),
        name: "Curves".into(),
        duration: segments.last().map_or(1., |s| s.0).max(0.1),
        fps,
        looping: false,
        restricted_beziers: false,
        fade_in: Some(0.),
        fade_out: Some(0.),
        tracks: vec![track].into(),
        events: vec![MotionEvent {
            id: uuid::Uuid::new_v4().to_string(),
            time: 0.05,
            value: "cue".into(),
            extensions: Default::default(),
        }]
        .into(),
        track_order: vec![],
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    }
}

fn state(clip: MotionClip) -> DocumentState {
    prepare(&document(), |b| b.create_authored_motion(clip))
}

fn roundtrip_motion(source: &DocumentState) -> (DocumentState, zipzi_project::MotionExportSummary) {
    let old = source.motion(MOTION).unwrap().clone();
    let exported =
        export_motion3_with_summary(source, MOTION, &MotionExportLimits::default()).unwrap();
    let imported = import_motion3(&document(), MOTION, "Imported", &exported.json).unwrap();
    assert_eq!(source.motion(MOTION), Some(&old));
    let wire: Value = serde_json::from_str(&exported.json).unwrap();
    assert_eq!(
        wire["UserData"][0]["Time"].as_f64().unwrap() as f32,
        old.events[0].time
    );
    assert_eq!(
        wire["Meta"]["TotalSegmentCount"].as_u64().unwrap() as usize,
        exported.summary.output_segments
    );
    (imported.candidate, exported.summary)
}

fn assert_samples(source: &DocumentState, result: &DocumentState, mut times: Vec<f32>) {
    let a = source.motion(MOTION).unwrap();
    let b = result.motion(MOTION).unwrap();
    let first = CompiledCurve::from_track(&a.tracks[0], a.fps);
    let second = CompiledCurve::from_track(&b.tracks[0], b.fps);
    times.extend((0..=1000).map(|i| a.duration * i as f32 / 1000.));
    for time in times {
        let x = sample_motion_curve(&first, time, a.restricted_beziers);
        let y = sample_motion_curve(&second, time, b.restricted_beziers);
        assert!((x - y).abs() < 0.00003, "time {time}: {x} != {y}");
    }
}

#[test]
fn package_admission_aggregates_clips_before_encoding() {
    let definition = clip([0., 0.], &[(1., 1., MotionCurve::AutoSmooth {}, 2)], 30.);
    let first = state(definition.clone());
    let estimate = preflight_motion3(&first, MOTION, &MotionExportLimits::default()).unwrap();
    let mut second = definition;
    second.name = "Second".into();
    second.id = uuid::Uuid::new_v4().to_string();
    for track in &mut second.tracks {
        track.id = uuid::Uuid::new_v4().to_string();
        track.initial.id = uuid::Uuid::new_v4().to_string();
        for segment in Arc::make_mut(&mut track.segments) {
            segment.end.id = uuid::Uuid::new_v4().to_string();
        }
    }
    for event in &mut second.events {
        event.id = uuid::Uuid::new_v4().to_string();
    }
    let both = prepare(&first, |b| b.create_authored_motion(second));
    for limits in [
        MotionExportLimits {
            max_output_segments: estimate.estimated_output_segments,
            ..Default::default()
        },
        MotionExportLimits {
            max_workspace_bytes: estimate.estimated_workspace_bytes,
            ..Default::default()
        },
    ] {
        assert!(zipzi_project::preflight_motion_exports(&first, &limits).is_ok());
        assert_eq!(
            zipzi_project::preflight_motion_exports(&both, &limits)
                .unwrap_err()
                .code,
            "MOTION_EXPORT_LIMIT"
        );
    }
}

#[test]
fn legacy_nonmonotonic_time_is_baked_or_explicitly_rejected_when_cropped() {
    let mut definition = clip(
        [0., 0.],
        &[(
            1.,
            1.,
            MotionCurve::Bezier {
                control1: MotionPoint {
                    time: -0.5,
                    value: 0.8,
                },
                control2: MotionPoint {
                    time: 0.5,
                    value: 0.1,
                },
                preset: None,
            },
            3,
        )],
        30.,
    );
    let source = state(definition.clone());
    let (result, _) = roundtrip_motion(&source);
    assert_samples(
        &source,
        &result,
        vec![0., 0_f32.next_up(), 0.1_f32.next_down(), 0.1],
    );
    definition.duration = 0.5;
    definition.meta_extensions.insert(
        "ZipziSourceKeyRange".into(),
        serde_json::json!({"start":0.,"end":1.}),
    );
    Arc::make_mut(&mut definition.tracks[0].segments)[0].frame_step = MotionFrameStep::Off {};
    let source = state(definition);
    assert_eq!(
        export_motion3(&source, MOTION).unwrap_err().code,
        "NON_MONOTONIC_MOTION_BEZIER"
    );
}

#[test]
fn native_reopen_keeps_modes_sources_controls_counts_and_identities() {
    let start = MotionPoint {
        time: 0.2,
        value: 0.8,
    };
    let end = MotionPoint {
        time: 0.51,
        value: 0.1,
    };
    let [control1, control2] = MotionEasingPreset::CubicOut
        .mapped_controls(start, end)
        .unwrap();
    let source = state(clip(
        [0.025, 0.],
        &[
            (0.2, 0.8, MotionCurve::AutoSmooth {}, 2),
            (
                0.51,
                0.1,
                MotionCurve::Bezier {
                    control1,
                    control2,
                    preset: Some(MotionEasingPreset::CubicOut),
                },
                3,
            ),
            (0.7, 0.7, MotionCurve::Linear {}, 7),
            (0.8, 0.4, MotionCurve::Stepped {}, 3),
            (1., 0.9, MotionCurve::InverseStepped {}, 2),
        ],
        29.97,
    ));
    let before = source.motion(MOTION).unwrap().clone();
    let bytes = native_bytes(&source);
    let reopened = load_native(&bytes).unwrap();
    assert_eq!(reopened.motion(MOTION), Some(&before));
    assert_samples(&source, &reopened, vec![0.025, 0.2, 0.51, 1.]);
    let (baked, summary) = roundtrip_motion(&reopened);
    assert_samples(
        &reopened,
        &baked,
        vec![0.2_f32.next_down(), 0.2, 0.51, 0.8_f32.next_down(), 0.8, 1.],
    );
    assert_eq!(summary.auto_smooth_segments, 1);
    assert_eq!(summary.frame_step_intervals, 3);
    assert!(summary.output_segments > summary.source_segments);
    assert!(baked.motion(MOTION).unwrap().tracks[0]
        .segments
        .iter()
        .all(|s| s.frame_step == MotionFrameStep::Off {}
            && !matches!(
                s.curve,
                MotionCurve::AutoSmooth {}
                    | MotionCurve::Bezier {
                        preset: Some(_),
                        ..
                    }
            )));
}

#[test]
fn stepped_export_matches_before_at_and_after_every_tick_and_mixed_inverse_boundary() {
    for fps in [24., 29.97, 60.] {
        for frames in [1, 2, 3, 7, 100, u32::MAX] {
            for mode in [
                MotionCurve::Linear {},
                MotionCurve::AutoSmooth {},
                MotionCurve::Stepped {},
                MotionCurve::InverseStepped {},
            ] {
                let source = state(clip(
                    [0.125, 0.],
                    &[
                        (0.487, 0.7, mode, frames),
                        (0.713, 0.3, MotionCurve::InverseStepped {}, frames),
                        (1., 0.8, MotionCurve::Linear {}, frames),
                    ],
                    fps,
                ));
                let (result, _) = roundtrip_motion(&source);
                let mut times = vec![
                    0.,
                    0.125,
                    0.487_f32.next_down(),
                    0.487,
                    0.487_f32.next_up(),
                    0.713_f32.next_down(),
                    0.713,
                    1.,
                    1_f32.next_up(),
                ];
                if frames >= 2 {
                    for tick in 0..24 {
                        let time = motion_frame_step_boundary(0.125, fps, frames, tick).unwrap();
                        if time < 0.487 {
                            times.extend([time.next_down(), time, time.next_up()]);
                        }
                    }
                }
                assert_samples(&source, &result, times);
            }
        }
    }
}

#[test]
fn all_presets_export_the_actual_controls_and_frame_step_effect() {
    for preset in MotionEasingPreset::ALL {
        for frames in [1, 3] {
            let start = MotionPoint {
                time: 0.,
                value: 0.9,
            };
            let end = MotionPoint {
                time: 0.7,
                value: 0.1,
            };
            let [control1, control2] = preset.mapped_controls(start, end).unwrap();
            let source = state(clip(
                [0., 0.9],
                &[(
                    0.7,
                    0.1,
                    MotionCurve::Bezier {
                        control1,
                        control2,
                        preset: Some(preset),
                    },
                    frames,
                )],
                30.,
            ));
            let (result, summary) = roundtrip_motion(&source);
            assert_samples(
                &source,
                &result,
                vec![0., 0.1_f32.next_down(), 0.1, 0.1_f32.next_up(), 0.7],
            );
            assert_eq!(summary.frame_step_intervals, usize::from(frames > 1));
        }
    }
}

#[test]
fn cropped_frame_steps_keep_the_original_negative_subframe_origin_and_final_jump() {
    for frames in [2, 3, 7] {
        for restricted in [false, true] {
            let mut definition = clip(
                [-0.125, 0.],
                &[
                    (0.5, 0.6, MotionCurve::AutoSmooth {}, frames),
                    (0.75, 0.2, MotionCurve::Linear {}, frames),
                    (1.25, 0.9, MotionCurve::InverseStepped {}, frames),
                ],
                30.,
            );
            definition.duration = 0.75;
            definition.restricted_beziers = restricted;
            definition.meta_extensions.insert(
                "ZipziSourceKeyRange".into(),
                serde_json::json!({"start":-0.125,"end":1.25}),
            );
            let source = state(definition);
            let (result, summary) = roundtrip_motion(&source);
            assert_samples(
                &source,
                &result,
                vec![0., 0_f32.next_up(), 0.5, 0.75_f32.next_down(), 0.75],
            );
            assert!(summary.boundary_segments > 0);
            assert!(result.motion(MOTION).unwrap().tracks[0]
                .keys()
                .all(|k| k.time >= 0. && k.time <= 0.75));
        }
    }
}

#[test]
fn loop_tail_keeps_the_source_playback_seam_after_step_baking() {
    let mut definition = clip([0., 0.1], &[(0.5, 0.9, MotionCurve::AutoSmooth {}, 3)], 30.);
    definition.looping = true;
    let source = state(definition);
    let (result, summary) = roundtrip_motion(&source);
    assert_samples(
        &source,
        &result,
        vec![0.5_f32.next_down(), 0.5, 0.5_f32.next_up()],
    );
    assert_eq!(summary.boundary_segments, 1);
    let mut a = MotionPreview::new(&source);
    let mut b = MotionPreview::new(&result);
    a.schedule_motion(MOTION, 0.).unwrap();
    b.schedule_motion(MOTION, 0.).unwrap();
    a.advance(0.).unwrap();
    b.advance(0.).unwrap();
    for delta in [0.25, 0.25, 0.005, 0.01, 0.01, 0.01, 0.2, 0.3] {
        let x = a.advance(delta).unwrap().parameters[PARAM];
        let y = b.advance(delta).unwrap().parameters[PARAM];
        assert!((x - y).abs() < 0.00003, "loop: {x} != {y}");
    }
}

#[test]
fn export_preflight_rejects_unbounded_expansion_and_tiny_workspaces_before_baking() {
    let source = state(clip(
        [0., 0.],
        &[(1e9, 1., MotionCurve::Linear {}, 2)],
        120.,
    ));
    assert_eq!(
        preflight_motion3(&source, MOTION, &MotionExportLimits::default())
            .unwrap_err()
            .code,
        "MOTION_EXPORT_LIMIT"
    );
    assert_eq!(
        export_motion3(&source, MOTION).unwrap_err().code,
        "MOTION_EXPORT_LIMIT"
    );
    let small = state(clip(
        [0., 0.],
        &[(1., 1., MotionCurve::AutoSmooth {}, 3)],
        30.,
    ));
    assert_eq!(
        export_motion3_with_summary(
            &small,
            MOTION,
            &MotionExportLimits {
                max_output_segments: 100,
                max_workspace_bytes: 1
            }
        )
        .unwrap_err()
        .code,
        "MOTION_EXPORT_LIMIT"
    );
    assert_eq!(
        export_motion3_with_summary(
            &small,
            MOTION,
            &MotionExportLimits {
                max_output_segments: 2,
                max_workspace_bytes: 256 << 20
            }
        )
        .unwrap_err()
        .code,
        "MOTION_EXPORT_LIMIT"
    );
}

#[test]
fn cropped_loop_keeps_a_frame_step_tick_after_duration_or_reports_the_format_limit() {
    let mut definition = clip([0., 0.], &[(1., 1., MotionCurve::Linear {}, 3)], 30.);
    definition.looping = true;
    definition.meta_extensions.insert(
        "ZipziSourceKeyRange".into(),
        serde_json::json!({"start":0.,"end":1.}),
    );
    definition.duration = 0.49;
    let source = state(definition.clone());
    let (reimported, summary) = roundtrip_motion(&source);
    assert!(summary.output_segments <= summary.estimated_output_segments);
    let cycle = definition.duration + 1. / definition.fps;
    for t in [
        0.49,
        0.5_f32.next_down(),
        0.5,
        0.5_f32.next_up(),
        0.51,
        cycle,
        cycle.next_up(),
    ] {
        let sample = |s| {
            let mut preview = MotionPreview::new(s);
            preview.schedule_motion(MOTION, 0.).unwrap();
            preview.advance(0.).unwrap();
            preview.advance(t).unwrap().parameters[PARAM]
        };
        let value = sample(&source);
        assert_eq!(sample(&reimported), value, "at {t}");
        if t == 0.51 {
            assert_eq!(value, 0.5);
        }
    }
    // The tick at 0.5 is still played, but now exceeds the wire endpoint tolerance.
    definition.duration = 0.475;
    assert_eq!(
        export_motion3(&state(definition), MOTION).unwrap_err().code,
        "MOTION_LOOP_TAIL_UNREPRESENTABLE"
    );
}
