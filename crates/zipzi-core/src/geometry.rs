use crate::types::{Canvas, Status, Vec2};

mod meshing;
pub use meshing::validate_mesh_meshing;

pub fn validate_positions(positions: &[Vec2]) -> Status {
    for p in positions {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Status::error("NON_FINITE", "Coordinates must be finite float32 values.");
        }
    }
    Status::ok()
}

pub fn validate_render_mesh(positions: &[Vec2], uvs: &[Vec2], indices: &[u32]) -> Status {
    if positions.len() > (i32::MAX as usize) || positions.len() != uvs.len() {
        return Status::error(
            "INVALID_LENGTH",
            "A mesh needs matching positions and UVs within 32-bit limits.",
        );
    }
    let s = validate_positions(positions);
    if !s.is_ok() {
        return s;
    }
    let s = validate_positions(uvs);
    if !s.is_ok() {
        return s;
    }
    // Zero-triangle mesh: indices are empty; valid geometry without rasterized triangles
    if indices.is_empty() {
        return Status::ok();
    }
    if !indices.len().is_multiple_of(3) || indices.len() > (i32::MAX as usize) {
        return Status::error(
            "INVALID_LENGTH",
            "Triangle indices must be a multiple of three.",
        );
    }
    if positions.len() < 3 {
        return Status::error(
            "INVALID_LENGTH",
            "A mesh with triangles needs at least three vertices.",
        );
    }
    for &chunk in indices.as_chunks::<3>().0 {
        let (a, b, c) = (chunk[0] as usize, chunk[1] as usize, chunk[2] as usize);
        if a >= positions.len() || b >= positions.len() || c >= positions.len() {
            return Status::error(
                "INVALID_INDEX",
                "Triangle index is outside the vertex array.",
            );
        }
        if a == b || b == c || a == c {
            return Status::error(
                "REPEATED_VERTEX",
                "A triangle must reference three distinct vertices.",
            );
        }
    }
    Status::ok()
}

pub fn is_renderable_mesh(positions: &[Vec2], indices: &[u32]) -> bool {
    !indices.is_empty() && positions.len() >= 3 && indices.len().is_multiple_of(3)
}

