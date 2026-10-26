//! Validation of the persisted UV triangulation, never of an evaluated pose.
use crate::{MeshMeshing, Status, Vec2, VertexId};
use robust::{Coord, orient2d};
use rstar::{AABB, RTree, RTreeObject};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type Point = [f64; 2];
type Edge = [VertexId; 2];

fn invalid(message: impl Into<String>) -> Status {
    Status::error("INVALID_MESH_MESHING", message)
}

fn edge(a: VertexId, b: VertexId) -> Edge {
    [a.min(b), a.max(b)]
}

fn orient(a: Point, b: Point, c: Point) -> f64 {
    orient2d(
        Coord { x: a[0], y: a[1] },
        Coord { x: b[0], y: b[1] },
        Coord { x: c[0], y: c[1] },
    )
}

fn length(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn on_segment(p: Point, a: Point, b: Point, tolerance: f64) -> bool {
    orient(a, b, p).abs() <= tolerance * length(a, b)
        && (0..2).all(|axis| {
            p[axis] >= a[axis].min(b[axis]) - tolerance
                && p[axis] <= a[axis].max(b[axis]) + tolerance
        })
}

#[derive(Clone)]
struct Segment {
    ids: Edge,
    a: Point,
    b: Point,
    envelope: AABB<Point>,
}
impl RTreeObject for Segment {
    type Envelope = AABB<Point>;
    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// Validate a canonical single-boundary triangulation. Call after render-mesh validation.
///
/// Robust f64 orientation predicates determine side/intersection signs. A shared tolerance of
/// two f32 ulps at the UV coordinate scale rejects geometry too close to distinguish reliably;
/// it has no relation to Editor hit radii or model-space scale/reflection. Planar edges, coherent
/// face orientation and exact boundary incidence prove coverage without rerunning triangulation.
pub fn validate_mesh_meshing(
    vertex_ids: &[VertexId],
    uvs: &[Vec2],
    triangles: &[[VertexId; 3]],
    meshing: &MeshMeshing,
) -> Status {
    if vertex_ids.len() != uvs.len() || uvs.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return invalid("meshing requires matching vertex IDs and finite UVs");
    }
    let points: HashMap<_, Point> = vertex_ids
        .iter()
        .zip(uvs)
        .map(|(&id, p)| (id, [p.x.into(), p.y.into()]))
        .collect();
    if points.len() != vertex_ids.len() {
        return invalid("meshing requires unique vertex IDs");
    }
    let boundary = &meshing.boundary;
    if boundary.len() < 3
        || boundary.iter().collect::<BTreeSet<_>>().len() != boundary.len()
        || boundary.first() != boundary.iter().min()
        || boundary.iter().any(|v| !points.contains_key(v))
    {
        return invalid(
            "boundary requires at least three distinct existing IDs, starting with the smallest ID",
        );
    }
    let scale = points
        .values()
        .flatten()
        .fold(0.0_f64, |s, v| s.max(v.abs()));
    let tolerance = scale * f64::from(f32::EPSILON) * 2.;
    let point_tree = RTree::bulk_load(points.values().copied().collect());
    for (&id, &p) in &points {
        let envelope = AABB::from_corners(
            [p[0] - tolerance, p[1] - tolerance],
            [p[0] + tolerance, p[1] + tolerance],
        );
        if point_tree
            .locate_in_envelope_intersecting(&envelope)
            .count()
            != 1
        {
            return invalid(format!(
                "vertex {id}: coincident or indistinguishable UV vertices"
            ));
        }
    }
    let boundary_edges: BTreeSet<_> = (0..boundary.len())
        .map(|i| edge(boundary[i], boundary[(i + 1) % boundary.len()]))
        .collect();
    let origin = points[&boundary[0]];
    let area: f64 = (1..boundary.len() - 1)
        .map(|i| orient(origin, points[&boundary[i]], points[&boundary[i + 1]]))
        .sum();
    if area
        <= tolerance
            * boundary
                .iter()
                .enumerate()
                .map(|(i, id)| length(points[id], points[&boundary[(i + 1) % boundary.len()]]))
                .sum::<f64>()
    {
        return invalid("boundary must have a reliable nonzero counterclockwise UV area");
    }

    // A coherent oriented two-manifold must use internal edges in opposite directions.
    let mut edges = BTreeMap::<Edge, (usize, i32)>::new();
    let mut faces = BTreeSet::new();
    let mut used = BTreeSet::<VertexId>::new();
    let mut winding = 0;
    for triangle in triangles {
        let mut key = *triangle;
        key.sort_unstable();
        if key[0] == key[1] || key[1] == key[2] || !faces.insert(key) {
            return invalid(format!(
                "triangle {triangle:?}: repeated vertex or duplicate face"
            ));
        }
        if triangle.iter().any(|id| !points.contains_key(id)) {
            return invalid(format!("triangle {triangle:?}: unknown vertex"));
        }
        let [a, b, c] = triangle.map(|id| points[&id]);
        let area = orient(a, b, c);
        if area.abs() <= tolerance * length(a, b).max(length(b, c)).max(length(c, a)) {
            return invalid(format!(
                "triangle {triangle:?}: degenerate or indistinguishable UV area"
            ));
        }
        let sign = if area > 0. { 1 } else { -1 };
        if winding != 0 && winding != sign {
            return invalid(format!("triangle {triangle:?}: inconsistent UV winding"));
        }
        winding = sign;
        used.extend(triangle.iter().copied());
        for i in 0..3 {
            let (a, b) = (triangle[i], triangle[(i + 1) % 3]);
            let (count, direction) = edges.entry(edge(a, b)).or_default();
            *count += 1;
            *direction += if a < b { 1 } else { -1 };
        }
    }
    if used.len() != vertex_ids.len() || triangles.is_empty() {
        return invalid("every meshing vertex must participate in a nondegenerate triangle");
    }
    for (&e, &(count, direction)) in &edges {
        if (boundary_edges.contains(&e) && count != 1)
            || (!boundary_edges.contains(&e) && (count != 2 || direction != 0))
        {
            return invalid(format!(
                "edge {e:?}: triangle adjacency does not match the boundary"
            ));
        }
    }
    for i in 0..boundary.len() {
        let (a, b) = (boundary[i], boundary[(i + 1) % boundary.len()]);
        if edges.get(&edge(a, b)) != Some(&(1, winding * if a < b { 1 } else { -1 })) {
            return invalid(format!(
                "boundary edge {:?}: missing or inconsistent face",
                edge(a, b)
            ));
        }
    }
    let mut previous = None;
    for constraint in &meshing.constraints {
        let e = constraint.endpoints;
        if e[0] >= e[1]
            || previous.is_some_and(|p| p >= e)
            || boundary_edges.contains(&e)
            || !edges.contains_key(&e)
        {
            return invalid(format!(
                "constraint {e:?}: expected a unique, sorted internal triangle edge"
            ));
        }
        previous = Some(e);
    }

    let segments: Vec<_> = edges
        .keys()
        .map(|&ids| {
            let (a, b) = (points[&ids[0]], points[&ids[1]]);
            Segment {
                ids,
                a,
                b,
                envelope: AABB::from_corners(
                    [a[0].min(b[0]) - tolerance, a[1].min(b[1]) - tolerance],
                    [a[0].max(b[0]) + tolerance, a[1].max(b[1]) + tolerance],
                ),
            }
        })
        .collect();
    let tree = RTree::bulk_load(segments);
    for a in &tree {
        for b in tree.locate_in_envelope_intersecting(&a.envelope) {
            if a.ids >= b.ids {
                continue;
            }
            // Including endpoint-on-edge tests rejects collinear overlap and unsplit T junctions.
            let touches = [(a.ids[0], a.a), (a.ids[1], a.b)]
                .iter()
                .any(|(id, p)| !b.ids.contains(id) && on_segment(*p, b.a, b.b, tolerance))
                || [(b.ids[0], b.a), (b.ids[1], b.b)]
                    .iter()
                    .any(|(id, p)| !a.ids.contains(id) && on_segment(*p, a.a, a.b, tolerance));
            let crosses = orient(a.a, a.b, b.a) * orient(a.a, a.b, b.b) < 0.
                && orient(b.a, b.b, a.a) * orient(b.a, b.b, a.b) < 0.;
            if touches || crosses {
                return invalid(format!(
                    "edges {:?} and {:?}: intersection, overlap or unsplit vertex",
                    a.ids, b.ids
                ));
            }
        }
    }
    Status::ok()
}

#[cfg(test)]
mod manual_mesh;
