// SPDX-License-Identifier: GPL-3.0-only
// Rust adaptation of PSD2Live StructuredMeshFill.kt. Fixed core triangles are
// constrained directly in spade; free gaps are triangulated without changing them.
use super::{FillParameters, MeshFillAlgorithm, bands, fill, geometry::*, triangulate};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
const MAX_CORE: usize = 24_000;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Key(i32, i32);
fn mid(a: Key, b: Key) -> Key {
    Key((a.0 + b.0) / 2, (a.1 + b.1) / 2)
}
fn ke(a: Key, b: Key) -> [Key; 2] {
    [a.min(b), a.max(b)]
}
struct Field<'a> {
    loops: &'a [Vec<Point>],
    index: SegmentIndex,
    first: f64,
    coarse: f64,
    gradation: f64,
}
impl<'a> Field<'a> {
    fn new(loops: &'a [Vec<Point>], coarse: f64, p: FillParameters) -> Self {
        let fine = median_spacing(loops).max(2.);
        let coarse = coarse.max(fine);
        Self {
            loops,
            index: SegmentIndex::new(loops, fine),
            first: (fine * p.edge_ratio).clamp(fine, coarse),
            coarse,
            gradation: p.gradation,
        }
    }
    fn target(&self, depth: f64) -> f64 {
        (self.first + self.gradation * depth).clamp(self.first, self.coarse)
    }
    fn refine(&self, p: [Point; 3], size: f64, finest: f64) -> bool {
        if size <= finest * 1.0001 {
            return false;
        }
        let center = mul(add(add(p[0], p[1]), p[2]), 1. / 3.);
        let radius = p
            .into_iter()
            .map(|p| distance(center, p))
            .fold(0., f64::max);
        let d = self.index.distance(center);
        let inside = in_domain(center, self.loops);
        if !inside && d > radius {
            return false;
        }
        size > self.target(if inside {
            (d - radius * 0.5).max(0.)
        } else {
            0.
        }) * 1.0001
    }
}
#[derive(Clone, Copy)]
struct Right {
    v: [Key; 3],
    alive: bool,
}
struct Tree {
    all: Vec<Right>,
    edges: BTreeMap<[Key; 2], BTreeSet<usize>>,
    queue: VecDeque<usize>,
}
impl Tree {
    fn add(&mut self, v: [Key; 3]) {
        let id = self.all.len();
        self.all.push(Right { v, alive: true });
        self.queue.push_back(id);
        for i in 0..3 {
            self.edges
                .entry(ke(v[i], v[(i + 1) % 3]))
                .or_default()
                .insert(id);
        }
    }
    fn bisect(&mut self, id: usize) {
        let [a, b, c] = self.all[id].v;
        let m = mid(b, c);
        self.all[id].alive = false;
        for i in 0..3 {
            let v = self.all[id].v;
            self.edges
                .get_mut(&ke(v[i], v[(i + 1) % 3]))
                .unwrap()
                .remove(&id);
        }
        self.add([m, b, a]);
        self.add([m, a, c]);
    }
    fn partner(&self, id: usize) -> Option<usize> {
        let v = self.all[id].v;
        self.edges
            .get(&ke(v[1], v[2]))?
            .iter()
            .copied()
            .find(|&i| i != id && self.all[i].alive)
    }
    fn split(&mut self, id: usize, depth: usize) -> bool {
        if !self.all[id].alive {
            return true;
        }
        let v = self.all[id].v;
        if (v[1].0 + v[2].0) % 2 != 0
            || (v[1].1 + v[2].1) % 2 != 0
            || depth > 64
            || self.all.len() > MAX_CORE
        {
            return false;
        }
        let mut partner = self.partner(id);
        if let Some(other) = partner {
            let p = self.all[other].v;
            if ke(v[1], v[2]) != ke(p[1], p[2]) {
                if !self.split(other, depth + 1) {
                    return false;
                }
                partner = self.partner(id);
                if let Some(other) = partner {
                    let p = self.all[other].v;
                    if ke(v[1], v[2]) != ke(p[1], p[2]) {
                        return false;
                    }
                }
            }
        }
        self.bisect(id);
        if let Some(other) = partner {
            self.bisect(other)
        }
        true
    }
}
fn quadtree(f: &Field<'_>) -> Vec<([Point; 3], f64)> {
    let b = bounds(f.loops);
    let levels = (f.coarse / f.first).log2().floor().clamp(0., 10.) as u32;
    let root = 1i32 << (levels + 1);
    let unit = f.coarse / root as f64;
    let cols = ((b[2] - b[0]) / f.coarse).ceil().max(1.) as i32;
    let rows = ((b[3] - b[1]) / f.coarse).ceil().max(1.) as i32;
    let origin = [
        b[0] - (cols as f64 * f.coarse - (b[2] - b[0])) * 0.5,
        b[1] - (rows as f64 * f.coarse - (b[3] - b[1])) * 0.5,
    ];
    let world = |k: Key| [origin[0] + k.0 as f64 * unit, origin[1] + k.1 as f64 * unit];
    let mut tree = Tree {
        all: vec![],
        edges: BTreeMap::new(),
        queue: VecDeque::new(),
    };
    for y in 0..rows {
        for x in 0..cols {
            let a = Key(x * root, y * root);
            let b = Key((x + 1) * root, y * root);
            let c = Key((x + 1) * root, (y + 1) * root);
            let d = Key(x * root, (y + 1) * root);
            if (x + y) % 2 == 0 {
                tree.add([b, a, c]);
                tree.add([d, c, a]);
            } else {
                tree.add([a, d, b]);
                tree.add([c, b, d]);
            }
        }
    }
    while let Some(id) = tree.queue.pop_front() {
        let t = tree.all[id];
        if !t.alive {
            continue;
        }
        let p = t.v.map(world);
        if f.refine(p, distance(p[0], p[1]), f.first) {
            tree.split(id, 0);
        }
    }
    tree.all
        .into_iter()
        .filter(|t| t.alive)
        .map(|t| {
            let p = t.v.map(world);
            (p, distance(p[0], p[1]))
        })
        .collect()
}
#[derive(Clone, Copy)]
struct Equilateral {
    v: [Key; 3],
    size: i32,
    alive: bool,
}
fn fractal(f: &Field<'_>) -> Vec<([Point; 3], f64)> {
    let b = bounds(f.loops);
    let levels = (f.coarse / f.first).log2().round().clamp(0., 10.) as u32;
    let root = 1i32 << levels;
    let unit = f.coarse / root as f64;
    let row_height = f.coarse * H;
    let rows = ((b[3] - b[1]) / row_height).ceil().max(1.) as i32;
    let oy = b[1] - (rows as f64 * row_height - (b[3] - b[1])) * 0.5;
    let world = |k: Key| {
        [
            b[0] + (k.0 as f64 + k.1 as f64 * 0.5) * unit,
            oy + k.1 as f64 * H * unit,
        ]
    };
    let mut all = vec![];
    let mut vertices = BTreeSet::new();
    fn push(all: &mut Vec<Equilateral>, vertices: &mut BTreeSet<Key>, v: [Key; 3], size: i32) {
        vertices.extend(v);
        all.push(Equilateral {
            v,
            size,
            alive: true,
        });
    }
    fn split(all: &mut Vec<Equilateral>, vertices: &mut BTreeSet<Key>, i: usize) {
        let t = all[i];
        all[i].alive = false;
        let [a, b, c] = t.v;
        let ab = mid(a, b);
        let bc = mid(b, c);
        let ca = mid(c, a);
        for v in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [bc, ca, ab]] {
            push(all, vertices, v, t.size / 2)
        }
    }
    for j in -1..=rows {
        let q = j * root;
        let from = (-q as f64 * 0.5 / root as f64).floor() as i32 - 1;
        let to = (((b[2] - b[0]) / unit - q as f64 * 0.5) / root as f64).ceil() as i32 + 1;
        for i in from..=to {
            let p = i * root;
            push(
                &mut all,
                &mut vertices,
                [Key(p, q), Key(p + root, q), Key(p, q + root)],
                root,
            );
            push(
                &mut all,
                &mut vertices,
                [Key(p + root, q), Key(p + root, q + root), Key(p, q + root)],
                root,
            );
        }
    }
    let mut i = 0;
    while i < all.len() && all.len() < MAX_CORE {
        let t = all[i];
        if t.alive
            && t.size >= 2
            && f.refine(
                t.v.map(world),
                t.size as f64 * unit,
                f.first * std::f64::consts::SQRT_2,
            )
        {
            split(&mut all, &mut vertices, i)
        }
        i += 1;
    }
    loop {
        let mut changed = false;
        let count = all.len();
        for i in 0..count {
            let t = all[i];
            if !t.alive || t.size < 2 || all.len() >= MAX_CORE {
                continue;
            }
            let mut hanging = 0;
            let mut deep = false;
            for j in 0..3 {
                let p = t.v[j];
                let q = t.v[(j + 1) % 3];
                if !vertices.contains(&mid(p, q)) {
                    continue;
                }
                hanging += 1;
                if t.size % 4 == 0 {
                    let dx = (q.0 - p.0) / 4;
                    let dy = (q.1 - p.1) / 4;
                    deep |= vertices.contains(&Key(p.0 + dx, p.1 + dy))
                        || vertices.contains(&Key(p.0 + dx * 3, p.1 + dy * 3));
                }
            }
            if deep || hanging == 3 {
                split(&mut all, &mut vertices, i);
                changed = true
            }
        }
        if !changed {
            break;
        }
    }
    let mut output = vec![];
    for t in all.into_iter().filter(|t| t.alive) {
        let size = t.size as f64 * unit;
        let hang: Vec<_> = (0..3)
            .map(|i| t.size >= 2 && vertices.contains(&mid(t.v[i], t.v[(i + 1) % 3])))
            .collect();
        match hang.iter().filter(|v| **v).count() {
            0 => output.push((t.v.map(world), size)),
            1 => {
                let i = hang.iter().position(|v| *v).unwrap();
                let p = t.v[i];
                let q = t.v[(i + 1) % 3];
                let o = t.v[(i + 2) % 3];
                let m = mid(p, q);
                for v in [[p, m, o], [m, q, o]] {
                    output.push((v.map(world), size * 0.5))
                }
            }
            2 => {
                let free = hang.iter().position(|v| !*v).unwrap();
                let b = t.v[free];
                let c = t.v[(free + 1) % 3];
                let a = t.v[(free + 2) % 3];
                let ma = mid(c, a);
                let mb = mid(a, b);
                for v in [[a, mb, ma], [mb, b, c], [mb, c, ma]] {
                    output.push((v.map(world), size * 0.5))
                }
            }
            _ => {
                // At the resource cap close all three hanging sides without T-junctions.
                let [a, b, c] = t.v;
                let ab = mid(a, b);
                let bc = mid(b, c);
                let ca = mid(c, a);
                for v in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [bc, ca, ab]] {
                    output.push((v.map(world), size * 0.5))
                }
            }
        }
    }
    output
}
fn ccw(mut p: [Point; 3]) -> [Point; 3] {
    if cross(p[0], p[1], p[2]) < 0. {
        p.swap(1, 2)
    }
    p
}
fn lattice_core(f: &Field<'_>, kind: MeshFillAlgorithm) -> Mesh {
    let triangles = if kind == MeshFillAlgorithm::AdaptiveQuadtree {
        quadtree(f)
    } else {
        fractal(f)
    };
    let mut mesh = Mesh::default();
    let mut keys = BTreeMap::new();
    for (p, size) in triangles {
        let p = ccw(p.map(snap));
        let center = mul(add(add(p[0], p[1]), p[2]), 1. / 3.);
        if !in_domain(center, f.loops)
            || p.iter()
                .any(|&p| !in_domain(p, f.loops) || f.index.distance(p) < 0.6 * size)
        {
            continue;
        }
        let probes = [
            lerp(p[0], p[1], 0.5),
            lerp(p[1], p[2], 0.5),
            lerp(p[2], p[0], 0.5),
            center,
        ];
        if probes
            .iter()
            .any(|&q| !in_domain(q, f.loops) || f.index.distance(q) < 0.3 * size)
        {
            continue;
        }
        let t = p.map(|p| {
            *keys
                .entry((p[0].to_bits(), p[1].to_bits()))
                .or_insert_with(|| {
                    let i = mesh.points.len();
                    mesh.points.push(p);
                    i
                })
        });
        mesh.triangles.push(t);
    }
    // Remove isolated and pinch-rim triangles as upstream does.
    for _ in 0..64 {
        let mut uses = BTreeMap::new();
        for t in &mesh.triangles {
            for e in edges(*t) {
                *uses.entry(e).or_insert(0) += 1;
            }
        }
        let mut rim_degree = BTreeMap::new();
        for (e, n) in &uses {
            if *n == 1 {
                for v in e {
                    *rim_degree.entry(*v).or_insert(0) += 1;
                }
            }
        }
        let before = mesh.triangles.len();
        mesh.triangles.retain(|t| {
            let rim = edges(*t).iter().filter(|e| uses[*e] == 1).count();
            rim < 3
                && !(rim > 0
                    && t.iter()
                        .any(|i| rim_degree.get(i).copied().unwrap_or(0) > 2))
        });
        if mesh.triangles.len() == before {
            break;
        }
    }
    compact(&mut mesh);
    conform_rim(&mut mesh, f);
    mesh
}
fn compact(m: &mut Mesh) {
    let used: BTreeSet<_> = m.triangles.iter().flatten().copied().collect();
    let mut map = vec![0; m.points.len()];
    let mut points = vec![];
    for i in used {
        map[i] = points.len();
        points.push(m.points[i]);
    }
    for t in &mut m.triangles {
        *t = t.map(|i| map[i])
    }
    m.points = points;
}
fn min_angle(a: Point, b: Point, c: Point) -> f64 {
    let ab = distance(a, b);
    let bc = distance(b, c);
    let ca = distance(c, a);
    let angle = |o: f64, x: f64, y: f64| {
        ((x * x + y * y - o * o) / (2. * x * y).max(EPS))
            .clamp(-1., 1.)
            .acos()
    };
    angle(bc, ab, ca)
        .min(angle(ca, ab, bc))
        .min(angle(ab, bc, ca))
}
fn conform_rim(m: &mut Mesh, f: &Field<'_>) {
    let mut uses = BTreeMap::new();
    let mut incident = vec![vec![]; m.points.len()];
    for (ti, &t) in m.triangles.iter().enumerate() {
        for e in edges(t) {
            *uses.entry(e).or_insert(0) += 1;
        }
        for v in t {
            incident[v].push(ti)
        }
    }
    let mut adjacent: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (e, n) in uses {
        if n == 1 {
            adjacent.entry(e[0]).or_default().push(e[1]);
            adjacent.entry(e[1]).or_default().push(e[0]);
        }
    }
    for _ in 0..2 {
        for (&v, neighbors) in &adjacent {
            if neighbors.len() != 2 {
                continue;
            }
            let p = m.points[v];
            let target = H
                * 0.5
                * (distance(p, m.points[neighbors[0]]) + distance(p, m.points[neighbors[1]]));
            let depth = f.index.distance(p);
            let g = [
                f.index.distance([p[0] + 0.5, p[1]]) - f.index.distance([p[0] - 0.5, p[1]]),
                f.index.distance([p[0], p[1] + 0.5]) - f.index.distance([p[0], p[1] - 0.5]),
            ];
            if distance(g, [0., 0.]) < 1e-6 {
                continue;
            }
            let shortest = incident[v]
                .iter()
                .flat_map(|&t| m.triangles[t])
                .filter(|&i| i != v)
                .map(|i| distance(p, m.points[i]))
                .fold(f64::INFINITY, f64::min);
            let reach = shortest * 0.4;
            let mut step = (target - depth).clamp(-reach, reach);
            for _ in 0..4 {
                if step.abs() < 0.25 {
                    break;
                }
                let q = snap(add(p, mul(normal(g), step)));
                let acceptable = in_domain(q, f.loops)
                    && f.index.distance(q) >= 0.5 * target
                    && incident[v].iter().all(|&t| {
                        let old = m.triangles[t].map(|i| m.points[i]);
                        let [a, b, c] =
                            m.triangles[t].map(|i| if i == v { q } else { m.points[i] });
                        cross(a, b, c) > EPS
                            && min_angle(a, b, c) >= 0.35_f64.min(min_angle(old[0], old[1], old[2]))
                    });
                if acceptable {
                    m.points[v] = q;
                    break;
                }
                step *= 0.5;
            }
        }
    }
}
fn in_triangle(p: Point, t: [Point; 3]) -> bool {
    cross(t[0], t[1], p) >= -EPS && cross(t[1], t[2], p) >= -EPS && cross(t[2], t[0], p) >= -EPS
}
pub(super) fn generate(
    loops: &[Vec<Point>],
    kind: MeshFillAlgorithm,
    p: FillParameters,
    spacing: f64,
    suppress: bool,
) -> Option<Mesh> {
    if !valid_domain(loops) {
        return None;
    }
    let b = bounds(loops);
    let pivot = [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5];
    let angle = if kind == MeshFillAlgorithm::ContourPaving {
        0.
    } else {
        p.angle.to_radians()
    };
    let rotate = |v: Point, a: f64| {
        let d = sub(v, pivot);
        snap(add(
            pivot,
            [
                d[0] * a.cos() - d[1] * a.sin(),
                d[0] * a.sin() + d[1] * a.cos(),
            ],
        ))
    };
    // Rotate the sampling field and patterned core, then triangulate against the
    // original contour. Rotating a completed triangulation back can collapse
    // tiny faces along originally collinear boundary stations at f32 precision.
    let local: Loops = loops
        .iter()
        .map(|r| r.iter().map(|&v| rotate(v, -angle)).collect())
        .collect();
    let field = Field::new(&local, spacing, p);
    let mut layout = Mesh {
        points: local.iter().flatten().copied().collect(),
        triangles: vec![],
    };
    let mut free = local.clone();
    let mut free_ids = loop_ids(&local);
    if kind == MeshFillAlgorithm::ContourPaving {
        pave(&field, p, &mut layout, &mut free_ids);
        free = free_ids
            .iter()
            .map(|r| r.iter().map(|&i| layout.points[i]).collect())
            .collect();
    }
    let core_field = Field::new(&free, field.coarse, p);
    let core = lattice_core(&core_field, kind);
    let shift = layout.points.len();
    layout.points.extend(core.points);
    layout
        .triangles
        .extend(core.triangles.into_iter().map(|t| t.map(|i| i + shift)));
    let mut candidates: Vec<_> = fill::hex(&free, core_field.first, 1.)
        .into_iter()
        .filter(|&q| {
            !layout
                .triangles
                .iter()
                .any(|t| in_triangle(q, t.map(|i| layout.points[i])))
        })
        .collect();
    let originals: Vec<_> = loops.iter().flatten().copied().collect();
    for (i, v) in layout.points.iter_mut().enumerate() {
        *v = if i < originals.len() {
            originals[i]
        } else {
            rotate(*v, angle)
        };
    }
    for v in &mut candidates {
        *v = rotate(*v, angle);
    }
    let Some(mut mesh) =
        triangulate::with_layout(loops, &layout, &candidates, core_field.first, suppress)
    else {
        return triangulate::triangulate(
            loops,
            &layout.points[loops.iter().map(Vec::len).sum::<usize>()..],
            spacing,
            suppress,
        );
    };
    let boundary_count = loops.iter().map(Vec::len).sum::<usize>();
    let max_fan = 4.max(p.edge_ratio.ceil() as usize + 2);
    let contour_spacing = median_spacing(loops);
    for _ in 0..3 {
        let mut neighbors: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for &t in &mesh.triangles {
            for v in t {
                neighbors.entry(v).or_default().extend(t);
            }
        }
        let mut seeds = vec![];
        for (v, around) in neighbors {
            if v < boundary_count {
                continue;
            }
            let contour: Vec<_> = around.into_iter().filter(|&i| i < boundary_count).collect();
            if contour.len() <= max_fan {
                continue;
            }
            let center = mul(
                contour.iter().map(|&i| mesh.points[i]).fold([0., 0.], add),
                1. / contour.len() as f64,
            );
            let reach = distance(center, mesh.points[v]);
            let depth = (core_field.first * H).min(reach * 0.5);
            if depth < contour_spacing {
                continue;
            }
            let seed = lerp(center, mesh.points[v], depth / reach);
            if !layout
                .triangles
                .iter()
                .any(|t| in_triangle(seed, t.map(|i| layout.points[i])))
            {
                seeds.push(seed);
            }
        }
        if seeds.is_empty() {
            break;
        }
        candidates.extend(seeds);
        if let Some(rebuilt) =
            triangulate::with_layout(loops, &layout, &candidates, core_field.first, suppress)
        {
            mesh = rebuilt;
        } else {
            break;
        }
    }
    Some(mesh)
}

