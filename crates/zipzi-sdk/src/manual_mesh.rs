//! Pure, atomic edits of a single UV triangulation. No document or UI state lives here.
use crate::{MeshGeometry, SdkError};
use rstar::{RTree, RTreeObject, AABB};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use zipzi_core::{
    geometry::{validate_mesh_meshing, validate_render_mesh},
    MeshConstraint, MeshConstraintKind, MeshMeshing, Vec2, VertexId,
};

type Edge = [VertexId; 2];
type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

/// Keep one allocator per editable mesh across candidate cancellation. Reserved IDs are
/// consumed even when an operation later fails, so a stale candidate ID never names a new point.
#[derive(Debug)]
pub struct ManualVertexIdAllocator {
    next: u64,
}
impl ManualVertexIdAllocator {
    pub fn new(geometry: &MeshGeometry) -> Self {
        Self {
            next: geometry
                .vertex_ids
                .iter()
                .copied()
                .max()
                .map_or(0, |id| u64::from(id) + 1),
        }
    }
    pub fn allocate(&mut self, geometry: &MeshGeometry) -> Result<VertexId, SdkError> {
        self.next = self.next.max(
            geometry
                .vertex_ids
                .iter()
                .copied()
                .max()
                .map_or(0, |id| u64::from(id) + 1),
        );
        let id = VertexId::try_from(self.next).map_err(|_| bad("vertex ID space exhausted"))?;
        self.next += 1;
        Ok(id)
    }
}

fn edge(a: VertexId, b: VertexId) -> Edge {
    [a.min(b), a.max(b)]
}
fn bad(message: impl Into<String>) -> SdkError {
    SdkError::new("INVALID_MANUAL_MESH", &message.into(), "edit_manual_mesh")
}
fn xy(p: Vec2) -> [f64; 2] {
    [p.x as f64, p.y as f64]
}
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn between(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> bool {
    // This only identifies an existing f32 site on a segment. Core performs the
    // scale-aware rejection of near misses and indistinguishable geometry.
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    cross(a, b, p).abs() <= len * 1e-12_f64.max(len * f64::EPSILON * 16.)
        && (0..2).all(|i| p[i] >= a[i].min(b[i]) && p[i] <= a[i].max(b[i]))
}
fn proper_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (u, v, w, z) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (u > 0. && v < 0. || u < 0. && v > 0.) && (w > 0. && z < 0. || w < 0. && z > 0.)
}

