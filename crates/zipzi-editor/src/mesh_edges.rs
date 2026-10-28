use gpui_kit::{Path, PathBuilder, Pixels, Point, point, px};
use zipzi_editor_core::ViewportPoint;

// PathBuilder tessellates with u16 indices. Independent straight strokes use four
// vertices each; keep ample headroom and bound the size of each temporary buffer.
const EDGES_PER_PATH: usize = 4096;

/// Paint equally styled mesh edges as disjoint subpaths, before vertex handles.
pub fn paint_edges(
    edges: &[(ViewportPoint, ViewportPoint)],
    origin: Point<Pixels>,
    paint: impl FnMut(Path<Pixels>),
) {
    paint_edges_with_width(edges, origin, 1., paint);
}

pub fn paint_edges_with_width(
    edges: &[(ViewportPoint, ViewportPoint)],
    origin: Point<Pixels>,
    width: f32,
    mut paint: impl FnMut(Path<Pixels>),
) {
    if !width.is_finite() || width <= 0. {
        return;
    }
    // These are independent straight, butt-capped strokes: two solid triangles
    // describe the same geometry as the general Lyon stroke tessellator. Keep
    // the existing chunk boundaries (including overlap/alpha behavior).
    for edges in edges.chunks(EDGES_PER_PATH) {
        let mut path = None;
        for (a, b) in edges {
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let length = dx.hypot(dy);
            if ![a.x, a.y, b.x, b.y, length].into_iter().all(f32::is_finite) || length == 0. {
                continue;
            }
            let normal = point(
                px(-dy / length * width * 0.5),
                px(dx / length * width * 0.5),
            );
            let a = origin + point(px(a.x), px(a.y));
            let b = origin + point(px(b.x), px(b.y));
            let path = path.get_or_insert_with(|| {
                let mut path = Path::new(a + normal);
                path.vertices.reserve(edges.len() * 6);
                path
            });
            let st = (point(0., 1.), point(0., 1.), point(0., 1.));
            // Match Lyon's triangle winding as well as its butt-cap coverage.
            path.push_triangle((a + normal, b + normal, a - normal), st);
            path.push_triangle((a - normal, b + normal, b - normal), st);
        }
        if let Some(mut path) = path {
            path.bounds = path.bounds.dilate(px(width.max(1.)));
            paint(path);
        }
    }
}

/// Retained as an opt-in benchmark/reference for the direct straight strokes.
pub fn paint_edges_tessellated(
    edges: &[(ViewportPoint, ViewportPoint)],
    origin: Point<Pixels>,
    paint: impl FnMut(Path<Pixels>),
) {
    paint_edges_with_width_tessellated(edges, origin, 1., paint);
}

