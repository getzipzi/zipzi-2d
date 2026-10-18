use crate::types::{Canvas, ModelSpace, Status, Vec2};

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

pub fn to_runtime_positions(canvas: ModelSpace, positions: &[Vec2]) -> Result<Vec<Vec2>, Status> {
    let mut result = Vec::with_capacity(positions.len());
    let ox = canvas.origin.x as f64;
    let oy = canvas.origin.y as f64;
    for p in positions {
        let qx = (p.x as f64 - ox) as f32;
        let qy = (oy - p.y as f64) as f32;
        if !qx.is_finite() || !qy.is_finite() {
            return Err(Status::error(
                "NON_FINITE",
                "Position conversion overflows float32",
            ));
        }
        result.push(Vec2::new(qx, qy));
    }
    Ok(result)
}

/// Derive a tight export rectangle from evaluated default-pose geometry.
/// Source dimensions never constrain it; format PPU and opaque flags are retained.
/// Bounds include hidden geometry, 10% or 50px padding per edge, and a minimum
/// 256px extent. Exporters translate root coordinates and apply format PPU at the file boundary.
pub fn finite_export_canvas(
    space: ModelSpace,
    source_canvas: Option<Canvas>,
    positions: impl IntoIterator<Item = Vec2>,
) -> Result<Canvas, Status> {
    if !space.is_valid() || source_canvas.is_some_and(|c| !c.is_valid()) {
        return Err(Status::error(
            "INVALID_MODEL_SPACE",
            "Invalid export coordinates",
        ));
    }
    let mut canvas = Canvas::with_flag(
        1.,
        1.,
        space.origin,
        source_canvas.map_or(1., |canvas| canvas.pixels_per_unit),
        (source_canvas.map_or(0, |c| c.flag) & !1) | u8::from(space.y_axis_down),
    );
    let mut bounds = None::<(f64, f64, f64, f64)>;
    for p in positions {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(Status::error("NON_FINITE", "Non-finite export geometry"));
        }
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        bounds = Some(match bounds {
            Some((xmin, xmax, ymin, ymax)) => (xmin.min(x), xmax.max(x), ymin.min(y), ymax.max(y)),
            None => (x, x, y, y),
        });
    }
    let (xmin, xmax, ymin, ymax) = bounds.unwrap_or((0., 0., 0., 0.));
    let width = xmax - xmin;
    let height = ymax - ymin;
    let padx = (width * 0.1).max(50.);
    let pady = (height * 0.1).max(50.);
    canvas.width = (width + 2. * padx).ceil().max(256.) as f32;
    canvas.height = (height + 2. * pady).ceil().max(256.) as f32;
    canvas.origin = Vec2::new((-xmin + padx).round() as f32, (ymax + pady).round() as f32);
    if !canvas.is_valid() || !(canvas.height - canvas.origin.y).is_finite() {
        return Err(Status::error(
            "NON_FINITE",
            "Export canvas exceeds float32 range",
        ));
    }
    Ok(canvas)
}

#[cfg(test)]
mod export_canvas_tests {
    use super::*;

    #[test]
    fn export_uses_tight_geometry_and_ignores_source_rectangle() {
        let fixed = Canvas::new(300., 200., Vec2::new(17., 29.), 2.5);
        let input = ModelSpace::new(Vec2::new(17., 29.));
        let result = finite_export_canvas(
            input,
            Some(fixed),
            [Vec2::new(-100., -40.), Vec2::new(100., 40.)],
        )
        .unwrap();
        assert_eq!(result.width, 300.);
        assert_eq!(result.height, 256.);
        assert_eq!(result.origin, Vec2::new(150., 90.));
        assert_eq!(result.pixels_per_unit, 2.5);
        assert_eq!(result.flag, u8::from(input.y_axis_down));
        assert_eq!(
            finite_export_canvas(
                input,
                Some(fixed),
                [Vec2::new(-100., -40.), Vec2::new(100., 40.)]
            )
            .unwrap(),
            result
        );
        // One-sided geometry stays tight; it does not include a mirrored empty region.
        let offset = finite_export_canvas(
            input,
            Some(fixed),
            [Vec2::new(20., -160.), Vec2::new(40., -120.)],
        )
        .unwrap();
        assert_eq!((offset.width, offset.height), (256., 256.));
        assert_eq!(offset.origin, Vec2::new(30., -70.));
    }

    #[test]
    fn model_bounds_remain_tight_and_respect_visibility() {
        let mut frame = crate::DrawableFrame::default();
        frame.drawables.push(crate::Drawable {
            positions: vec![Vec2::new(10., -40.), Vec2::new(30., -20.)],
            ..Default::default()
        });
        assert_eq!(frame.model_bounds(false), Some([10., -40., 30., -20.]));
        frame.drawables[0].visible = false;
        assert_eq!(frame.model_bounds(false), None);
        assert_eq!(frame.model_bounds(true), Some([10., -40., 30., -20.]));
        assert_eq!(crate::DrawableFrame::default().model_bounds(true), None);
    }

    #[test]
    fn empty_and_non_finite_export_geometry() {
        let empty = finite_export_canvas(ModelSpace::default(), None, []).unwrap();
        assert_eq!((empty.width, empty.height), (256., 256.));
        assert_eq!(empty.origin, Vec2::new(50., 50.));
        assert!(
            finite_export_canvas(ModelSpace::default(), None, [Vec2::new(f32::NAN, 0.)]).is_err()
        );
        let huge = ModelSpace::new(Vec2::default());
        assert!(finite_export_canvas(
            huge,
            None,
            [Vec2::new(-f32::MAX, 0.), Vec2::new(f32::MAX, 0.)]
        )
        .is_err());
    }
}
