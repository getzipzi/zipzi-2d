//! Shared geometry for painting, hit testing and pointer clamping.
use super::{Surface, color::Hsv};
use gpui_kit::{App, RenderImage, Window};
use std::{f32::consts::TAU, sync::Arc};

pub(super) const SIDE: f32 = 164.;
pub(super) const OUTER: f32 = 0.485;
pub(super) const INNER: f32 = 0.395;
const TRIANGLE_RADIUS: f32 = 0.365;

pub(super) type Position = [f32; 2];

pub(super) fn hue_position(hue: f32) -> Position {
    let (sin, cos) = (hue * TAU).sin_cos();
    let radius = (OUTER + INNER) / 2.;
    [0.5 + radius * cos, 0.5 + radius * sin]
}

pub(super) fn hue_at([x, y]: Position) -> f32 {
    (y - 0.5).atan2(x - 0.5).rem_euclid(TAU) / TAU
}

pub(super) fn hit(position: Position, hue: f32) -> Option<Surface> {
    let radius = (position[0] - 0.5).hypot(position[1] - 0.5);
    if (INNER..=OUTER).contains(&radius) {
        Some(Surface::Hue)
    } else if Triangle::new(hue)
        .weights(position)
        .into_iter()
        .all(|w| w >= -0.000001)
    {
        Some(Surface::Sv)
    } else {
        None
    }
}

#[derive(Clone, Copy)]
pub(super) struct Triangle {
    /// Pure hue, white, black, in normalized wheel coordinates.
    pub vertices: [Position; 3],
}

impl Triangle {
    pub fn new(hue: f32) -> Self {
        Self {
            vertices: std::array::from_fn(|i| {
                let (sin, cos) = ((hue + i as f32 / 3.) * TAU).sin_cos();
                [0.5 + cos * TRIANGLE_RADIUS, 0.5 + sin * TRIANGLE_RADIUS]
            }),
        }
    }

    pub fn weights(self, [x, y]: Position) -> [f32; 3] {
        let [a, b, c] = self.vertices;
        let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        let hue = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / denominator;
        let white = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / denominator;
        [hue, white, 1. - hue - white]
    }

    pub fn position(self, saturation: f32, value: f32) -> Position {
        let weights = [saturation * value, (1. - saturation) * value, 1. - value];
        std::array::from_fn(|axis| (0..3).map(|i| self.vertices[i][axis] * weights[i]).sum())
    }

    pub fn closest(self, position: Position) -> Position {
        if self.weights(position).into_iter().all(|w| w >= 0.) {
            return position;
        }
        let mut nearest = self.vertices[0];
        let mut distance = f32::INFINITY;
        for i in 0..3 {
            let a = self.vertices[i];
            let b = self.vertices[(i + 1) % 3];
            let edge = [b[0] - a[0], b[1] - a[1]];
            let t = (((position[0] - a[0]) * edge[0] + (position[1] - a[1]) * edge[1])
                / (edge[0] * edge[0] + edge[1] * edge[1]))
                .clamp(0., 1.);
            let point = [a[0] + t * edge[0], a[1] + t * edge[1]];
            let d = (point[0] - position[0]).powi(2) + (point[1] - position[1]).powi(2);
            if d < distance {
                nearest = point;
                distance = d;
            }
        }
        nearest
    }

    pub fn sv(self, position: Position, previous_saturation: f32) -> (f32, f32) {
        let weights = self
            .weights(self.closest(position))
            .map(|w| w.clamp(0., 1.));
        let value = (weights[0] + weights[1]).clamp(0., 1.);
        let saturation = if value <= 0.000001 {
            previous_saturation
        } else {
            (weights[0] / value).clamp(0., 1.)
        };
        (saturation, if value <= 0.000001 { 0. } else { value })
    }

    pub fn bounds(self) -> (Position, Position) {
        (
            std::array::from_fn(|axis| {
                self.vertices
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::INFINITY, f32::min)
            }),
            std::array::from_fn(|axis| {
                self.vertices
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::NEG_INFINITY, f32::max)
            }),
        )
    }
}

#[derive(Default)]
pub(super) struct WheelCache {
    ring: Option<(u32, Arc<RenderImage>)>,
    triangle: Option<((u32, u32), Arc<RenderImage>)>,
}

