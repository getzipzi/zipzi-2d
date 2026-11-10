//! CPU geometry and mask layout shared by GPU backends.

use bytemuck::{Pod, Zeroable};
use zipzi_core::evaluation::{Drawable, DrawableFrame};
use zipzi_core::types::{Status, Vec2};

use crate::{Affine2, Bounds2, ViewportConfig};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub mask_point: [f32; 2],
}

pub fn vertices_for(drawable: &Drawable) -> Vec<Vertex> {
    let mut vertices = Vec::with_capacity(drawable.positions.len());
    append_vertices_for(drawable, &mut vertices);
    vertices
}

/// Append converted geometry directly into a packed GPU staging buffer.
pub fn append_vertices_for(drawable: &Drawable, out: &mut Vec<Vertex>) {
    out.extend(
        drawable
            .positions
            .iter()
            .zip(drawable.uvs.iter())
            .map(|(position, uv)| {
                let model = *position;
                Vertex {
                    position: [model.x, model.y],
                    uv: [uv.x, 1.0 - uv.y],
                    mask_point: [model.x, model.y],
                }
            }),
    );
}

pub fn transformed_vertices_for(drawable: &Drawable, viewport: ViewportConfig) -> Vec<Vertex> {
    let mut vertices = vertices_for(drawable);
    for vertex in &mut vertices {
        let point = viewport
            .transform
            .transform_point(Vec2::new(vertex.position[0], vertex.position[1]));
        vertex.position = [point.x, point.y];
    }
    vertices
}

pub fn triangle_indices(indices: &[u32]) -> Vec<u32> {
    let (triangles, _) = indices.as_chunks::<3>();
    triangles
        .iter()
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect()
}

pub fn quad_vertices(size: (u32, u32), transform: Affine2, mask_transform: Affine2) -> Vec<Vertex> {
    [
        (Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
        (Vec2::new(size.0 as f32, 0.0), Vec2::new(1.0, 0.0)),
        (Vec2::new(size.0 as f32, size.1 as f32), Vec2::new(1.0, 1.0)),
        (Vec2::new(0.0, size.1 as f32), Vec2::new(0.0, 1.0)),
    ]
    .into_iter()
    .map(|(position, uv)| {
        let mask_point = mask_transform.transform_point(position);
        let position = transform.transform_point(position);
        Vertex {
            position: [position.x, position.y],
            uv: [uv.x, uv.y],
            mask_point: [mask_point.x, mask_point.y],
        }
    })
    .collect()
}

pub fn quad_indices() -> Vec<u32> {
    triangle_indices(&[0, 1, 2, 0, 2, 3])
}

pub fn inverse_affine(transform: Affine2) -> Result<Affine2, Status> {
    let determinant = transform.determinant();
    if !transform.is_finite() || determinant.abs() < 1.0e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }
    let inverse_a = Vec2::new(transform.b.y / determinant, -transform.a.y / determinant);
    let inverse_b = Vec2::new(-transform.b.x / determinant, transform.a.x / determinant);
    let origin = Vec2::new(
        -(inverse_a.x * transform.origin.x + inverse_b.x * transform.origin.y),
        -(inverse_a.y * transform.origin.x + inverse_b.y * transform.origin.y),
    );
    Ok(Affine2 {
        a: inverse_a,
        b: inverse_b,
        origin,
    })
}

