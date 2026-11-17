use zipzi_animation::{sample_motion_curve, CompiledCurve};
use zipzi_core::{state::DocumentState, Canvas, Vec2};
fn decode_motion3(
    source: &str,
) -> Result<zipzi_core::document::MotionClip, Box<dyn std::error::Error>> {
    let document = DocumentState::new(
        "00000000-0000-4000-8000-000000000001",
        Canvas::new(100.0, 100.0, Vec2::default(), 10.0).into(),
    )
    .unwrap();
    let id = "00000000-0000-4000-8000-000000000002";
    let imported = zipzi_project::import_motion3(&document, id, "Sample", source).unwrap();
    Ok(imported.candidate.motion(id).unwrap().clone())
}

const RESTRICTED: &str =
    include_str!("../../../fixtures/animation_cpu/bezier_restricted.motion3.json");
const UNRESTRICTED: &str =
    include_str!("../../../fixtures/animation_cpu/bezier_unrestricted.motion3.json");
const TYPED: &str = include_str!("../../../fixtures/animation_cpu/typed.motion3.json");

#[test]
fn beziers_match_official_framework_cpu_samples() {
    let cases = [
        (
            RESTRICTED,
            [
                0.0,
                0.223_437_5,
                0.325,
                0.351_562_5,
                0.35,
                0.367_187_5,
                0.45,
                0.645_312_5,
                1.0,
            ],
        ),
        (
            UNRESTRICTED,
            [
                0.000_000_710_705_25,
                0.257_220_95,
                0.333_164_3,
                0.351_470_44,
                0.350_415_3,
                0.354_238_87,
                0.389_742_5,
                0.509_106_2,
                1.0,
            ],
        ),
    ];
    for (source, expected) in cases {
        let motion = decode_motion3(source).unwrap();
        for (index, wanted) in expected.into_iter().enumerate() {
            let time = index as f32 * 0.125;
            let value = sample_motion_curve(
                &CompiledCurve::from(&motion.tracks[0]),
                time,
                motion.restricted_beziers,
            );
            assert!(
                (value - wanted).abs() < 0.00002,
                "restricted={} time={time}: {value} != {wanted}",
                motion.restricted_beziers
            );
        }
    }
}

#[test]
fn linear_stepped_and_inverse_stepped_use_framework_boundaries() {
    let motion = decode_motion3(TYPED).unwrap();
    let compiled = CompiledCurve::from(&motion.tracks[0]);
    let curve = &compiled;
    assert!((sample_motion_curve(curve, 0.125, false) - 0.5).abs() < 0.00001);
    assert!((sample_motion_curve(curve, 0.6, false) - 0.5).abs() < 0.00001);
    assert!((sample_motion_curve(curve, 0.75, false) - 0.9).abs() < 0.00001);
    assert!((sample_motion_curve(curve, 1.0, false) - 0.9).abs() < 0.00001);
}

#[test]
fn mixed_segment_boundaries_choose_the_outgoing_segment_and_hold_the_tail() {
    use zipzi_animation::CompiledSegment;
    use zipzi_core::document::MotionPoint;
    let point = |time, value| MotionPoint { time, value };
    let curve = CompiledCurve {
        initial: point(0., 0.),
        segments: std::sync::Arc::new(vec![
            CompiledSegment::Linear { end: point(1., 1.) },
            CompiledSegment::Stepped { end: point(2., 2.) },
            CompiledSegment::InverseStepped { end: point(3., 3.) },
            CompiledSegment::Linear { end: point(4., 4.) },
        ]),
    };
    for restricted in [false, true] {
        for (time, expected) in [
            (-1., 0.),
            (0., 0.),
            (0.5, 0.5),
            (1., 1.),
            (2_f32.next_down(), 1.),
            (2., 3.),
            (2_f32.next_up(), 3.),
            (3., 3.),
            (3.5, 3.5),
            (4., 4.),
            (10., 4.),
        ] {
            assert_eq!(
                sample_motion_curve(&curve, time, restricted),
                expected,
                "time={time}"
            );
        }
    }
    let single = CompiledCurve {
        initial: point(0.25, 0.75),
        segments: Default::default(),
    };
    assert_eq!(sample_motion_curve(&single, 0., false), 0.75);
    assert_eq!(sample_motion_curve(&single, 10., false), 0.75);
}

#[test]
fn monotone_inverse_time_handles_crossed_controls_flat_derivatives_and_absolute_times() {
    use zipzi_animation::motion_bezier_parameter;
    use zipzi_core::document::MotionPoint;
    for (start, end) in [
        (0., 1.),
        (0.25, 0.26),
        (1024., 1024.001),
        (1e9_f32, 1e9_f32.next_up()),
        (1e-30, 4e-30),
    ] {
        let span = f64::from(end) - f64::from(start);
        let map = |x: f64| (f64::from(start) + x * span) as f32;
        for [a, b] in [
            [0., 0.],
            [0., 1.],
            [1., 1.],
            [1., 0.],
            [0.65, 0.35],
            [0.12, 0.39],
            [0.55, 1.],
        ] {
            let (first, second) = (map(a), map(b));
            let a = (f64::from(first) - f64::from(start)) / span;
            let b = (f64::from(second) - f64::from(start)) / span;
            let point = |time| MotionPoint { time, value: 0. };
            for i in 0..=64 {
                let time = map(f64::from(i) / 64.);
                let target = (f64::from(time) - f64::from(start)) / span;
                let (mut low, mut high) = (0., 1.);
                for _ in 0..48 {
                    let t = (low + high) / 2.;
                    let u = 1. - t;
                    let x = 3. * u * u * t * a + 3. * u * t * t * b + t * t * t;
                    if x < target {
                        low = t;
                    } else {
                        high = t;
                    }
                }
                let expected = if target == 0. || target == 1. {
                    target
                } else if a == 1. && b == 0. && target == 0.5 {
                    0.5
                } else {
                    (low + high) / 2.
                };
                let actual = motion_bezier_parameter(
                    point(start),
                    point(first),
                    point(second),
                    point(end),
                    time,
                    false,
                );
                assert!(
                    (f64::from(actual) - expected).abs() < 0.000003,
                    "{start}..{end} controls {first} {second} at {time}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn non_monotone_time_retains_the_framework_cardano_root_choice() {
    use zipzi_animation::{motion_bezier_parameter, motion_bezier_time_is_monotonic};
    use zipzi_core::document::MotionPoint;
    let point = |time| MotionPoint { time, value: 0. };
    // x(u) = 7u^3 - 10.5u^2 + 4.5u has three roots at x=0.5.
    // MotionBehavior V2 chooses the last one, (1 + sqrt(3/7)) / 2.
    assert!(!motion_bezier_time_is_monotonic(0., 1.5, -0.5, 1.));
    let actual = motion_bezier_parameter(point(0.), point(1.5), point(-0.5), point(1.), 0.5, false);
    assert!((actual - 0.82732683).abs() < 0.000001);
    assert_eq!(
        motion_bezier_parameter(point(0.), point(1.5), point(-0.5), point(1.), 0.5, true),
        0.5
    );
}