fn det(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
fn add_linear_root(times: &mut Vec<f64>, a: f64, b: f64) {
    if b != 0. {
        let t = -a / b;
        if t > 0. && t < 1. {
            times.push(t);
        }
    }
}
fn add_orient_roots(times: &mut Vec<f64>, a: [[f64; 2]; 2], b: [[f64; 2]; 2], c: [[f64; 2]; 2]) {
    let ab = sub(b[0], a[0]);
    let ac = sub(c[0], a[0]);
    let dab = sub(sub(b[1], b[0]), sub(a[1], a[0]));
    let dac = sub(sub(c[1], c[0]), sub(a[1], a[0]));
    let (p, q, r) = (det(dab, dac), det(ab, dac) + det(dab, ac), det(ab, ac));
    if p == 0. {
        add_linear_root(times, r, q);
    } else {
        let disc = q * q - 4. * p * r;
        if disc >= 0. {
            let root = disc.sqrt();
            for t in [(-q - root) / (2. * p), (-q + root) / (2. * p)] {
                if t > 0. && t < 1. {
                    times.push(t);
                }
            }
        }
    }
}
fn swept_intersect(a: [[f64; 2]; 2], b: [[f64; 2]; 2], c: [[f64; 2]; 2], d: [[f64; 2]; 2]) -> bool {
    let mut times = vec![0., 1.];
    for (x, y, z) in [(a, b, c), (a, b, d), (c, d, a), (c, d, b)] {
        add_orient_roots(&mut times, x, y, z);
    }
    // When two segments remain collinear, overlap changes at an endpoint meeting.
    for x in [a, b] {
        for y in [c, d] {
            for axis in 0..2 {
                add_linear_root(
                    &mut times,
                    x[0][axis] - y[0][axis],
                    (x[1][axis] - x[0][axis]) - (y[1][axis] - y[0][axis]),
                );
            }
        }
    }
    times.sort_by(f64::total_cmp);
    times.dedup();
    for window in times.windows(2) {
        for t in [window[0], (window[0] + window[1]) * 0.5] {
            if t == 0. {
                continue;
            }
            let (a, b, c, d) = (
                lerp(a[0], a[1], t),
                lerp(b[0], b[1], t),
                lerp(c[0], c[1], t),
                lerp(d[0], d[1], t),
            );
            if proper_cross(a, b, c, d)
                || between(a, c, d)
                || between(b, c, d)
                || between(c, a, b)
                || between(d, a, b)
            {
                return true;
            }
        }
    }
    false
}

fn motion_crossing(
    edges: &BTreeSet<Edge>,
    old: &HashMap<VertexId, [f64; 2]>,
    from: &HashMap<VertexId, Vec2>,
    targets: &HashMap<VertexId, Vec2>,
) -> Option<(Edge, Edge)> {
    #[derive(Clone)]
    struct SweptEdge {
        ids: Edge,
        index: usize,
        points: [[[f64; 2]; 2]; 2],
        moving: bool,
        bounds: AABB<[f64; 2]>,
    }
    impl RTreeObject for SweptEdge {
        type Envelope = AABB<[f64; 2]>;
        fn envelope(&self) -> Self::Envelope {
            self.bounds
        }
    }
    let trajectory = |id: VertexId| {
        let start = from.get(&id).copied().map(xy).unwrap_or(old[&id]);
        let end = targets.get(&id).copied().map(xy).unwrap_or(old[&id]);
        [start, end]
    };
    let tree = RTree::bulk_load(
        edges
            .iter()
            .copied()
            .enumerate()
            .map(|(index, ids)| {
                let points = [trajectory(ids[0]), trajectory(ids[1])];
                let bounds = AABB::from_corners(
                    [
                        points
                            .iter()
                            .flat_map(|p| p.iter())
                            .map(|p| p[0])
                            .fold(f64::INFINITY, f64::min),
                        points
                            .iter()
                            .flat_map(|p| p.iter())
                            .map(|p| p[1])
                            .fold(f64::INFINITY, f64::min),
                    ],
                    [
                        points
                            .iter()
                            .flat_map(|p| p.iter())
                            .map(|p| p[0])
                            .fold(f64::NEG_INFINITY, f64::max),
                        points
                            .iter()
                            .flat_map(|p| p.iter())
                            .map(|p| p[1])
                            .fold(f64::NEG_INFINITY, f64::max),
                    ],
                );
                SweptEdge {
                    ids,
                    index,
                    points,
                    moving: ids.iter().any(|id| targets.contains_key(id)),
                    bounds,
                }
            })
            .collect(),
    );
    for a in &tree {
        if !a.moving {
            continue;
        }
        for b in tree.locate_in_envelope_intersecting(&a.bounds) {
            if a.index == b.index
                || (b.moving && b.index < a.index)
                || a.ids.iter().any(|id| b.ids.contains(id))
            {
                continue;
            }
            if swept_intersect(a.points[0], a.points[1], b.points[0], b.points[1]) {
                return Some((a.ids, b.ids));
            }
        }
    }
    None
}
fn motion_face_collapse(
    triangles: &[[VertexId; 3]],
    old: &HashMap<VertexId, [f64; 2]>,
    from: &HashMap<VertexId, Vec2>,
    targets: &HashMap<VertexId, Vec2>,
) -> Option<[VertexId; 3]> {
    let trajectory = |id: VertexId| {
        [
            from.get(&id).copied().map(xy).unwrap_or(old[&id]),
            targets.get(&id).copied().map(xy).unwrap_or(old[&id]),
        ]
    };
    for &triangle in triangles {
        if !triangle.iter().any(|id| targets.contains_key(id)) {
            continue;
        }
        let [a, b, c] = triangle.map(trajectory);
        let mut roots = Vec::new();
        add_orient_roots(&mut roots, a, b, c);
        if !roots.is_empty() {
            return Some(triangle);
        }
    }
    None
}
fn uv_map(g: &MeshGeometry) -> HashMap<VertexId, [f64; 2]> {
    g.vertex_ids
        .iter()
        .copied()
        .zip(g.uvs.iter().copied().map(xy))
        .collect()
}
fn polygon_contains(p: [f64; 2], ids: &[VertexId], points: &HashMap<VertexId, [f64; 2]>) -> bool {
    let mut inside = false;
    for i in 0..ids.len() {
        let a = points[&ids[i]];
        let b = points[&ids[(i + 1) % ids.len()]];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
        {
            inside = !inside;
        }
    }
    inside
}
fn mesh_edges(triangles: &[[VertexId; 3]]) -> BTreeSet<Edge> {
    triangles
        .iter()
        .flat_map(|t| (0..3).map(|i| edge(t[i], t[(i + 1) % 3])))
        .collect()
}
fn boundary_edges(ids: &[VertexId]) -> BTreeSet<Edge> {
    (0..ids.len())
        .map(|i| edge(ids[i], ids[(i + 1) % ids.len()]))
        .collect()
}
fn canonical_boundary(ids: &mut Vec<VertexId>, points: &HashMap<VertexId, [f64; 2]>) {
    let area: f64 = (0..ids.len())
        .map(|i| {
            let a = points[&ids[i]];
            let b = points[&ids[(i + 1) % ids.len()]];
            a[0] * b[1] - a[1] * b[0]
        })
        .sum();
    if area < 0. {
        ids.reverse();
    }
    if let Some(i) = ids
        .iter()
        .enumerate()
        .min_by_key(|(_, id)| *id)
        .map(|(i, _)| i)
    {
        ids.rotate_left(i);
    }
}

/// Recover a fixed image-to-authoring affine transform from the existing mesh.
/// A reflection is valid. Every vertex must obey the same transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AffineUvMap {
    pub origin: Vec2,
    pub u: Vec2,
    pub v: Vec2,
}
impl AffineUvMap {
    pub fn from_geometry(g: &MeshGeometry) -> Result<Self, SdkError> {
        if g.uvs.len() != g.positions.len() || g.uvs.len() < 3 {
            return Err(bad("affine mapping requires matching UVs and positions"));
        }
        let a = xy(g.uvs[0]);
        let pa = xy(g.positions[0]);
        // Use a wide triangle rather than the first three slots, which may be
        // nearly collinear samples along a long boundary edge.
        let i = (1..g.uvs.len())
            .max_by(|&i, &j| {
                let p = xy(g.uvs[i]);
                let q = xy(g.uvs[j]);
                ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2))
                    .total_cmp(&((q[0] - a[0]).powi(2) + (q[1] - a[1]).powi(2)))
            })
            .unwrap();
        let b = xy(g.uvs[i]);
        let j = (0..g.uvs.len())
            .max_by(|&i, &j| {
                cross(a, b, xy(g.uvs[i]))
                    .abs()
                    .total_cmp(&cross(a, b, xy(g.uvs[j])).abs())
            })
            .unwrap();
        let c = xy(g.uvs[j]);
        let det = cross(a, b, c);
        if det.abs() < 1e-12 {
            return Err(bad("UVs cannot determine an affine image mapping"));
        }
        let pb = xy(g.positions[i]);
        let pc = xy(g.positions[j]);
        let du = [b[0] - a[0], b[1] - a[1]];
        let dv = [c[0] - a[0], c[1] - a[1]];
        let x = [pb[0] - pa[0], pb[1] - pa[1]];
        let y = [pc[0] - pa[0], pc[1] - pa[1]];
        let u = [
            (x[0] * dv[1] - y[0] * du[1]) / det,
            (x[1] * dv[1] - y[1] * du[1]) / det,
        ];
        let v = [
            (y[0] * du[0] - x[0] * dv[0]) / det,
            (y[1] * du[0] - x[1] * dv[0]) / det,
        ];
        let origin = [
            pa[0] - u[0] * a[0] - v[0] * a[1],
            pa[1] - u[1] * a[0] - v[1] * a[1],
        ];
        let map = Self {
            origin: Vec2::new(origin[0] as f32, origin[1] as f32),
            u: Vec2::new(u[0] as f32, u[1] as f32),
            v: Vec2::new(v[0] as f32, v[1] as f32),
        };
        let pd = (u[0] * v[1] - u[1] * v[0]).abs();
        if pd <= 1e-12 {
            return Err(bad("image mapping is not invertible"));
        }
        let scale = pa
            .iter()
            .chain(pb.iter())
            .chain(pc.iter())
            .fold(1.0_f64, |m, x| m.max(x.abs()));
        if g.uvs.iter().zip(&g.positions).all(|(&uv, &p)| {
            let q = map.position(uv);
            (q.x as f64 - p.x as f64).abs() <= scale * 2e-5
                && (q.y as f64 - p.y as f64).abs() <= scale * 2e-5
        }) {
            return Ok(map);
        }
        return Err(bad(
            "positions and UVs do not share one affine image mapping",
        ));
    }
    pub fn position(self, uv: Vec2) -> Vec2 {
        Vec2::new(
            ((self.origin.x as f64)
                + (self.u.x as f64) * (uv.x as f64)
                + (self.v.x as f64) * (uv.y as f64)) as f32,
            ((self.origin.y as f64)
                + (self.u.y as f64) * (uv.x as f64)
                + (self.v.y as f64) * (uv.y as f64)) as f32,
        )
    }
    pub fn uv(self, position: Vec2) -> Result<Vec2, SdkError> {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(bad("authoring position must be finite"));
        }
        let (u, v, p) = (xy(self.u), xy(self.v), sub(xy(position), xy(self.origin)));
        let determinant = det(u, v);
        if determinant == 0. || !determinant.is_finite() {
            return Err(bad("image mapping is not invertible"));
        }
        let uv = Vec2::new(
            (det(p, v) / determinant) as f32,
            (det(u, p) / determinant) as f32,
        );
        if !uv.x.is_finite() || !uv.y.is_finite() {
            return Err(bad("inverse image mapping overflows UV precision"));
        }
        Ok(uv)
    }
}

