//! UI tuning in source pixels. Keep the SDK generator's defaults independent.
use zipzi_editor_core::adaptive_mesh::{FillParameters, MeshFillParameters};

pub(super) struct Controls {
    pub defaults: [f64; 11],
    pub steps: [f64; 11],
    pub fill: MeshFillParameters,
}

impl Controls {
    pub fn for_texture(width: u32, height: u32) -> Self {
        let length = width.max(height).max(1) as f64;
        let scale = length / 2048.;
        let steps = [
            nice_step(20. * scale).max(1.),
            nice_step(5. * scale).max(0.5),
            nice_step(0.5 * scale).max(0.1),
            nice_step(scale).max(0.1),
            nice_step(scale).max(1.),
            1.,
            0.1,
            0.1,
            0.1,
            5.,
            1.,
        ];
        let base = FillParameters {
            edge_ratio: 1.5,
            gradation: 1.,
            jitter: 0.3,
            angle: 0.,
            max_rows: 8,
        };
        let rounded = |value: f64, step: f64| (value / step).round() * step;
        Self {
            defaults: [
                // The contour sampler can subdivide curved spans up to 12x.
                // Starting at L/4 avoids the old six-pixel default on large art.
                rounded(length / 4., nice_step(length / 40.).max(1.)).clamp(6., 16384.),
                rounded(length / 16., steps[1]).clamp(6., 16384.),
                rounded(length / 1024., steps[2]).clamp(1., 256.),
                rounded(length / 128., steps[3]).clamp(2., 256.),
                0., // Wrapping can join separate islands; keep it opt-in.
                8.,
                base.edge_ratio,
                base.gradation,
                base.jitter,
                base.angle,
                base.max_rows as f64,
            ],
            steps,
            fill: MeshFillParameters {
                poisson: base,
                quadtree: base,
                fractal: FillParameters {
                    gradation: 2.,
                    ..base
                },
                paving: base,
            },
        }
    }
}

/// Familiar 1/2/5 steps scale with image size rather than the entire input range.
fn nice_step(value: f64) -> f64 {
    let magnitude = 10_f64.powf(value.log10().floor());
    let leading = value / magnitude;
    let digit = if leading < 1.5 {
        1.
    } else if leading < 3.5 {
        2.
    } else if leading < 7.5 {
        5.
    } else {
        10.
    };
    digit * magnitude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_scale_for_small_large_and_portrait_art() {
        let standard = Controls::for_texture(2048, 1024);
        assert_eq!(
            standard.defaults,
            [500., 130., 2., 16., 0., 8., 1.5, 1., 0.3, 0., 8.]
        );
        assert_eq!(
            standard.steps,
            [20., 5., 0.5, 1., 1., 1., 0.1, 0.1, 0.1, 5., 1.]
        );
        let large = Controls::for_texture(4096, 2048);
        assert_eq!(large.defaults[0], 1000.);
        assert_eq!(large.steps[0], 50.);
        assert_eq!(large.defaults, Controls::for_texture(2048, 4096).defaults);
        let tiny = Controls::for_texture(16, 32);
        assert_eq!(tiny.defaults[1], 6.);
        assert_eq!(tiny.defaults[2], 1.);
        assert_eq!(tiny.defaults[3], 2.);
    }

    #[test]
    fn all_sizes_have_valid_parameters_and_single_decimal_steps() {
        for length in [1, 32, 96, 256, 1024, 2048, 4096, 8192, 16384] {
            let controls = Controls::for_texture(length, length / 2 + 1);
            let mut settings = zipzi_editor_core::MeshSettings::default();
            settings.edge_spacing = controls.defaults[0];
            settings.interior_spacing = controls.defaults[1];
            settings.outer_margin = controls.defaults[2];
            settings.edge_width = controls.defaults[3];
            settings.fill_parameters = controls.fill;
            settings.validate().unwrap();
            for step in controls.steps {
                assert!(step >= 0.1);
                assert!((step * 10. - (step * 10.).round()).abs() < 1e-8);
            }
        }
    }
}
