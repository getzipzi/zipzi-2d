//! Pure geometry helpers shared by picking and canvas interaction.
use crate::{CanvasPoint, ModelPoint};

pub fn bounds(points: impl IntoIterator<Item = ModelPoint>) -> Option<[f32; 4]> {
    points
        .into_iter()
        .filter(|p| p.x.is_finite() && p.y.is_finite())
        .fold(None, |b, p| {
            Some(match b {
                None => [p.x, p.y, p.x, p.y],
                Some([x0, y0, x1, y1]) => [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
            })
        })
}
pub fn distance2(a: CanvasPoint, b: CanvasPoint) -> f32 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}
pub fn segment_distance2(p: CanvasPoint, a: CanvasPoint, b: CanvasPoint) -> f32 {
    let length = distance2(a, b);
    let k = if length > 0. {
        ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / length
    } else {
        0.
    }
    .clamp(0., 1.);
    distance2(
        p,
        CanvasPoint {
            x: a.x + k * (b.x - a.x),
            y: a.y + k * (b.y - a.y),
        },
    )
}
pub(super) fn barycentric(p: ModelPoint, [a, b, c]: [ModelPoint; 3]) -> Option<[f32; 3]> {
    let cross = |x: ModelPoint, y: ModelPoint, z: ModelPoint| {
        (y.x - x.x) as f64 * (z.y - x.y) as f64 - (y.y - x.y) as f64 * (z.x - x.x) as f64
    };
    let area = cross(a, b, c);
    if area.abs() < 1e-12 {
        return None;
    }
    let weights = [
        cross(p, b, c) / area,
        cross(a, p, c) / area,
        cross(a, b, p) / area,
    ];
    weights
        .iter()
        .all(|w| *w >= -1e-7 && *w <= 1. + 1e-7)
        .then(|| weights.map(|w| w as f32))
}