/// Establish editable topology without modifying or reordering the input faces.
pub fn extract_manual_meshing(g: &MeshGeometry) -> Result<MeshMeshing, SdkError> {
    if let Some(m) = &g.meshing {
        validate(g, m)?;
        return Ok(m.clone());
    }
    let mut incidence = BTreeMap::<Edge, Vec<(VertexId, VertexId)>>::new();
    for t in &g.triangles {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            incidence.entry(edge(a, b)).or_default().push((a, b));
        }
    }
    let directed: Vec<_> = incidence
        .values()
        .filter(|uses| uses.len() == 1)
        .map(|uses| uses[0])
        .collect();
    if directed.len() < 3 {
        return Err(bad("mesh has no single closed outer boundary"));
    }
    let mut next = HashMap::new();
    for (a, b) in directed {
        if next.insert(a, b).is_some() {
            return Err(bad("mesh boundary branches"));
        }
    }
    let start = *next.keys().min().unwrap();
    let mut boundary = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current = start;
    loop {
        if !visited.insert(current) {
            return Err(bad("mesh boundary repeats a vertex"));
        }
        boundary.push(current);
        current = *next
            .get(&current)
            .ok_or_else(|| bad("mesh boundary is open"))?;
        if current == start {
            break;
        }
    }
    if boundary.len() != next.len() {
        return Err(bad("mesh has holes or separate regions"));
    }
    canonical_boundary(&mut boundary, &uv_map(g));
    let meshing = MeshMeshing {
        boundary,
        constraints: Vec::new(),
    };
    validate(g, &meshing)?;
    Ok(meshing)
}

