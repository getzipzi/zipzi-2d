// SPDX-License-Identifier: GPL-3.0-only
// PSD2Live's domain triangulation/refinement policy, using spade's robust CDT.
use super::geometry::*;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;
pub(super) fn triangulate(
    loops: &[Vec<Point>],
    candidates: &[Point],
    spacing: f64,
    suppress: bool,
) -> Option<Mesh> {
    with_layout(
        loops,
        &Mesh {
            points: loops.iter().flatten().copied().collect(),
            triangles: vec![],
        },
        candidates,
        spacing,
        suppress,
    )
}
pub(super) fn with_layout(
    loops: &[Vec<Point>],
    layout: &Mesh,
    candidates: &[Point],
    spacing: f64,
    suppress: bool,
) -> Option<Mesh> {
    with_edges(loops, layout, candidates, spacing, suppress, &[])
}
pub(super) fn with_edges(
    loops: &[Vec<Point>],
    layout: &Mesh,
    candidates: &[Point],
    spacing: f64,
    suppress: bool,
    fixed_edges: &[[usize; 2]],
) -> Option<Mesh> {
    if !valid_domain(loops) {
        return None;
    }
    let mut pts: Vec<_> = layout.points.iter().copied().map(snap).collect();
    let boundary_count = loops.iter().map(Vec::len).sum::<usize>();
    let ids = loop_ids(loops);
    let mut constraints: Vec<_> = ids
        .iter()
        .flat_map(|r| (0..r.len()).map(|i| [r[i], r[(i + 1) % r.len()]]))
        .collect();
    constraints.extend(layout.triangles.iter().flat_map(|t| edges(*t)));
    constraints.extend_from_slice(fixed_edges);
    let mut conflict = false;
    let mut cdt = Cdt::try_bulk_load_cdt(
        pts.iter().map(|p| Point2::new(p[0], p[1])).collect(),
        constraints,
        |_| conflict = true,
    )
    .ok()?;
    if conflict || cdt.num_vertices() != pts.len() {
        return None;
    }
    let index = SegmentIndex::new(loops, spacing);
    // A polygon ray test is ambiguous on its boundary. In particular, repeatedly
    // splitting an exterior chord whose midpoint rounds onto a straight contour
    // creates slivers (and eventually subnormal coordinates). Keep free sites a
    // small, scale-aware distance from the constraints at f32 output precision.
    let extent = bounds(loops).into_iter().map(f64::abs).fold(1., f64::max);
    let clearance = (extent * f32::EPSILON as f64 * 16.).max(1e-4);
    for &p in candidates {
        let p = snap(p);
        if in_domain(p, loops) && index.distance(p) > 1. {
            cdt.insert(Point2::new(p[0], p[1])).ok()?;
        }
    }
    // Only free interior edges are refined, never the sampled boundary or band.
    // Authored contours can have very long fixed sides. Repeatedly bisecting
    // their adjacent free edges cannot shorten those sides; it only crowds
    // points against them until triangles collapse at output precision.
    let refinement_clearance = clearance.max(spacing * 0.25);
    let mut budget = 512;
    for _ in 0..4 {
        let mut inserted = 0;
        while budget > 0 {
            let longest = cdt
                .undirected_edges()
                .filter(|e| !e.is_constraint_edge())
                .filter_map(|e| {
                    let [a, b] = e.vertices().map(|v| [v.position().x, v.position().y]);
                    let mid = snap(lerp(a, b, 0.5));
                    (distance(a, b) > spacing * 1.72
                        && in_domain(mid, loops)
                        && index.distance(mid) > refinement_clearance)
                        .then_some((distance(a, b), mid))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0));
            let Some((_, p)) = longest else { break };
            let before = cdt.num_vertices();
            cdt.insert(Point2::new(p[0], p[1])).ok()?;
            if cdt.num_vertices() == before {
                break;
            }
            budget -= 1;
            inserted += 1;
        }
        if inserted == 0 {
            break;
        }
    }
    if suppress {
        // Explicit midpoint splits prevent boundary-to-boundary chords.
        let mut guard = 0;
        loop {
            let diagonal = cdt
                .undirected_edges()
                .find(|e| {
                    let v = e.vertices();
                    !e.is_constraint_edge() && v.iter().all(|v| v.index() < boundary_count) && {
                        let p = v.map(|v| [v.position().x, v.position().y]);
                        in_domain(lerp(p[0], p[1], 0.5), loops)
                            && index.distance(lerp(p[0], p[1], 0.5)) > clearance
                    }
                })
                .map(|e| e.vertices().map(|v| [v.position().x, v.position().y]));
            let Some([a, b]) = diagonal else { break };
            let p = snap(lerp(a, b, 0.5));
            let before = cdt.num_vertices();
            cdt.insert(Point2::new(p[0], p[1])).ok()?;
            guard += 1;
            if before == cdt.num_vertices() || guard > 4096 {
                return None;
            }
        }
    }
    pts = cdt
        .vertices()
        .map(|v| [v.position().x, v.position().y])
        .collect();
    let triangles = cdt
        .inner_faces()
        .filter_map(|f| {
            let ids = f.vertices().map(|v| v.index());
            let [a, b, c] = ids.map(|i| pts[i]);
            in_domain(mul(add(add(a, b), c), 1. / 3.), loops).then_some(if cross(a, b, c) > 0. {
                ids
            } else {
                [ids[0], ids[2], ids[1]]
            })
        })
        .collect();
    let mesh = Mesh {
        points: pts,
        triangles,
    };
    valid_mesh(&mesh, &ids).then_some(mesh)
}