pub fn paint_edges_with_width_tessellated(
    edges: &[(ViewportPoint, ViewportPoint)],
    origin: Point<Pixels>,
    width: f32,
    mut paint: impl FnMut(Path<Pixels>),
) {
    let build = |edges: &[(ViewportPoint, ViewportPoint)]| {
        let mut path = PathBuilder::stroke(px(width));
        for (a, b) in edges {
            path.move_to(origin + point(px(a.x), px(a.y)));
            path.line_to(origin + point(px(b.x), px(b.y)));
        }
        path.build().map(|mut path| {
            // GPUI copies the MSAA result through the path's bounds. Include
            // fractional edge coverage beyond the geometric bounds at 1x/2x.
            path.bounds = path.bounds.dilate(px(width.max(1.)));
            path
        })
    };
    for edges in edges.chunks(EDGES_PER_PATH) {
        match build(edges) {
            Ok(path) => paint(path),
            // Match the old per-edge failure behavior: one failed batch must not
            // discard all otherwise drawable edges in the chunk.
            Err(_) => {
                for edge in edges {
                    if let Ok(path) = build(std::slice::from_ref(edge)) {
                        paint(path);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers(path: &Path<Pixels>, p: Point<Pixels>) -> bool {
        path.vertices.chunks_exact(3).any(|t| {
            let cross = |a: Point<Pixels>, b: Point<Pixels>, c: Point<Pixels>| {
                (b.x - a.x).as_f32() * (c.y - a.y).as_f32()
                    - (b.y - a.y).as_f32() * (c.x - a.x).as_f32()
            };
            let [a, b, c] = [t[0].xy_position, t[1].xy_position, t[2].xy_position];
            if cross(a, b, c).abs() < 1e-8 {
                return false;
            }
            let sides = [cross(a, b, p), cross(b, c, p), cross(c, a, p)];
            sides.iter().all(|&v| v >= -1e-5) || sides.iter().all(|&v| v <= 1e-5)
        })
    }

    #[test]
    fn direct_edges_match_general_strokes_at_fractional_origins_and_widths() {
        let pt = |x, y| ViewportPoint { x, y };
        for (a, b) in [
            (pt(0., 0.), pt(10., 0.)),
            (pt(2., -7.), pt(2., 10.)),
            (pt(-5., -4.), pt(13.25, 8.5)),
            (pt(13.25, 8.5), pt(-5., -4.)),
        ] {
            for origin in [point(px(0.), px(0.)), point(px(-4.25), px(18.5))] {
                for width in [0.75, 1., 3.] {
                    let mut direct = Vec::new();
                    let mut reference = Vec::new();
                    paint_edges_with_width(&[(a, b)], origin, width, |p| direct.push(p));
                    paint_edges_with_width_tessellated(&[(a, b)], origin, width, |p| {
                        reference.push(p)
                    });
                    assert_eq!(direct.len(), 1);
                    assert_eq!(reference.len(), 1);
                    let (direct, reference) = (&direct[0], &reference[0]);
                    assert_eq!(direct.vertices.len(), 6);
                    for v in &direct.vertices {
                        assert_eq!(v.st_position, point(0., 1.));
                        assert!(direct.bounds.contains(&v.xy_position));
                    }
                    let area = |p: &Path<Pixels>| {
                        p.vertices
                            .chunks_exact(3)
                            .map(|t| {
                                let [a, b, c] =
                                    [t[0].xy_position, t[1].xy_position, t[2].xy_position];
                                ((b.x - a.x).as_f32() * (c.y - a.y).as_f32()
                                    - (b.y - a.y).as_f32() * (c.x - a.x).as_f32())
                                    * 0.5
                            })
                            .sum::<f32>()
                    };
                    assert!(
                        (area(direct) - area(reference)).abs() < 0.001,
                        "stroke area or winding changed: {} vs {}",
                        area(direct),
                        area(reference)
                    );
                    let bounds = direct.bounds.union(&reference.bounds);
                    for x in 0..31 {
                        for y in 0..31 {
                            let p = bounds.origin
                                + point(
                                    bounds.size.width * ((x as f32 + 0.317) / 31.),
                                    bounds.size.height * ((y as f32 + 0.413) / 31.),
                                );
                            assert_eq!(
                                covers(direct, p),
                                covers(reference, p),
                                "stroke coverage at {p:?}, {a:?}->{b:?}, width {width}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn direct_edges_keep_chunks_and_discard_degenerate_nonfinite_geometry() {
        let a = ViewportPoint { x: 1., y: 2. };
        let b = ViewportPoint { x: 9., y: 4. };
        let mut edges = vec![(a, b); EDGES_PER_PATH + 1];
        edges.extend([(a, a), (a, ViewportPoint { x: f32::NAN, y: 0. })]);
        let mut paths = Vec::new();
        paint_edges(&edges, point(px(0.), px(0.)), |p| paths.push(p));
        assert_eq!(paths.len(), 2);
        assert_eq!(
            paths.iter().map(|p| p.vertices.len()).sum::<usize>(),
            (EDGES_PER_PATH + 1) * 6
        );
        for width in [0., -1., f32::NAN] {
            paint_edges_with_width(&[(a, b)], point(px(0.), px(0.)), width, |_| {
                panic!("invalid stroke painted")
            });
        }
    }
}