fn validate(g: &MeshGeometry, m: &MeshMeshing) -> Result<(), SdkError> {
    let status = validate_mesh_meshing(&g.vertex_ids, &g.uvs, &g.triangles, m);
    if !status.is_ok() {
        return Err(bad(status.message));
    }
    if g.positions.len() != g.vertex_ids.len() {
        return Err(bad("vertex IDs and positions have different lengths"));
    }
    let slots: HashMap<_, _> = g
        .vertex_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i as u32))
        .collect();
    let indices: Vec<_> = g
        .triangles
        .iter()
        .flat_map(|t| t.iter().map(|id| slots[id]))
        .collect();
    let render = validate_render_mesh(&g.positions, &g.uvs, &indices);
    if !render.is_ok() {
        return Err(bad(render.message));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManualMeshOperation {
    /// The caller supplies a fresh ID; its allocator must not reuse canceled candidate IDs.
    InsertVertex {
        id: VertexId,
        uv: Vec2,
    },
    /// Absolute UV targets; motion_from is the last accepted preview when supplied.
    MoveVertices {
        targets: Vec<(VertexId, Vec2)>,
        motion_from: Option<Vec<(VertexId, Vec2)>>,
    },
    DeleteVertices {
        ids: Vec<VertexId>,
    },
    ConstrainEdge {
        a: VertexId,
        b: VertexId,
    },
    ReleaseConstraint {
        a: VertexId,
        b: VertexId,
    },
    Reconnect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualMeshResult {
    pub geometry: MeshGeometry,
    pub vertex_mapping: Vec<(VertexId, Option<VertexId>)>,
    pub added: Vec<VertexId>,
    pub removed: Vec<VertexId>,
    pub affected_edges: Vec<[VertexId; 2]>,
    pub changed: bool,
}

fn split_edges(m: &mut MeshMeshing, a: VertexId, b: VertexId, id: VertexId) {
    if let Some(i) = (0..m.boundary.len())
        .find(|&i| edge(m.boundary[i], m.boundary[(i + 1) % m.boundary.len()]) == edge(a, b))
    {
        m.boundary.insert(i + 1, id);
    }
    if let Some(i) = m.constraints.iter().position(|c| c.endpoints == edge(a, b)) {
        let kind = m.constraints.remove(i).kind;
        m.constraints.extend([
            MeshConstraint {
                endpoints: edge(a, id),
                kind,
            },
            MeshConstraint {
                endpoints: edge(id, b),
                kind,
            },
        ]);
        m.constraints.sort_by_key(|c| c.endpoints);
    }
}

fn triangulate(
    g: &MeshGeometry,
    m: &MeshMeshing,
    preserved: &BTreeSet<Edge>,
) -> Result<Vec<[VertexId; 3]>, SdkError> {
    let points = uv_map(g);
    let mut cdt = Cdt::new();
    let mut handles = HashMap::new();
    let mut reverse = HashMap::new();
    let mut ids = g.vertex_ids.clone();
    ids.sort_unstable();
    for id in ids {
        let p = points[&id];
        let h = cdt
            .insert(Point2::new(p[0], p[1]))
            .map_err(|e| bad(e.to_string()))?;
        if reverse.insert(h.index(), id).is_some() {
            return Err(bad(format!("vertex {id}: overlapping UV")));
        }
        handles.insert(id, h);
    }
    let boundary = boundary_edges(&m.boundary);
    let structural: BTreeSet<_> = m.constraints.iter().map(|c| c.endpoints).collect();
    let required: BTreeSet<_> = boundary
        .union(&structural)
        .copied()
        .chain(preserved.iter().copied())
        .collect();
    for e in &required {
        if !handles.contains_key(&e[0]) || !handles.contains_key(&e[1]) {
            return Err(bad(format!("edge {e:?}: unknown endpoint")));
        }
        if cdt
            .try_add_constraint(handles[&e[0]], handles[&e[1]])
            .is_empty()
        {
            return Err(bad(format!("edge {e:?}: crosses a retained edge")));
        }
    }
    let old_sign = g
        .triangles
        .first()
        .and_then(|t| {
            let [a, b, c] = t.map(|id| points.get(&id).copied());
            Some(cross(a?, b?, c?))
        })
        .unwrap_or(1.0)
        .signum();
    let mut faces = Vec::new();
    for face in cdt.inner_faces() {
        let center = face.center();
        if !polygon_contains([center.x, center.y], &m.boundary, &points) {
            continue;
        }
        let mut t = face.vertices().map(|v| reverse[&v.index()]);
        if old_sign < 0. {
            t.swap(1, 2);
        }
        let i = (0..3).min_by_key(|&i| t[i]).unwrap();
        t.rotate_left(i);
        faces.push(t);
    }
    faces.sort_unstable();
    Ok(faces)
}

/// Apply a manual edit to an owned geometry value. Errors leave the source untouched.
pub fn edit_manual_mesh(
    source: &MeshGeometry,
    operation: ManualMeshOperation,
) -> Result<ManualMeshResult, SdkError> {
    let original = extract_manual_meshing(source)?;
    let mut g = source.clone();
    g.meshing = Some(original.clone());
    let mut m = original.clone();
    let old_edges = mesh_edges(&source.triangles);
    let mut preserved = old_edges.clone();
    let mut force_rebuild = false;
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut touched = BTreeSet::new();
    match operation {
        ManualMeshOperation::InsertVertex { id, uv } => {
            if g.vertex_ids.contains(&id) {
                return Err(bad(format!("vertex {id}: ID already exists")));
            }
            let map = AffineUvMap::from_geometry(&g)?;
            if !uv.x.is_finite()
                || !uv.y.is_finite()
                || !(0.0..=1.0).contains(&uv.x)
                || !(0.0..=1.0).contains(&uv.y)
            {
                return Err(bad("new UV lies outside the source image"));
            }
            let p = xy(uv);
            let points = uv_map(&g);
            if points
                .values()
                .any(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-7)
            {
                return Err(bad("new UV coincides with an existing vertex"));
            }
            let hit: Vec<_> = old_edges
                .iter()
                .copied()
                .filter(|e| between(p, points[&e[0]], points[&e[1]]))
                .collect();
            if hit.len() > 1 {
                return Err(bad("new UV touches multiple edges"));
            }
            if hit.is_empty() && !polygon_contains(p, &m.boundary, &points) {
                return Err(bad("new UV lies outside the boundary"));
            }
            if let Some([a, b]) = hit.first().copied() {
                split_edges(&mut m, a, b, id);
                preserved.remove(&edge(a, b));
            }
            g.vertex_ids.push(id);
            g.uvs.push(uv);
            g.positions.push(map.position(uv));
            added.push(id);
            touched.insert(id);
            force_rebuild = true;
        }
        ManualMeshOperation::MoveVertices {
            targets,
            motion_from,
        } => {
            let map = AffineUvMap::from_geometry(&g)?;
            let ids: BTreeSet<_> = targets.iter().map(|(id, _)| *id).collect();
            if ids.len() != targets.len() {
                return Err(bad("duplicate moved vertex ID"));
            }
            let old = uv_map(&g);
            for id in &ids {
                if !old.contains_key(id) {
                    return Err(bad(format!("vertex {id}: unknown ID")));
                }
            }
            let moved: BTreeSet<_> = targets
                .iter()
                .filter(|(id, to)| old[id] != xy(*to))
                .map(|(id, _)| *id)
                .collect();
            touched.extend(&moved);
            let from: HashMap<_, _> = motion_from
                .unwrap_or_else(|| {
                    targets
                        .iter()
                        .map(|(id, _)| (*id, Vec2::new(old[id][0] as f32, old[id][1] as f32)))
                        .collect()
                })
                .into_iter()
                .collect();
            if from.len() != ids.len() || from.keys().any(|id| !ids.contains(id)) {
                return Err(bad("motion start IDs do not match targets"));
            }
            if from.values().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
                return Err(bad("motion start UVs must be finite"));
            }
            for (id, to) in &targets {
                if !to.x.is_finite()
                    || !to.y.is_finite()
                    || !(0.0..=1.0).contains(&to.x)
                    || !(0.0..=1.0).contains(&to.y)
                {
                    return Err(bad(format!("vertex {id}: UV outside source image")));
                }
            }
            preserved.retain(|e| !moved.contains(&e[0]) && !moved.contains(&e[1]));
            let endpoints: HashMap<_, _> = targets.iter().copied().collect();
            if let Some((a, b)) = motion_crossing(&old_edges, &old, &from, &endpoints) {
                return Err(bad(format!("motion crosses edges {a:?} and {b:?}")));
            }
            if let Some(face) = motion_face_collapse(&g.triangles, &old, &from, &endpoints) {
                return Err(bad(format!("motion collapses triangle {face:?}")));
            }
            for (id, to) in targets {
                let i = g
                    .vertex_ids
                    .iter()
                    .position(|v| *v == id)
                    .ok_or_else(|| bad(format!("vertex {id}: unknown ID")))?;
                if moved.contains(&id) {
                    g.uvs[i] = to;
                    g.positions[i] = map.position(to);
                }
            }
            for c in &mut m.constraints {
                if (moved.contains(&c.endpoints[0]) || moved.contains(&c.endpoints[1]))
                    && c.kind == MeshConstraintKind::EdgeSupport
                {
                    c.kind = MeshConstraintKind::User;
                }
            }
            force_rebuild = !moved.is_empty();
        }
        ManualMeshOperation::DeleteVertices { ids } => {
            let drop: BTreeSet<_> = ids.into_iter().collect();
            touched.extend(&drop);
            for id in &drop {
                if !g.vertex_ids.contains(id) {
                    return Err(bad(format!("vertex {id}: unknown ID")));
                }
            }
            m.boundary.retain(|id| !drop.contains(id));
            if m.boundary.len() < 3 {
                return Err(bad("deletion leaves fewer than three boundary vertices"));
            }
            m.constraints
                .retain(|c| !drop.contains(&c.endpoints[0]) && !drop.contains(&c.endpoints[1]));
            for i in (0..g.vertex_ids.len()).rev() {
                if drop.contains(&g.vertex_ids[i]) {
                    removed.push(g.vertex_ids.remove(i));
                    g.uvs.remove(i);
                    g.positions.remove(i);
                }
            }
            removed.sort_unstable();
            preserved.retain(|e| !drop.contains(&e[0]) && !drop.contains(&e[1]));
            force_rebuild = !drop.is_empty();
        }
        ManualMeshOperation::ConstrainEdge { a, b } => {
            if a == b || !g.vertex_ids.contains(&a) || !g.vertex_ids.contains(&b) {
                return Err(bad(
                    "constraint endpoints must be distinct existing vertices",
                ));
            }
            let points = uv_map(&g);
            let pa = points[&a];
            let pb = points[&b];
            let mut chain: Vec<_> = g
                .vertex_ids
                .iter()
                .copied()
                .filter(|id| between(points[id], pa, pb))
                .collect();
            chain.sort_by(|x, y| {
                let px = points[x];
                let py = points[y];
                ((px[0] - pa[0]).powi(2) + (px[1] - pa[1]).powi(2))
                    .total_cmp(&((py[0] - pa[0]).powi(2) + (py[1] - pa[1]).powi(2)))
            });
            if chain.first() != Some(&a) || chain.last() != Some(&b) {
                return Err(bad("constraint chain cannot be resolved"));
            }
            let boundary = boundary_edges(&m.boundary);
            for pair in chain.windows(2) {
                let e = edge(pair[0], pair[1]);
                if boundary.contains(&e) {
                    continue;
                }
                match m.constraints.iter_mut().find(|c| c.endpoints == e) {
                    Some(c) => c.kind = MeshConstraintKind::User,
                    None => m.constraints.push(MeshConstraint {
                        endpoints: e,
                        kind: MeshConstraintKind::User,
                    }),
                }
            }
            m.constraints.sort_by_key(|c| c.endpoints);
            let desired: BTreeSet<_> = chain.windows(2).map(|p| edge(p[0], p[1])).collect();
            preserved.retain(|e| {
                desired.contains(e) || !proper_cross(points[&e[0]], points[&e[1]], pa, pb)
            });
            force_rebuild = desired.iter().any(|e| !old_edges.contains(e));
        }
        ManualMeshOperation::ReleaseConstraint { a, b } => {
            let e = edge(a, b);
            let before = m.constraints.len();
            m.constraints.retain(|c| c.endpoints != e);
            if before == m.constraints.len() {
                return Err(bad(format!("edge {e:?}: no structural constraint")));
            }
        }
        ManualMeshOperation::Reconnect => {
            preserved.clear();
            force_rebuild = true;
        }
    }
    if force_rebuild {
        g.triangles = triangulate(&g, &m, &preserved)?;
    }
    g.meshing = Some(m.clone());
    validate(&g, &m)?;
    if m == original
        && g.vertex_ids == source.vertex_ids
        && g.positions == source.positions
        && g.uvs == source.uvs
        && g.triangles == source.triangles
    {
        g = source.clone();
    }
    let changed = &g != source;
    let new_edges = mesh_edges(&g.triangles);
    let mut affected: BTreeSet<_> = old_edges
        .symmetric_difference(&new_edges)
        .copied()
        .collect();
    affected.extend(
        old_edges
            .union(&new_edges)
            .copied()
            .filter(|e| touched.contains(&e[0]) || touched.contains(&e[1])),
    );
    affected.extend(
        boundary_edges(&original.boundary)
            .symmetric_difference(&boundary_edges(&m.boundary))
            .copied(),
    );
    let old_constraints: BTreeMap<_, _> = original
        .constraints
        .iter()
        .map(|c| (c.endpoints, c.kind))
        .collect();
    let new_constraints: BTreeMap<_, _> = m
        .constraints
        .iter()
        .map(|c| (c.endpoints, c.kind))
        .collect();
    affected.extend(
        old_constraints
            .keys()
            .chain(new_constraints.keys())
            .filter(|e| old_constraints.get(*e) != new_constraints.get(*e))
            .copied(),
    );
    Ok(ManualMeshResult {
        vertex_mapping: source
            .vertex_ids
            .iter()
            .map(|&id| {
                (
                    id,
                    if removed.contains(&id) {
                        None
                    } else {
                        Some(id)
                    },
                )
            })
            .collect(),
        geometry: g,
        added,
        removed,
        affected_edges: affected.into_iter().collect(),
        changed,
    })
}
