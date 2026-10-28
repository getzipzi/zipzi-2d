// SPDX-License-Identifier: GPL-3.0-only
// Rust port of AdaptiveMeshGenerator.buildRibbon.
use super::geometry::*;
pub(super) fn build(
    boundary: &[Point],
    neighbors: &[Vec<Point>],
    spacing: f64,
    scale: f64,
    sampling: f64,
) -> Option<(Mesh, Vec<usize>)> {
    let mean = mul(
        boundary.iter().copied().fold([0., 0.], add),
        1. / boundary.len() as f64,
    );
    let xx = boundary
        .iter()
        .map(|p| (p[0] - mean[0]).powi(2))
        .sum::<f64>();
    let yy = boundary
        .iter()
        .map(|p| (p[1] - mean[1]).powi(2))
        .sum::<f64>();
    let xy = boundary
        .iter()
        .map(|p| (p[0] - mean[0]) * (p[1] - mean[1]))
        .sum::<f64>();
    let principal = (2. * xy).atan2(xx - yy) * 0.5;
    let angle = if principal.sin().abs() < 0.08 {
        0.
    } else if principal.cos().abs() < 0.08 {
        std::f64::consts::FRAC_PI_2
    } else {
        principal
    };
    let axis = [angle.cos(), angle.sin()];
    let projected: Vec<_> = boundary
        .iter()
        .map(|p| {
            [
                p[0] * axis[0] + p[1] * axis[1],
                -p[0] * axis[1] + p[1] * axis[0],
            ]
        })
        .collect();
    let start = projected.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let end = projected
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let length = end - start;
    let thickness = area(boundary).abs() / length.max(EPS);
    if thickness <= 0. || length < thickness * 7. || thickness > spacing * 0.45 {
        return None;
    }
    let section = |x: f64| {
        let mut hits = vec![];
        for i in 0..projected.len() {
            let a = projected[i];
            let b = projected[(i + 1) % projected.len()];
            if x < a[0].min(b[0]) - 1e-7 || x > a[0].max(b[0]) + 1e-7 {
                continue;
            }
            if (a[0] - b[0]).abs() < 1e-8 {
                hits.extend([a[1], b[1]])
            } else {
                hits.push(a[1] + ((x - a[0]) / (b[0] - a[0])).clamp(0., 1.) * (b[1] - a[1]))
            }
        }
        hits.sort_by(f64::total_cmp);
        hits.dedup_by(|a, b| (*a - *b).abs() <= 1e-6);
        hits
    };
    let mut events: Vec<_> = projected.iter().map(|p| p[0]).collect();
    events.sort_by(f64::total_cmp);
    events.dedup();
    for pair in events.windows(2) {
        if pair[1] - pair[0] <= 1e-6 {
            continue;
        }
        let hits = section((pair[0] + pair[1]) * 0.5);
        if hits.len() < 2
            || hits.len() % 2 != 0
            || (1..hits.len() - 1)
                .step_by(2)
                .any(|i| hits[i + 1] - hits[i] > 0.75)
        {
            return None;
        }
    }
    let clearance = if neighbors.is_empty() {
        f64::INFINITY
    } else {
        let idx = SegmentIndex::new(neighbors, spacing);
        boundary
            .iter()
            .map(|&p| idx.distance(p))
            .fold(f64::INFINITY, f64::min)
    };
    let margin = 6_f64.min(spacing * 0.16).min(clearance * 0.24) * scale;
    let step = (spacing * 0.5).min(3_f64.max((thickness + margin * 2.) * 1.25)) * sampling;
    let intervals = (length / step).ceil().clamp(2., 480.) as usize;
    let stations: Vec<_> = (0..=intervals)
        .map(|i| start + length * i as f64 / intervals as f64)
        .collect();
    let mut lows = vec![];
    let mut highs = vec![];
    for &x in &stations {
        let hits = section(x);
        lows.push(*hits.first()?);
        highs.push(*hits.last()?);
    }
    for i in 0..intervals {
        let mut low = 0_f64;
        let mut high = 0_f64;
        for &p in &projected {
            if p[0] < stations[i] || p[0] > stations[i + 1] {
                continue;
            }
            let t = (p[0] - stations[i]) / (stations[i + 1] - stations[i]);
            low = low.max(lows[i] * (1. - t) + lows[i + 1] * t - p[1]);
            high = high.max(p[1] - highs[i] * (1. - t) - highs[i + 1] * t);
        }
        lows[i] -= low;
        lows[i + 1] -= low;
        highs[i] += high;
        highs[i + 1] += high;
    }
    let mut mesh = Mesh::default();
    for i in 0..stations.len() {
        let center = (lows[i] + highs[i]) * 0.5;
        let half = ((highs[i] - lows[i]) * 0.5 + margin).max(0.25);
        for side in [-1., 1.] {
            let y = center + half * side;
            mesh.points.push(snap([
                stations[i] * axis[0] - y * axis[1],
                stations[i] * axis[1] + y * axis[0],
            ]));
        }
    }
    for i in 0..intervals {
        let a = i * 2;
        let b = a + 2;
        let c = b + 1;
        let d = a + 1;
        if i % 2 == 0 {
            mesh.triangles.extend([[a, b, c], [a, c, d]])
        } else {
            mesh.triangles.extend([[a, b, d], [b, c, d]])
        }
    }
    let border: Vec<_> = (0..stations.len())
        .map(|i| i * 2)
        .chain((0..stations.len()).rev().map(|i| i * 2 + 1))
        .collect();
    if !valid_mesh(&mesh, std::slice::from_ref(&border)) {
        return None;
    }
    Some((mesh, border))
}
