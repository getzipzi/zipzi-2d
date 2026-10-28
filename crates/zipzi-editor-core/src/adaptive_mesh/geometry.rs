// SPDX-License-Identifier: GPL-3.0-only
// Rust adaptation of PSD2Live core/mesh/MeshGeometry.kt; see README.md.
use std::collections::{BTreeMap, BTreeSet};
pub(super) type Point = [f64; 2];
pub(super) type Triangle = [usize; 3];
pub(super) type Loops = Vec<Vec<Point>>;
pub(super) const EPS: f64 = 1e-8;
pub(super) const H: f64 = 0.8660254037844386;
#[derive(Clone, Debug, Default)]
pub(super) struct Mesh {
    pub points: Vec<Point>,
    pub triangles: Vec<Triangle>,
}
pub(super) fn snap(p: Point) -> Point {
    p.map(|v| v as f32 as f64)
}
pub(super) fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
pub(super) fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
pub(super) fn mul(a: Point, t: f64) -> Point {
    [a[0] * t, a[1] * t]
}
pub(super) fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
pub(super) fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
pub(super) fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
pub(super) fn lerp(a: Point, b: Point, t: f64) -> Point {
    add(a, mul(sub(b, a), t))
}
pub(super) fn normal(a: Point) -> Point {
    mul(a, 1.0 / distance(a, [0., 0.]).max(EPS))
}
pub(super) fn area(p: &[Point]) -> f64 {
    (0..p.len())
        .map(|i| {
            let q = p[(i + 1) % p.len()];
            p[i][0] * q[1] - q[0] * p[i][1]
        })
        .sum::<f64>()
        * 0.5
}
pub(super) fn orient(mut p: Vec<Point>, positive: bool) -> Vec<Point> {
    if (area(&p) > 0.) != positive {
        p.reverse()
    }
    p.into_iter().map(snap).collect()
}
pub(super) fn edges(t: Triangle) -> [[usize; 2]; 3] {
    [edge(t[0], t[1]), edge(t[1], t[2]), edge(t[2], t[0])]
}
pub(super) fn edge(a: usize, b: usize) -> [usize; 2] {
    [a.min(b), a.max(b)]
}
pub(super) fn inside(p: Point, ring: &[Point]) -> bool {
    let mut yes = false;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            yes = !yes
        }
    }
    yes
}
pub(super) fn in_domain(p: Point, loops: &[Vec<Point>]) -> bool {
    !loops.is_empty() && inside(p, &loops[0]) && !loops[1..].iter().any(|r| inside(p, r))
}
pub(super) fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let d = sub(b, a);
    distance(
        p,
        lerp(a, b, (dot(sub(p, a), d) / dot(d, d).max(EPS)).clamp(0., 1.)),
    )
}
pub(super) fn on_segment(p: Point, a: Point, b: Point) -> bool {
    segment_distance(p, a, b) <= 1e-7
}
pub(super) fn intersects(a: Point, b: Point, c: Point, d: Point) -> bool {
    cross(a, b, c) * cross(a, b, d) < -EPS && cross(c, d, a) * cross(c, d, b) < -EPS
}
pub(super) fn touches(a: &[Point], b: &[Point]) -> bool {
    (0..a.len()).any(|i| {
        (0..b.len()).any(|j| {
            let (p, q, r, s) = (a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()]);
            intersects(p, q, r, s)
                || on_segment(p, r, s)
                || on_segment(q, r, s)
                || on_segment(r, p, q)
                || on_segment(s, p, q)
        })
    })
}
pub(super) fn simple(r: &[Point]) -> bool {
    r.len() >= 3
        && area(r).abs() > 1e-6
        && !(0..r.len()).any(|i| {
            (i + 1..r.len()).any(|j| {
                if j == i + 1 || (i == 0 && j == r.len() - 1) {
                    return false;
                }
                intersects(r[i], r[(i + 1) % r.len()], r[j], r[(j + 1) % r.len()])
            })
        })
}
pub(super) fn valid_domain(loops: &[Vec<Point>]) -> bool {
    !loops.is_empty()
        && loops.iter().all(|r| simple(r))
        && (0..loops.len()).all(|i| {
            (0..i).all(|j| {
                !touches(&loops[i], &loops[j])
                    && if j == 0 {
                        inside(loops[i][0], &loops[0])
                    } else {
                        !inside(loops[i][0], &loops[j]) && !inside(loops[j][0], &loops[i])
                    }
            })
        })
}
pub(super) fn bounds(loops: &[Vec<Point>]) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in loops.iter().flatten() {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}
pub(super) fn median_spacing(loops: &[Vec<Point>]) -> f64 {
    let mut d: Vec<_> = loops
        .iter()
        .flat_map(|r| (0..r.len()).map(|i| distance(r[i], r[(i + 1) % r.len()])))
        .collect();
    d.sort_by(f64::total_cmp);
    d[d.len() / 2]
}
pub(super) fn loop_ids(loops: &[Vec<Point>]) -> Vec<Vec<usize>> {
    let mut n = 0;
    loops
        .iter()
        .map(|r| {
            let ids = (n..n + r.len()).collect();
            n += r.len();
            ids
        })
        .collect()
}
pub(super) fn valid_mesh(mesh: &Mesh, boundaries: &[Vec<usize>]) -> bool {
    if mesh.triangles.is_empty()
        || mesh
            .triangles
            .iter()
            .any(|t| cross(mesh.points[t[0]], mesh.points[t[1]], mesh.points[t[2]]) <= EPS)
    {
        return false;
    }
    let mut uses = BTreeMap::new();
    for t in &mesh.triangles {
        for e in edges(*t) {
            *uses.entry(e).or_insert(0usize) += 1;
        }
    }
    let rim: BTreeSet<_> = boundaries
        .iter()
        .flat_map(|r| (0..r.len()).map(|i| edge(r[i], r[(i + 1) % r.len()])))
        .collect();
    let used: BTreeSet<_> = mesh.triangles.iter().flatten().copied().collect();
    used.len() == mesh.points.len()
        && uses
            .iter()
            .all(|(e, n)| *n == if rim.contains(e) { 1 } else { 2 })
        && rim.iter().all(|e| uses.contains_key(e))
        && mesh.points.len() as isize - uses.len() as isize + mesh.triangles.len() as isize
            == 2 - boundaries.len() as isize
}
/// Segment buckets keep distance/ray queries independent of raster size.
pub(super) struct SegmentIndex {
    segments: Vec<(Point, Point)>,
    origin: Point,
    cell: f64,
    cols: usize,
    rows: usize,
    buckets: Vec<Vec<usize>>,
}
impl SegmentIndex {
    pub fn new(loops: &[Vec<Point>], hint: f64) -> Self {
        let b = bounds(loops);
        let cell = hint.max(((b[2] - b[0]).max(b[3] - b[1])) / 256.).max(1.);
        let cols = ((b[2] - b[0]) / cell).ceil() as usize + 1;
        let rows = ((b[3] - b[1]) / cell).ceil() as usize + 1;
        let segments: Vec<_> = loops
            .iter()
            .flat_map(|r| (0..r.len()).map(|i| (r[i], r[(i + 1) % r.len()])))
            .collect();
        let mut s = Self {
            segments,
            origin: [b[0], b[1]],
            cell,
            cols,
            rows,
            buckets: vec![vec![]; cols * rows],
        };
        for (i, (a, b)) in s.segments.iter().enumerate() {
            for y in s.row(a[1].min(b[1]))..=s.row(a[1].max(b[1])) {
                for x in s.col(a[0].min(b[0]))..=s.col(a[0].max(b[0])) {
                    s.buckets[y * cols + x].push(i)
                }
            }
        }
        s
    }
    fn col(&self, x: f64) -> usize {
        ((x - self.origin[0]) / self.cell)
            .floor()
            .clamp(0., (self.cols - 1) as f64) as usize
    }
    fn row(&self, y: f64) -> usize {
        ((y - self.origin[1]) / self.cell)
            .floor()
            .clamp(0., (self.rows - 1) as f64) as usize
    }
    pub fn distance(&self, p: Point) -> f64 {
        let cx = self.col(p[0]) as isize;
        let cy = self.row(p[1]) as isize;
        let mut best = f64::INFINITY;
        for ring in 0..=self.cols.max(self.rows) as isize {
            // Lower bound uses the visited rectangle, including queries outside the grid.
            for y in (cy - ring).max(0)..=(cy + ring).min(self.rows as isize - 1) {
                for x in (cx - ring).max(0)..=(cx + ring).min(self.cols as isize - 1) {
                    if ring > 0 && (x - cx).abs() != ring && (y - cy).abs() != ring {
                        continue;
                    }
                    for &i in &self.buckets[y as usize * self.cols + x as usize] {
                        let (a, b) = self.segments[i];
                        best = best.min(segment_distance(p, a, b));
                    }
                }
            }
            let mut lower = f64::INFINITY;
            if cx - ring > 0 {
                lower = lower.min(p[0] - (self.origin[0] + (cx - ring) as f64 * self.cell))
            }
            if cx + ring + 1 < self.cols as isize {
                lower = lower.min(self.origin[0] + (cx + ring + 1) as f64 * self.cell - p[0])
            }
            if cy - ring > 0 {
                lower = lower.min(p[1] - (self.origin[1] + (cy - ring) as f64 * self.cell))
            }
            if cy + ring + 1 < self.rows as isize {
                lower = lower.min(self.origin[1] + (cy + ring + 1) as f64 * self.cell - p[1])
            }
            if best <= lower {
                break;
            }
        }
        best
    }
    pub fn ray(&self, p: Point, d: Point, reach: f64) -> f64 {
        let mut best = f64::INFINITY;
        for y in self.row(p[1] - reach)..=self.row(p[1] + reach) {
            for x in self.col(p[0] - reach)..=self.col(p[0] + reach) {
                for &i in &self.buckets[y * self.cols + x] {
                    let (a, b) = self.segments[i];
                    if segment_distance(p, a, b) < 1e-5 {
                        continue;
                    }
                    let e = sub(b, a);
                    let den = d[0] * e[1] - d[1] * e[0];
                    if den.abs() < 1e-12 {
                        continue;
                    }
                    let w = sub(a, p);
                    let t = (w[0] * e[1] - w[1] * e[0]) / den;
                    let u = (w[0] * d[1] - w[1] * d[0]) / den;
                    if t > 1e-6 && (-1e-9..=1. + 1e-9).contains(&u) {
                        best = best.min(t)
                    }
                }
            }
        }
        best
    }
}
