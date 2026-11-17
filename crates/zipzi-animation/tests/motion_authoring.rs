use std::sync::Arc;
use zipzi_animation::{
    compile_motion_auto_smooth, motion_bezier_time_is_monotonic, motion_easing_controls,
    motion_frame_step_time, sample_motion_curve, sample_motion_segment, CompiledCurve,
    CompiledSegment, MotionCurveError,
};
use zipzi_contract::motion_curve::{MotionEasingPreset, MotionFrameStep};
use zipzi_core::document::MotionPoint;

fn point([time, value]: [f32; 2]) -> MotionPoint {
    MotionPoint { time, value }
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.00002,
        "{actual} != {expected}"
    );
}

#[test]
fn auto_smooth_matches_the_deterministic_examples() {
    #[derive(serde::Deserialize)]
    struct Examples {
        version: u32,
        cases: Vec<Case>,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        points: Vec<[f32; 2]>,
        controls: Vec<[[f32; 2]; 2]>,
        samples: Vec<[f32; 2]>,
    }
    let examples: Examples = serde_json::from_str(include_str!(
        "../../../fixtures/animation_curves/auto_smooth.json"
    ))
    .unwrap();
    assert_eq!(examples.version, 1);
    for case in examples.cases {
        let points: Vec<_> = case.points.into_iter().map(point).collect();
        let curve = compile_motion_auto_smooth(&points).unwrap();
        assert_eq!(curve.initial, points[0]);
        assert_eq!(curve.segments.len(), case.controls.len(), "{}", case.name);
        for (index, (segment, controls)) in curve.segments.iter().zip(case.controls).enumerate() {
            let CompiledSegment::Bezier {
                control1,
                control2,
                end,
            } = segment
            else {
                panic!()
            };
            for (actual, expected) in [control1, control2].into_iter().zip(controls.map(point)) {
                close(actual.time, expected.time);
                close(actual.value, expected.value);
            }
            assert_eq!(*end, points[index + 1]);
        }
        for [time, value] in case.samples {
            for restricted in [false, true] {
                close(sample_motion_curve(&curve, time, restricted), value);
            }
        }
    }
}

#[test]
fn auto_smooth_is_continuous_and_preserves_monotonicity_without_overshoot() {
    let sets = [
        vec![[0., 0.], [0.1, 2.], [7., 3.], [7.25, 4.]],
        vec![[0., 4.], [0.1, 3.], [7., 2.], [7.25, 0.]],
        vec![[0., 0.], [0.125, 1.], [3., -1.], [9., 0.]],
        vec![[0., 1.], [0.125, 1.], [3., 2.], [9., 2.]],
    ];
    for coordinates in sets {
        let points: Vec<_> = coordinates.into_iter().map(point).collect();
        let curve = compile_motion_auto_smooth(&points).unwrap();
        let mut start = curve.initial;
        let mut incoming_slope = 0.;
        for segment in curve.segments.iter() {
            let CompiledSegment::Bezier {
                control1,
                control2,
                end,
            } = segment
            else {
                panic!()
            };
            let outgoing_slope = (control1.value - start.value) / (control1.time - start.time);
            assert!((outgoing_slope - incoming_slope).abs() < 0.0001);
            incoming_slope = (end.value - control2.value) / (end.time - control2.time);
            let mut previous = start.value;
            for index in 0..=256 {
                let time = start.time + (end.time - start.time) * index as f32 / 256.;
                // Time is linear for generated Hermite controls. Restricted
                // sampling avoids testing the imported Cardano solver here.
                let value = sample_motion_segment(start, segment, time, true);
                let tolerance = 0.00001;
                assert!(value >= start.value.min(end.value) - tolerance);
                assert!(value <= start.value.max(end.value) + tolerance);
                if start.value <= end.value {
                    assert!(value >= previous - tolerance);
                } else {
                    assert!(value <= previous + tolerance);
                }
                previous = value;
            }
            start = *end;
        }
        assert_eq!(incoming_slope, 0.);
    }
}