pub fn compose_affine(lhs: Affine2, rhs: Affine2) -> Affine2 {
    let origin = lhs.transform_point(rhs.origin);
    let lhs_origin = lhs.transform_point(Vec2::new(0.0, 0.0));
    let a_point = lhs.transform_point(rhs.a);
    let b_point = lhs.transform_point(rhs.b);
    Affine2 {
        a: Vec2::new(a_point.x - lhs_origin.x, a_point.y - lhs_origin.y),
        b: Vec2::new(b_point.x - lhs_origin.x, b_point.y - lhs_origin.y),
        origin,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MaskLayout {
    pub width: u32,
    pub height: u32,
    pub origin: Vec2,
    pub logical_size: Vec2,
    pub scale: f32,
}

pub fn mask_layout(
    frame: &DrawableFrame,
    source_ids: &[String],
    requested_scale: f64,
    max_dimension: u32,
) -> Result<MaskLayout, Status> {
    if !requested_scale.is_finite() || requested_scale <= 0.0 {
        return Err(Status::error(
            "INVALID_MASK_SCALE",
            "Mask scale must be finite and positive.",
        ));
    }
    let mut bounds: Option<(Vec2, Vec2)> = None;
    for source_id in source_ids {
        let drawable = frame
            .drawables
            .iter()
            .find(|drawable| drawable.id == *source_id)
            .ok_or_else(|| Status::error("INVALID_MASK", source_id))?;
        for position in &drawable.positions {
            let point = *position;
            bounds = Some(bounds.map_or((point, point), |(min, max)| {
                (
                    Vec2::new(min.x.min(point.x), min.y.min(point.y)),
                    Vec2::new(max.x.max(point.x), max.y.max(point.y)),
                )
            }));
        }
    }
    let (min, max) = bounds.unwrap_or((Vec2::default(), Vec2::default()));
    mask_layout_from_bounds(min, max, requested_scale, max_dimension)
}

/// Lower already computed canvas-pixel bounds using the same sampling policy
/// as `mask_layout`, without resolving source IDs or walking geometry again.
pub fn mask_layout_from_bounds(
    min: Vec2,
    max: Vec2,
    requested_scale: f64,
    max_dimension: u32,
) -> Result<MaskLayout, Status> {
    if !requested_scale.is_finite() || requested_scale <= 0.0 {
        return Err(Status::error(
            "INVALID_MASK_SCALE",
            "Mask scale must be finite and positive.",
        ));
    }
    let origin = Vec2::new(min.x - 4.0, min.y - 4.0);
    let size_x = (max.x - min.x + 8.0).ceil().max(1.0);
    let size_y = (max.y - min.y + 8.0).ceil().max(1.0);
    let scale = (requested_scale as f32).min(max_dimension as f32 / size_x.max(size_y));
    let width = (size_x * scale).ceil().max(1.0);
    let height = (size_y * scale).ceil().max(1.0);
    Ok(MaskLayout {
        width: width as u32,
        height: height as u32,
        origin,
        logical_size: Vec2::new(width / scale, height / scale),
        scale,
    })
}

/// Keep the best available density before sacrificing resolution. Color-only
/// exhaustion remains an error, even after every mask has reached one texel.
pub fn fit_mask_budget(
    viewport: ViewportConfig,
    device_limit: u32,
    mut budget: impl FnMut(ViewportConfig, MaskViewport) -> Result<u64, Status>,
) -> Result<(ViewportConfig, MaskViewport, u64), Status> {
    let mut mask_view = MaskViewport::new(viewport)?;
    mask_view.max_dimension = mask_view.max_dimension.min(device_limit);
    let rounded_scale = viewport.mask_scale.log2().ceil().exp2();
    let rounded_view = ViewportConfig {
        mask_scale: rounded_scale,
        ..viewport
    };
    if rounded_scale.is_finite() && rounded_scale > 0. {
        match budget(rounded_view, mask_view) {
            Ok(bytes) => return Ok((rounded_view, mask_view, bytes)),
            Err(error) if error.code == "OFFSCREEN_BUDGET_EXCEEDED" => {}
            Err(error) => return Err(error),
        }
    }
    loop {
        match budget(viewport, mask_view) {
            Ok(bytes) => return Ok((viewport, mask_view, bytes)),
            Err(error)
                if error.code == "OFFSCREEN_BUDGET_EXCEEDED" && mask_view.max_dimension > 1 =>
            {
                mask_view.max_dimension /= 2;
            }
            Err(error) => return Err(error),
        }
    }
}

// A fully offscreen mask has zero coverage, including for inverted consumers.
// Its 1x1 attachment is cleared without drawing the source geometry.
pub const EMPTY_MASK_LAYOUT: MaskLayout = MaskLayout {
    width: 1,
    height: 1,
    origin: Vec2 { x: 0., y: 0. },
    logical_size: Vec2 { x: 1., y: 1. },
    scale: 1.,
};

#[derive(Clone, Copy)]
pub struct MaskViewport {
    bounds: Bounds2,
    max_dimension: u32,
}

impl MaskViewport {
    pub fn max_dimension(self) -> u32 {
        self.max_dimension
    }

    pub fn new(viewport: ViewportConfig) -> Result<Self, Status> {
        let inverse = inverse_affine(viewport.transform)?;
        // A small screen-space apron and snapped bounds make small pans share
        // coverage. The layout helper retains its four canvas-pixel border.
        let margin = 64.;
        let min = Vec2::new(-margin, -margin);
        let max = Vec2::new(
            viewport.target_extent.x + margin,
            viewport.target_extent.y + margin,
        );
        let mut bounds = Bounds2::from_point(inverse.transform_point(min));
        for point in [Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)] {
            bounds = bounds.include(inverse.transform_point(point));
        }
        if !bounds.position.x.is_finite()
            || !bounds.position.y.is_finite()
            || !bounds.size.x.is_finite()
            || !bounds.size.y.is_finite()
        {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Visible mask bounds overflowed.",
            ));
        }
        let tile = Vec2::new(
            margin * (inverse.a.x.abs() + inverse.b.x.abs()),
            margin * (inverse.a.y.abs() + inverse.b.y.abs()),
        );
        if !tile.x.is_finite() || !tile.y.is_finite() || tile.x <= 0. || tile.y <= 0. {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Visible mask grid overflowed.",
            ));
        }
        let end = bounds.end();
        let origin = Vec2::new(
            (bounds.position.x / tile.x).floor() * tile.x,
            (bounds.position.y / tile.y).floor() * tile.y,
        );
        bounds = Bounds2 {
            position: origin,
            size: Vec2::new(
                (end.x / tile.x).ceil() * tile.x - origin.x,
                (end.y / tile.y).ceil() * tile.y - origin.y,
            ),
        };
        Ok(Self {
            bounds,
            max_dimension: 4096,
        })
    }

    pub fn layout(self, bounds: Bounds2, scale: f64) -> Result<Option<MaskLayout>, Status> {
        let min = Vec2::new(
            bounds.position.x.max(self.bounds.position.x),
            bounds.position.y.max(self.bounds.position.y),
        );
        let max = Vec2::new(
            bounds.end().x.min(self.bounds.end().x),
            bounds.end().y.min(self.bounds.end().y),
        );
        if max.x <= min.x || max.y <= min.y {
            return Ok(None);
        }
        // Cropping must not change the sampling phase. The screen-space crop
        // grid is generally fractional in mask texels (e.g. zoom 20 with a
        // rounded mask density of 32), which otherwise makes edges shimmer
        // whenever panning crosses a crop boundary. Align to the uncropped
        // source's grid before adding the filtering border.
        let layout = mask_layout_from_bounds(min, max, scale, self.max_dimension)?;
        if layout.scale != scale as f32 || !layout.scale.is_finite() || layout.scale <= 0. {
            // Resolution-limited layouts already change density with their
            // extent. Keep their dimensions within the selected budget cap.
            return Ok(Some(layout));
        }
        let align = |value: f32, origin: f32| {
            let density = f64::from(layout.scale);
            (f64::from(origin)
                + ((f64::from(value) - f64::from(origin)) * density).floor() / density)
                as f32
        };
        let min = Vec2::new(
            align(min.x, bounds.position.x),
            align(min.y, bounds.position.y),
        );
        mask_layout_from_bounds(min, max, scale, self.max_dimension).map(Some)
    }
}