fn pave(field: &Field<'_>, p: FillParameters, mesh: &mut Mesh, rows: &mut [Vec<usize>]) {
    let mut depths = vec![0.; rows.len()];
    let mut active = vec![true; rows.len()];
    for _ in 0..p.max_rows {
        for i in 0..rows.len() {
            if !active[i] {
                continue;
            }
            if let Some((points, anchors, step)) = advance(field, mesh, rows, i, depths[i]) {
                let ids: Vec<_> = (mesh.points.len()..mesh.points.len() + points.len()).collect();
                mesh.points.extend(points);
                mesh.triangles
                    .extend(bands::zip_rows(&rows[i], &ids, &anchors));
                rows[i] = ids;
                depths[i] += step;
            } else {
                active[i] = false
            }
        }
    }
}
fn advance(
    field: &Field<'_>,
    mesh: &Mesh,
    rows: &[Vec<usize>],
    i: usize,
    depth: f64,
) -> Option<(Vec<Point>, Vec<usize>, f64)> {
    let row: Vec<_> = rows[i].iter().map(|&v| mesh.points[v]).collect();
    let n = row.len();
    if n < 6 {
        return None;
    }
    let segments: Vec<_> = (0..n).map(|i| distance(row[i], row[(i + 1) % n])).collect();
    let spacing = segments.iter().sum::<f64>() / n as f64;
    let target = field.target(depth);
    let coarsen = n >= 12 && target >= spacing * 1.41 && spacing * 2. <= field.coarse * 1.41;
    if !coarsen && target >= field.coarse * 0.95 {
        return None;
    }
    let reach = if coarsen { 2. * H } else { H };
    let normals = bands::normals(&row, (2. * reach * reach).ceil().max(2.) as usize);
    let m = if coarsen { n / 2 } else { n };
    let anchors: Vec<_> = (0..m)
        .map(|j| if coarsen { (2 * j + 1) % n } else { j })
        .collect();
    let bases: Vec<_> = anchors
        .iter()
        .map(|&a| {
            if coarsen {
                row[a]
            } else {
                lerp(row[a], row[(a + 1) % n], 0.5)
            }
        })
        .collect();
    let ns: Vec<_> = anchors
        .iter()
        .map(|&a| {
            if coarsen {
                normals[a]
            } else {
                normal(add(normals[a], normals[(a + 1) % n]))
            }
        })
        .collect();
    let nominal: Vec<_> = anchors
        .iter()
        .map(|&a| {
            if coarsen {
                2. * H * ((segments[(a + n - 1) % n] + segments[a]) * 0.5 + spacing) * 0.5
            } else {
                H * (segments[a] + spacing) * 0.5
            }
        })
        .collect();
    let loops: Loops = rows
        .iter()
        .map(|r| r.iter().map(|&v| mesh.points[v]).collect())
        .collect();
    let idx = SegmentIndex::new(&loops, spacing);
    let mut amounts: Vec<_> = (0..m)
        .map(|j| nominal[j].min(0.45 * idx.ray(bases[j], ns[j], nominal[j] / 0.45 * 1.01 + 1e-6)))
        .collect();
    let arcs: Vec<_> = (0..m)
        .map(|j| distance(bases[j], bases[(j + 1) % m]))
        .collect();
    bands::min_smooth(&mut amounts, &arcs, 0.4);
    if (0..m).any(|j| amounts[j] < nominal[j] * 0.6) {
        return None;
    }
    let pts: Vec<_> = (0..m)
        .map(|j| snap(add(bases[j], mul(ns[j], amounts[j]))))
        .collect();
    if !simple(&pts)
        || loops
            .iter()
            .enumerate()
            .any(|(j, r)| j != i && touches(&pts, r))
    {
        return None;
    }
    let local: Vec<_> = row.iter().chain(&pts).copied().collect();
    for t in bands::zip_rows(
        &(0..n).collect::<Vec<_>>(),
        &(n..n + m).collect::<Vec<_>>(),
        &anchors,
    ) {
        let [a, b, c] = t.map(|i| local[i]);
        if cross(a, b, c)
            <= distance(a, b)
                .max(distance(b, c))
                .max(distance(c, a))
                .powi(2)
                * 0.08
        {
            return None;
        }
    }
    Some((pts, anchors, amounts.iter().sum::<f64>() / m as f64))
}
