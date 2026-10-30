use crate::document::Document;
use crate::geometry::validate_positions;
use crate::types::{Status, Vec2};

pub use zipzi_contract::evaluation::{
    Drawable, DrawableFrame, EvaluatedDeformer, EvaluatedParameter, OffscreenFrame, PreviewValues,
    RenderCommand,
};

pub trait DrawableFrameBounds {
    fn model_bounds(&self, include_hidden: bool) -> Option<[f64; 4]>;
}
impl DrawableFrameBounds for DrawableFrame {
    /// Tight AABB of evaluated vertices in Y-up model units, without export padding.
    /// This is geometry, not an alpha/mask silhouette or a collision shape.
    /// To include invisible geometry, obtain the frame using
    /// `evaluate_frame_including_hidden` before passing `include_hidden = true`.
    fn model_bounds(&self, include_hidden: bool) -> Option<[f64; 4]> {
        let mut bounds = None::<[f64; 4]>;
        for d in &self.drawables {
            if !include_hidden && !(d.enabled && d.visible && d.opacity > 0.) {
                continue;
            }
            for p in &d.positions {
                let (x, y) = (f64::from(p.x), f64::from(p.y));
                bounds = Some(match bounds {
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                    None => [x, y, x, y],
                });
            }
        }
        bounds
    }
}

pub fn parent_positions<'a>(
    doc: &Document,
    parent: &str,
    positions: &'a [Vec2],
) -> Result<&'a [Vec2], Status> {
    doc.coordinate_space(parent)?;
    let s = validate_positions(positions);
    if !s.is_ok() {
        return Err(s);
    }
    Ok(positions)
}

pub fn parent_origin(
    doc: &Document,
    parent: &str,
    origin: crate::types::PreciseVec2,
) -> Result<Vec2, Status> {
    doc.coordinate_space(parent)?;
    let p = Vec2::new(origin.x as f32, origin.y as f32);
    let status = validate_positions(&[p]);
    if !status.is_ok() {
        return Err(status);
    }
    Ok(p)
}