#[test]
fn automatic_groups_end_at_manual_segments_and_recompile_without_history() {
    let a = point([0., 0.]);
    let b = point([1., 1.]);
    let c = point([2., -5.]);
    let d = point([4., 2.]);
    let manual = CompiledSegment::Bezier {
        control1: point([1.2, 8.]),
        control2: point([1.8, -6.]),
        end: c,
    };
    let left = compile_motion_auto_smooth(&[a, b]).unwrap();
    let right = compile_motion_auto_smooth(&[c, d]).unwrap();
    let curve = CompiledCurve::new(
        a,
        Arc::new(vec![left.segments[0], manual, right.segments[0]]),
    );
    let CompiledSegment::Bezier { control2, .. } = curve.segments[0] else {
        panic!()
    };
    assert_eq!(control2.value, b.value);
    let CompiledSegment::Bezier { control1, .. } = curve.segments[2] else {
        panic!()
    };
    assert_eq!(control1.value, c.value);
    assert_eq!(curve.segments[1], manual);
    let changed = compile_motion_auto_smooth(&[a, point([0.5, 3.]), b]).unwrap();
    assert_ne!(changed.segments[0], left.segments[0]);
    assert_eq!(
        compile_motion_auto_smooth(&[a, b]).unwrap().segments,
        left.segments
    );
}

#[test]
fn curve_compilation_rejects_invalid_points_and_supports_one_key_and_wide_values() {
    use MotionCurveError::*;
    assert!(matches!(compile_motion_auto_smooth(&[]), Err(EmptyCurve)));
    let single = compile_motion_auto_smooth(&[point([1., 2.])]).unwrap();
    assert_eq!(sample_motion_curve(&single, 100., false), 2.);
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for bad in [point([invalid, 0.]), point([0., invalid])] {
            assert!(matches!(
                compile_motion_auto_smooth(&[bad]),
                Err(NonFinitePoint)
            ));
            assert_eq!(
                motion_easing_controls(MotionEasingPreset::Ease, bad, point([1., 1.])),
                Err(NonFinitePoint)
            );
            assert_eq!(
                motion_easing_controls(MotionEasingPreset::Ease, point([0., 0.]), bad),
                Err(NonFinitePoint)
            );
        }
    }
    for end in [point([0., 1.]), point([-1., 1.])] {
        assert!(matches!(
            compile_motion_auto_smooth(&[point([0., 0.]), end]),
            Err(UnorderedKeys)
        ));
        assert_eq!(
            motion_easing_controls(MotionEasingPreset::Ease, point([0., 0.]), end),
            Err(UnorderedKeys)
        );
    }
    for preset in MotionEasingPreset::ALL {
        let controls =
            motion_easing_controls(preset, point([0., -f32::MAX]), point([1., f32::MAX])).unwrap();
        assert!(controls
            .into_iter()
            .all(|p| p.time.is_finite() && p.value.is_finite()));
    }
    let curve = compile_motion_auto_smooth(&[
        point([0., -f32::MAX]),
        point([1., 0.]),
        point([4., f32::MAX]),
    ])
    .unwrap();
    for segment in curve.segments.iter() {
        let CompiledSegment::Bezier {
            control1, control2, ..
        } = segment
        else {
            panic!()
        };
        assert!(control1.value.is_finite() && control2.value.is_finite());
    }
}

#[test]
fn presets_map_to_descending_and_flat_intervals_and_keep_time_monotone() {
    for preset in MotionEasingPreset::ALL {
        let normalized = preset.definition().controls;
        for [start, end] in [
            [point([2., 5.]), point([6., -3.])],
            [point([2., 5.]), point([6., 5.])],
        ] {
            let [control1, control2] = motion_easing_controls(preset, start, end).unwrap();
            assert!(motion_bezier_time_is_monotonic(
                start.time,
                control1.time,
                control2.time,
                end.time
            ));
            for (control, [x, y]) in [control1, control2].into_iter().zip(normalized) {
                close(control.time, 2. + x * 4.);
                close(control.value, start.value + y * (end.value - start.value));
            }
        }
    }
    let start = point([0., 0.]);
    let end = point([1., 1.]);
    let values = [
        MotionEasingPreset::CubicIn,
        MotionEasingPreset::CubicOut,
        MotionEasingPreset::CubicInOut,
    ]
    .map(|preset| {
        let [control1, control2] = motion_easing_controls(preset, start, end).unwrap();
        sample_motion_segment(
            start,
            &CompiledSegment::Bezier {
                control1,
                control2,
                end,
            },
            0.25,
            false,
        )
    });
    close(values[0], 0.015625);
    close(values[1], 0.578125);
    assert!(values[0] < values[2] && values[2] < 0.25 && 0.25 < values[1]);
}

