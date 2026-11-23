use crate::SceneTexture;
use zipzi_core::{Drawable, Vec2};

pub(super) fn samples_alpha(drawable: &Drawable, texture: &SceneTexture) -> bool {
    if texture.width == 0 || texture.height == 0 {
        return false;
    }
    let repeat = drawable.texture_repeat.unwrap_or(true);
    drawable.indices.as_chunks::<3>().0.iter().any(|face| {
        let [Some(&a), Some(&b), Some(&c)] = face.map(|i| drawable.uvs.get(i as usize)) else {
            return false;
        };
        triangle_samples_alpha([a, b, c], texture, repeat)
    })
}

fn triangle_samples_alpha(uv: [Vec2; 3], texture: &SceneTexture, repeat: bool) -> bool {
    let width = texture.width as usize;
    let height = texture.height as usize;
    // Use texel-center coordinates, with decoded rows stored top down.
    let origin = if repeat {
        [f64::from(uv[0].x).floor(), (-f64::from(uv[0].y)).floor()]
    } else {
        [0., -1.]
    };
    let points = uv.map(|p| {
        [
            (f64::from(p.x) - origin[0]) * width as f64 - 0.5,
            (-f64::from(p.y) - origin[1]) * height as f64 - 0.5,
        ]
    });
    let ys = points.map(|p| p[1]);
    let mut first = ys.into_iter().fold(f64::INFINITY, f64::min).floor();
    let mut last = ys.into_iter().fold(f64::NEG_INFINITY, f64::max).ceil();
    if repeat {
        // Bound work for unbounded tiled UVs. Test the horizontal footprint
        // across one texture period when a triangle spans many vertical tiles.
        if last - first > (height * 64) as f64 {
            let xs = points.map(|p| p[0]);
            let left = xs.into_iter().fold(f64::INFINITY, f64::min);
            let right = xs.into_iter().fold(f64::NEG_INFINITY, f64::max);
            return texture
                .rgba
                .chunks_exact(width * 4)
                .any(|row| span_samples_alpha(row, left, right, true));
        }
    } else {
        first = first.clamp(0., (height - 1) as f64);
        last = last.clamp(0., (height - 1) as f64);
    }
    (first as i64..=last as i64).any(|y| {
        let row = y.rem_euclid(height as i64) as usize;
        // Bilinear sampling sees this row up to one texel center away. Edge
        // texels also cover the exterior half-plane for clamp addressing.
        let low = if !repeat && row == 0 {
            f64::NEG_INFINITY
        } else {
            y as f64 - 1.
        };
        let high = if !repeat && row + 1 == height {
            f64::INFINITY
        } else {
            y as f64 + 1.
        };
        let [left, right] = horizontal_span(points, low, high);
        let rgba = &texture.rgba[row * width * 4..(row + 1) * width * 4];
        span_samples_alpha(rgba, left, right, repeat)
    })
}

fn span_samples_alpha(row: &[u8], left: f64, right: f64, repeat: bool) -> bool {
    let width = row.len() / 4;
    let has_alpha =
        |start: usize, end: usize| row[start * 4..end * 4].chunks_exact(4).any(|p| p[3] != 0);
    if repeat {
        let start = left.floor().rem_euclid(width as f64) as usize;
        let count = (right.ceil() - left.floor() + 1.).min(width as f64) as usize;
        let end = start + count;
        has_alpha(start, end.min(width)) || (end > width && has_alpha(0, end - width))
    } else {
        let start = left.floor().clamp(0., (width - 1) as f64) as usize;
        let end = right.ceil().clamp(0., (width - 1) as f64) as usize;
        has_alpha(start, end + 1)
    }
}

/// Project the triangle's intersection with a horizontal strip onto the X axis.
fn horizontal_span(points: [[f64; 2]; 3], low: f64, high: f64) -> [f64; 2] {
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut include = |x: f64| {
        left = left.min(x);
        right = right.max(x);
    };
    for (a, b) in [
        (points[0], points[1]),
        (points[1], points[2]),
        (points[2], points[0]),
    ] {
        if (low..=high).contains(&a[1]) {
            include(a[0]);
        }
        for y in [low, high] {
            if a[1].min(b[1]) < y && y < a[1].max(b[1]) {
                include(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
    }
    [left, right]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_footprint_respects_texel_centers_and_address_mode() {
        let mut pixels = vec![0; 4 * 2 * 4];
        pixels[3] = 1; // Top-left texel, smallest nonzero alpha.
        let texture = SceneTexture {
            asset_id: String::new(),
            cache_key: String::new(),
            width: 4,
            height: 2,
            rgba: pixels.into(),
        };
        let sample = |x, y, repeat| triangle_samples_alpha([Vec2::new(x, y); 3], &texture, repeat);
        assert!(sample(0.374, 0.75, false));
        assert!(!sample(0.375, 0.75, false));
        assert!(!sample(0.125, 0.25, false));
        assert!(sample(-2., 3., false));
        assert!(sample(2.99, -1.25, true));
        assert!(!sample(2.99, -1.25, false));
        let tiled = [
            Vec2::new(0.5, -1000.),
            Vec2::new(0.6, 1000.),
            Vec2::new(0.7, 0.),
        ];
        assert!(!triangle_samples_alpha(tiled, &texture, true));
    }
}
