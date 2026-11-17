//! Independent bisection references keep the preset approximation budget
//! separate from runtime inverse-time solver accuracy.

use std::sync::Arc;
use zipzi_animation::{
    motion_easing_controls, sample_motion_curve, CompiledCurve, CompiledSegment,
};
use zipzi_contract::motion_curve::MotionEasingPreset;
use zipzi_core::document::MotionPoint;

#[derive(serde::Deserialize)]
struct Presets {
    presets: Vec<Preset>,
}

#[derive(serde::Deserialize)]
struct Preset {
    id: MotionEasingPreset,
    reference: Option<String>,
    max_error: Option<f64>,
    measured_max_error: Option<f64>,
}

fn bezier(coordinates: [f64; 4], t: f64) -> f64 {
    let [a, b, c, d] = coordinates;
    let u = 1.0 - t;
    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
}

fn reference_bezier(preset: MotionEasingPreset, time: f64) -> f64 {
    if time == 0.0 || time == 1.0 {
        return time;
    }
    let [[x1, y1], [x2, y2]] = preset.definition().controls.map(|p| p.map(f64::from));
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..48 {
        let middle = (low + high) / 2.0;
        if bezier([0.0, x1, x2, 1.0], middle) < time {
            low = middle;
        } else {
            high = middle;
        }
    }
    bezier([0.0, y1, y2, 1.0], (low + high) / 2.0)
}

fn analytic(reference: &str, time: f64) -> f64 {
    let (family, direction) = if let Some(family) = reference.strip_suffix("_in_out") {
        (family, 2)
    } else if let Some(family) = reference.strip_suffix("_out") {
        (family, 1)
    } else {
        (reference.strip_suffix("_in").unwrap(), 0)
    };
    let ease_in = |t: f64| match family {
        "sine" => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
        "quad" => t.powi(2),
        "cubic" => t.powi(3),
        "quart" => t.powi(4),
        "quint" => t.powi(5),
        "expo" => {
            if t == 0.0 {
                0.0
            } else {
                2_f64.powf(10.0 * t - 10.0)
            }
        }
        "circ" => 1.0 - (1.0 - t * t).sqrt(),
        _ => panic!("unknown analytic reference {reference}"),
    };
    match direction {
        0 => ease_in(time),
        1 => 1.0 - ease_in(1.0 - time),
        _ if time <= 0.5 => ease_in(time * 2.0) / 2.0,
        _ => 1.0 - ease_in((1.0 - time) * 2.0) / 2.0,
    }
}

fn reference_times() -> impl Iterator<Item = f64> {
    (0..=8192)
        .map(|i| f64::from(i) / 8192.0)
        .chain((1..=48).flat_map(|power| {
            let t = 2_f64.powi(-power);
            [t, 1.0 - t]
        }))
}

#[test]
fn presets_stay_within_the_recorded_analytic_approximation_budgets() {
    let table: Presets = serde_json::from_str(include_str!(
        "../../../fixtures/animation_curves/presets.json"
    ))
    .unwrap();
    for row in table.presets {
        let Some(reference) = row.reference else {
            // The four basic curves are defined by Bezier controls themselves.
            assert!(!row.id.definition().approximate);
            assert_eq!(row.max_error, None);
            continue;
        };
        let maximum = reference_times()
            .map(|time| (reference_bezier(row.id, time) - analytic(&reference, time)).abs())
            .fold(0.0, f64::max);
        eprintln!("{reference}: maximum normalized error {maximum:.9}");
        assert!(maximum <= row.max_error.unwrap(), "{reference}: {maximum}");
        assert!((maximum - row.measured_max_error.unwrap()).abs() < 0.00000001);
    }
}

#[test]
fn runtime_preset_samples_match_an_independent_inverse_time_reference() {
    let start = MotionPoint {
        time: 0.,
        value: 0.,
    };
    let end = MotionPoint {
        time: 1.,
        value: 1.,
    };
    for preset in MotionEasingPreset::ALL {
        let [control1, control2] = motion_easing_controls(preset, start, end).unwrap();
        let segment = CompiledSegment::Bezier {
            control1,
            control2,
            end,
        };
        let curve = CompiledCurve {
            initial: start,
            segments: Arc::new(vec![segment]),
        };
        let mut maximum = 0_f64;
        for i in 0..=8192 {
            let time = i as f32 / 8192.;
            let actual = sample_motion_curve(&curve, time, false);
            let expected = reference_bezier(preset, f64::from(time));
            assert!(actual.is_finite());
            maximum = maximum.max((f64::from(actual) - expected).abs());
        }
        eprintln!("{preset:?}: maximum runtime inverse-time error {maximum:.9}");
        assert!(maximum < 0.000001, "{preset:?}: {maximum}");
    }
}