impl WheelCache {
    pub fn images(
        &mut self,
        side: u32,
        hue: f32,
        window: &mut Window,
        cx: &mut App,
    ) -> [Arc<RenderImage>; 2] {
        if self.ring.as_ref().is_none_or(|(old, _)| *old != side) {
            if let Some((_, image)) = self.ring.take() {
                cx.drop_image(image, Some(window));
            }
            self.ring = Some((side, raster(side, None)));
        }
        let key = (side, hue.to_bits());
        if self.triangle.as_ref().is_none_or(|(old, _)| *old != key) {
            if let Some((_, image)) = self.triangle.take() {
                cx.drop_image(image, Some(window));
            }
            self.triangle = Some((key, raster(side, Some(hue))));
        }
        [
            self.ring.as_ref().unwrap().1.clone(),
            self.triangle.as_ref().unwrap().1.clone(),
        ]
    }

    pub fn clear(&mut self, cx: &mut App) {
        if let Some((_, image)) = self.ring.take() {
            cx.drop_image(image, None);
        }
        if let Some((_, image)) = self.triangle.take() {
            cx.drop_image(image, None);
        }
    }
}

// One texture for the hue ring, one for the affine RGB triangle. Only changing
// hue/scale rebuilds the triangle; S/V and alpha drags reuse both GPU images.
fn raster(side: u32, hue: Option<f32>) -> Arc<RenderImage> {
    let triangle = hue.map(Triangle::new);
    let pure = Hsv {
        h: hue.unwrap_or(0.),
        s: 1.,
        v: 1.,
    }
    .rgb(1.);
    let buffer = image::RgbaImage::from_fn(side, side, |x, y| {
        let position = [
            (x as f32 + 0.5) / side as f32,
            (y as f32 + 0.5) / side as f32,
        ];
        let (color, alpha) = if let Some(triangle) = triangle {
            let weights = triangle.weights(position);
            let distance = weights.into_iter().fold(f32::INFINITY, f32::min)
                * (TRIANGLE_RADIUS * 1.5)
                * side as f32;
            let alpha = (distance + 0.5).clamp(0., 1.);
            if alpha == 0. {
                return image::Rgba([0; 4]);
            }
            let weights = weights.map(|w| w.max(0.));
            let total = weights.into_iter().sum::<f32>();
            (
                gpui_kit::Rgba {
                    r: (pure.r * weights[0] + weights[1]) / total,
                    g: (pure.g * weights[0] + weights[1]) / total,
                    b: (pure.b * weights[0] + weights[1]) / total,
                    a: 1.,
                },
                alpha,
            )
        } else {
            let radius = (position[0] - 0.5).hypot(position[1] - 0.5);
            let distance = (OUTER - radius).min(radius - INNER) * side as f32;
            let alpha = (distance + 0.5).clamp(0., 1.);
            if alpha == 0. {
                return image::Rgba([0; 4]);
            }
            (
                Hsv {
                    h: hue_at(position),
                    s: 1.,
                    v: 1.,
                }
                .rgb(1.),
                alpha,
            )
        };
        // GPUI RenderImage consumes straight BGRA.
        image::Rgba([
            (color.b * 255.).round() as u8,
            (color.g * 255.).round() as u8,
            (color.r * 255.).round() as u8,
            (alpha * 255.).round() as u8,
        ])
    });
    Arc::new(RenderImage::new([image::Frame::new(buffer)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_vertices_and_interior_roundtrip_at_every_hue() {
        for h in [0., 0.13, 0.5, 0.79, 1.] {
            let triangle = Triangle::new(h);
            for (s, v) in [(1., 1.), (0., 1.), (0.7, 0.), (0.43, 0.72)] {
                let (actual_s, actual_v) = triangle.sv(triangle.position(s, v), s);
                assert!((s - actual_s).abs() < 0.00001);
                assert!((v - actual_v).abs() < 0.00001);
            }
            for vertex in triangle.vertices {
                let outside = vertex.map(|v| 0.5 + (v - 0.5) * 4.);
                let closest = triangle.closest(outside);
                assert!((closest[0] - vertex[0]).abs() < 0.00001);
                assert!((closest[1] - vertex[1]).abs() < 0.00001);
            }
        }
    }

    #[test]
    fn hue_ring_is_continuous_and_does_not_select_empty_space() {
        for hue in [0., 0.17, 0.49, 0.83, 0.999] {
            let actual = hue_at(hue_position(hue));
            let error = (actual - hue).abs();
            assert!(error.min(1. - error) < 0.000001);
            assert!(matches!(hit(hue_position(hue), hue), Some(Surface::Hue)));
        }
        assert!(hit([0., 0.], 0.).is_none());
        assert!(hit([0.5, 0.11], 0.).is_none());
    }
}