/// Composition of the production selection, time mapping and base sampler.
/// Persistent per-segment settings are wired in the next implementation stage.
fn sample_stepped(curve: &CompiledCurve, time: f32, fps: f32, step: MotionFrameStep) -> f32 {
    let (start, segment) = curve.segment_at(time);
    segment.map_or(start.value, |segment| {
        let time = motion_frame_step_time(start.time, time, fps, step).unwrap();
        sample_motion_segment(start, segment, time, false)
    })
}

#[test]
fn cubic_out_on_threes_samples_the_base_curve_and_holds_the_tail() {
    let start = point([0., 0.]);
    let end = point([0.5, 1.]);
    let [control1, control2] =
        motion_easing_controls(MotionEasingPreset::CubicOut, start, end).unwrap();
    let curve = CompiledCurve::new(
        start,
        Arc::new(vec![CompiledSegment::Bezier {
            control1,
            control2,
            end,
        }]),
    );
    let step = MotionFrameStep::from_frames(3).unwrap();
    let expected = [
        0., 0., 0., 0.578125, 0.578125, 0.578125, 0.875, 0.875, 0.875, 0.984375, 0.984375,
        0.984375, 1.,
    ];
    for (frame, value) in expected.into_iter().enumerate().rev() {
        close(sample_stepped(&curve, frame as f32 / 24., 24., step), value);
    }
    assert_eq!(sample_stepped(&curve, 20., 24., step), 1.);
    let long = MotionFrameStep::from_frames(100).unwrap();
    assert_eq!(sample_stepped(&curve, 0.5_f32.next_down(), 24., long), 0.);
    assert_eq!(sample_stepped(&curve, 0.5, 24., long), 1.);
}

#[test]
fn frame_step_does_not_delay_non_frame_endpoints_or_inverse_step_boundaries() {
    let curve = CompiledCurve::new(
        point([0., 0.]),
        Arc::new(vec![
            CompiledSegment::Linear {
                end: point([0.26, 1.]),
            },
            CompiledSegment::InverseStepped {
                end: point([0.51, 3.]),
            },
            CompiledSegment::Stepped {
                end: point([0.77, 5.]),
            },
        ]),
    );
    for frames in [1, 2, 3, 17, u32::MAX] {
        let step = MotionFrameStep::from_frames(frames).unwrap();
        assert_eq!(sample_stepped(&curve, 0.26, 24., step), 3.);
        assert_eq!(sample_stepped(&curve, 0.26_f32.next_up(), 24., step), 3.);
        assert_eq!(sample_stepped(&curve, 0.51_f32.next_down(), 24., step), 3.);
        assert_eq!(sample_stepped(&curve, 0.51, 24., step), 3.);
        assert_eq!(sample_stepped(&curve, 0.77_f32.next_down(), 24., step), 3.);
        assert_eq!(sample_stepped(&curve, 0.77, 24., step), 5.);
        assert_eq!(sample_stepped(&curve, 0.77_f32.next_up(), 24., step), 5.);
    }
    close(
        sample_stepped(
            &curve,
            0.26_f32.next_down(),
            24.,
            MotionFrameStep::from_frames(3).unwrap(),
        ),
        0.25 / 0.26,
    );
}

#[test]
fn frame_step_distinguishes_every_tick_from_its_previous_float_without_epsilon() {
    for start in [0_f32, 0.1, 0.1234, 4096.25, -2.] {
        for fps in [24_f32, 30., 60., 23.976, 29.97] {
            for frames in [2, 3, 7, u32::MAX] {
                let step = MotionFrameStep::from_frames(frames).unwrap();
                let boundary = |i: u32| {
                    (f64::from(start) + f64::from(i) * f64::from(frames) / f64::from(fps)) as f32
                };
                for index in 1..128 {
                    let time = boundary(index);
                    let previous = boundary(index - 1);
                    assert_eq!(
                        motion_frame_step_time(start, time.next_down(), fps, step).unwrap(),
                        previous,
                        "before: {start} {fps} {frames} {index}"
                    );
                    assert_eq!(
                        motion_frame_step_time(start, time, fps, step).unwrap(),
                        time,
                        "at: {start} {fps} {frames} {index}"
                    );
                    assert_eq!(
                        motion_frame_step_time(start, time.next_up(), fps, step).unwrap(),
                        time,
                        "after: {start} {fps} {frames} {index}"
                    );
                }
            }
        }
    }
}