/// A finite format rectangle derived from model-space geometry and output policy.
/// Source dimensions and orientation never participate in this calculation.
pub fn finite_export_canvas(
    settings: crate::ExportSettings,
    positions: impl IntoIterator<Item = Vec2>,
) -> Result<Canvas, Status> {
    if !settings.is_valid() {
        return Err(Status::error(
            "INVALID_EXPORT_SETTINGS",
            "Invalid export policy",
        ));
    }
    let mut bounds = None::<[f64; 4]>;
    for p in positions {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(Status::error("NON_FINITE", "Non-finite export geometry"));
        }
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        bounds = Some(match bounds {
            Some([l, b, r, t]) => [l.min(x), b.min(y), r.max(x), t.max(y)],
            None => [x, y, x, y],
        });
    }
    let [mut l, mut b, mut r, mut t] =
        settings
            .bounds
            .map(|v| v.map(f64::from))
            .unwrap_or_else(|| {
                let [l, b, r, t] = bounds.unwrap_or([0.; 4]);
                let px = (f64::from(((r - l) as f32 * settings.padding_fraction).ceil()))
                    .max(f64::from(settings.padding_pixels));
                let py = (f64::from(((t - b) as f32 * settings.padding_fraction).ceil()))
                    .max(f64::from(settings.padding_pixels));
                let w = (r - l + 2. * px)
                    .max(f64::from(settings.minimum_extent))
                    .max(1.);
                let h = (t - b + 2. * py)
                    .max(f64::from(settings.minimum_extent))
                    .max(1.);
                let (cx, cy) = ((l + r) * 0.5, (b + t) * 0.5);
                [cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5]
            });
    // Snap outwards so rounding never clips content or requested padding.
    l = l.floor();
    b = b.floor();
    r = r.ceil();
    t = t.ceil();
    let canvas = Canvas::with_flag(
        (r - l) as f32,
        (t - b) as f32,
        Vec2::new(-l as f32, t as f32),
        settings.pixels_per_unit,
        1,
    );
    if !canvas.is_valid() || !(canvas.height - canvas.origin.y).is_finite() {
        return Err(Status::error(
            "NON_FINITE",
            "Export canvas exceeds float32 range",
        ));
    }
    Ok(canvas)
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use crate::{DrawableFrameBounds, ExportSettings};
    #[test]
    fn auto_bounds_are_tight_padded_and_independent_of_ppu() {
        let points = [Vec2::new(-100., 0.), Vec2::new(100., 600.)];
        let a = finite_export_canvas(ExportSettings::default(), points).unwrap();
        let b = finite_export_canvas(
            ExportSettings {
                pixels_per_unit: 100.,
                ..Default::default()
            },
            points,
        )
        .unwrap();
        assert_eq!((a.width, a.height, a.origin), (b.width, b.height, b.origin));
        assert_eq!((a.width, a.height), (300., 720.));
        assert_eq!(a.origin, Vec2::new(150., 660.));
    }
    #[test]
    fn specified_bounds_keep_a_stable_frame_without_padding() {
        let s = ExportSettings {
            bounds: Some([-100., 0., 100., 600.]),
            ..Default::default()
        };
        let a = finite_export_canvas(s, [Vec2::new(2000., 3000.)]).unwrap();
        assert_eq!(
            (a.width, a.height, a.origin),
            (200., 600., Vec2::new(100., 600.))
        );
    }
    #[test]
    fn empty_and_invalid_export_geometry() {
        assert_eq!(
            finite_export_canvas(ExportSettings::default(), [])
                .unwrap()
                .width,
            256.
        );
        assert!(
            finite_export_canvas(ExportSettings::default(), [Vec2::new(f32::NAN, 0.)]).is_err()
        );
        assert!(finite_export_canvas(
            ExportSettings::default(),
            [Vec2::new(-f32::MAX, 0.), Vec2::new(f32::MAX, 0.)]
        )
        .is_err());
        assert!(finite_export_canvas(
            ExportSettings {
                pixels_per_unit: 0.,
                ..Default::default()
            },
            []
        )
        .is_err());
    }
    #[test]
    fn model_bounds_remain_tight_and_respect_visibility() {
        let mut frame = crate::DrawableFrame::default();
        frame.drawables.push(crate::Drawable {
            positions: vec![Vec2::new(-4., 2.), Vec2::new(8., 9.)],
            ..Default::default()
        });
        assert_eq!(frame.model_bounds(false), Some([-4., 2., 8., 9.]));
        frame.drawables[0].visible = false;
        assert_eq!(frame.model_bounds(false), None);
        assert_eq!(frame.model_bounds(true), Some([-4., 2., 8., 9.]));
    }
}

/// Sample a stored vertex channel with nonnegative, normalized weights. All
/// authoring shape transfers use f64 intermediates, including delta channels.
pub fn sample_vertex_channel(values: &[Vec2], weights: &[(usize, f64)]) -> Result<Vec2, Status> {
    if weights.is_empty()
        || weights
            .iter()
            .any(|&(i, w)| i >= values.len() || !w.is_finite() || w < 0.)
        || (weights.iter().map(|v| v.1).sum::<f64>() - 1.).abs() > 1e-5
    {
        return Err(Status::error(
            "INVALID_VERTEX_SOURCE",
            "Expected normalized nonnegative vertex weights",
        ));
    }
    let (mut x, mut y) = (0., 0.);
    for &(i, w) in weights {
        x += f64::from(values[i].x) * w;
        y += f64::from(values[i].y) * w;
    }
    let p = Vec2::new(x as f32, y as f32);
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err(Status::error(
            "NON_FINITE",
            "Sampled channel exceeds finite float32 coordinates",
        ));
    }
    Ok(p)
}
