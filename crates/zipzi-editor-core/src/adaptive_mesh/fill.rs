// SPDX-License-Identifier: GPL-3.0-only
// Rust port of PSD2Live graded Poisson and StructuredMeshFill sizing policies.
use super::{FillParameters, MeshFillAlgorithm, geometry::*, structured, triangulate};
use std::collections::HashMap;
pub(super) fn noise(x: i32, y: i32, salt: i32) -> f64 {
    let mut v = x
        .wrapping_mul(0x1f123bb5)
        .wrapping_add(y.wrapping_mul(0x5f356495))
        .wrapping_add(salt.wrapping_mul(0x45d9f3b)) as u32;
    v = (v ^ (v >> 16)).wrapping_mul(0x45d9f3b);
    v = (v ^ (v >> 16)).wrapping_mul(0x45d9f3b);
    (v ^ (v >> 16)) as f64 / 4294967296.
}
pub(super) fn poisson(loops: &[Vec<Point>], p: FillParameters, spacing: f64) -> Vec<Point> {
    let contour = median_spacing(loops).max(2.);
    let minimum = (contour * p.edge_ratio).clamp(4., spacing.max(4.));
    let rim = minimum * H;
    let index = SegmentIndex::new(loops, minimum);
    let b = bounds(loops);
    let step = minimum * 0.72;
    let mut candidates = vec![];
    for row in (b[1] / step).floor() as i32..=(b[3] / step).ceil() as i32 {
        for col in (b[0] / step).floor() as i32..=(b[2] / step).ceil() as i32 {
            let point = [
                (col as f64 + 0.5 + (noise(col, row, 1) - 0.5) * p.jitter) * step,
                (row as f64 + 0.5 + (noise(col, row, 2) - 0.5) * p.jitter) * step,
            ];
            if !in_domain(point, loops) {
                continue;
            }
            let depth = index.distance(point);
            if depth < rim * 0.8 {
                continue;
            }
            let radius = (minimum + p.gradation * (depth - rim).max(0.))
                .clamp(minimum, minimum.max(spacing));
            candidates.push((point, radius, noise(col, row, 3)));
        }
    }
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)));
    let cell = minimum / std::f64::consts::SQRT_2;
    let reach = (spacing.max(minimum) / cell).ceil() as i32 + 1;
    let mut cells: HashMap<(i32, i32), Vec<(Point, f64)>> = HashMap::new();
    let mut accepted = vec![];
    for (point, radius, _) in candidates {
        if accepted.len() >= 1200 {
            break;
        }
        let x = (point[0] / cell).floor() as i32;
        let y = (point[1] / cell).floor() as i32;
        // A very coarse interior spacing can span millions of empty buckets.
        // Switch to the (at most 1200) occupied samples when that costs less.
        let nearby = |q: Point, r: f64| distance(point, q) < (radius + r) * 0.5;
        let clear = if (2 * reach as i64 + 1).pow(2) > cells.len() as i64 {
            !cells.values().flatten().any(|&(q, r)| nearby(q, r))
        } else {
            let mut clear = true;
            'near: for dy in -reach..=reach {
                for dx in -reach..=reach {
                    if let Some(others) = cells.get(&(x + dx, y + dy)) {
                        if others.iter().any(|&(q, r)| nearby(q, r)) {
                            clear = false;
                            break 'near;
                        }
                    }
                }
            }
            clear
        };
        if clear {
            cells.entry((x, y)).or_default().push((point, radius));
            accepted.push(point);
        }
    }
    accepted
}
pub(super) fn hex(loops: &[Vec<Point>], spacing: f64, clearance: f64) -> Vec<Point> {
    let b = bounds(loops);
    let idx = SegmentIndex::new(loops, spacing);
    let step = spacing * H;
    let rows = ((b[3] - b[1]) / step).floor() as usize;
    let origin = b[1] + ((b[3] - b[1]) - rows as f64 * step) * 0.5;
    let mut points = vec![];
    for row in 0..=rows {
        let y = origin + row as f64 * step;
        let mut x = b[0] + if row % 2 == 0 { 0. } else { spacing * 0.5 };
        while x <= b[2] {
            let p = [x, y];
            if in_domain(p, loops) && idx.distance(p) >= clearance * spacing {
                points.push(p)
            }
            x += spacing;
        }
    }
    points
}
pub(super) fn generate(
    loops: &[Vec<Point>],
    algorithm: MeshFillAlgorithm,
    p: FillParameters,
    spacing: f64,
    suppress: bool,
    empty: bool,
) -> Option<Mesh> {
    if empty {
        return triangulate::triangulate(loops, &[], f64::INFINITY, suppress);
    }
    // Also bound the expanded domain, since margin/band controls can extend
    // beyond the source-image budget. A rejected layout uses the caller's
    // narrower-band/raw-contour recovery, before allocating candidate arrays.
    let b = bounds(loops);
    let step = match algorithm {
        MeshFillAlgorithm::GradedPoisson => {
            (median_spacing(loops).max(2.) * p.edge_ratio).clamp(4., spacing.max(4.)) * 0.72
        }
        MeshFillAlgorithm::SimpleTriangles => spacing * H,
        _ => median_spacing(loops).max(2.) * H,
    };
    if ((b[2] - b[0]) / step + 3.).ceil() * ((b[3] - b[1]) / step + 3.).ceil() > 1_048_576. {
        return None;
    }
    match algorithm {
        MeshFillAlgorithm::GradedPoisson => {
            triangulate::triangulate(loops, &poisson(loops, p, spacing), spacing, suppress)
        }
        MeshFillAlgorithm::SimpleTriangles => {
            triangulate::triangulate(loops, &hex(loops, spacing, 0.), spacing, suppress)
        }
        _ => structured::generate(loops, algorithm, p, spacing, suppress),
    }
}