#[test]
fn frame_step_keeps_subframes_when_off_and_changes_duration_with_fps() {
    for step in [
        MotionFrameStep::Off {},
        MotionFrameStep::EveryFrames { frames: 1 },
    ] {
        for time in [0.09, 0.101, 1. / 29.97] {
            assert_eq!(motion_frame_step_time(0.1, time, 24., step).unwrap(), time);
        }
    }
    let threes = MotionFrameStep::from_frames(3).unwrap();
    assert_eq!(motion_frame_step_time(0., 0.09, 24., threes).unwrap(), 0.);
    assert_eq!(motion_frame_step_time(0., 0.09, 60., threes).unwrap(), 0.05);
    let twos = MotionFrameStep::from_frames(2).unwrap();
    assert_eq!(
        motion_frame_step_time(0.125, 0.24, 24., twos).unwrap(),
        (0.125_f64 + 2. / 24.) as f32
    );
    assert_eq!(
        motion_frame_step_time(0.125, 0.12, 24., twos).unwrap(),
        0.125
    );
    for fps in [0., -24., f32::NAN, f32::INFINITY] {
        assert_eq!(
            motion_frame_step_time(0., 0., fps, twos),
            Err(MotionCurveError::InvalidFps)
        );
    }
    assert_eq!(
        motion_frame_step_time(0., 0., 24., MotionFrameStep::EveryFrames { frames: 0 }),
        Err(MotionCurveError::InvalidFrameStep)
    );
    for (start, time) in [(f32::NAN, 0.), (0., f32::INFINITY)] {
        assert_eq!(
            motion_frame_step_time(start, time, 24., twos),
            Err(MotionCurveError::NonFinitePoint)
        );
    }
}

#[test]
fn auto_smooth_on_threes_keeps_its_subframe_origin_and_can_sample_in_any_order() {
    let curve = compile_motion_auto_smooth(&[point([0.125, 0.]), point([0.625, 1.])]).unwrap();
    let step = MotionFrameStep::from_frames(3).unwrap();
    let expected = [
        0., 0., 0., 0.15625, 0.15625, 0.15625, 0.5, 0.5, 0.5, 0.84375, 0.84375, 0.84375, 1.,
    ];
    for frame in [12, 7, 1, 10, 3, 8, 0, 11, 2, 4, 9, 5, 6] {
        let time = (0.125_f64 + frame as f64 / 24.) as f32;
        close(sample_stepped(&curve, time, 24., step), expected[frame]);
    }
    // Switching stepping off resumes the unchanged continuous base curve.
    close(sample_stepped(&curve, 0.3125, 24., step), 0.15625);
    close(
        sample_stepped(&curve, 0.3125, 24., MotionFrameStep::Off {}),
        0.31640625,
    );
    assert_eq!(curve.segments.len(), 1);
}

#[test]
fn finite_wide_curve_values_do_not_overflow_during_sampling() {
    let curve =
        compile_motion_auto_smooth(&[point([0., -f32::MAX]), point([1., f32::MAX])]).unwrap();
    for restricted in [false, true] {
        assert_eq!(sample_motion_curve(&curve, 0.5, restricted), 0.);
        for time in [0., 0.125, 0.25, 0.75, 0.875, 1.] {
            assert!(sample_motion_curve(&curve, time, restricted).is_finite());
        }
    }
    let curve = CompiledCurve::new(
        point([-f32::MAX, -f32::MAX]),
        Arc::new(vec![CompiledSegment::Linear {
            end: point([f32::MAX, f32::MAX]),
        }]),
    );
    assert_eq!(sample_motion_curve(&curve, 0., false), 0.);
}
