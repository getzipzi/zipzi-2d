use gpui_kit::{Bounds, Path, PathBuilder, PathVertex, Pixels, Point, point, px};
use zipzi_editor_core::ViewportPoint;

// PathBuilder tessellates with u16 indices. Independent straight strokes use four
// vertices each; keep ample headroom and bound the size of each temporary buffer.
const EDGES_PER_PATH: usize = 4096;

// Standalone render tests/examples also include this module. The application
// configures its optional trace policy without coupling the renderer to it.
static LEGACY_BOUNDS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub(crate) fn configure_legacy_bounds(enabled: bool) {
    let _ = LEGACY_BOUNDS.set(enabled);
}

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
    paint: impl FnMut(Path<Pixels>),
) {
    if LEGACY_BOUNDS.get().copied().unwrap_or(false) {
        paint_edges_impl::<false>(edges, origin, width, paint);
    } else {
        paint_edges_impl::<true>(edges, origin, width, paint);
    }
}

fn paint_edges_impl<const DIRECT_BOUNDS: bool>(
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
        let mut minimum = point(f32::INFINITY, f32::INFINITY);
        let mut maximum = point(f32::NEG_INFINITY, f32::NEG_INFINITY);
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
            if !DIRECT_BOUNDS {
                let st = (point(0., 1.), point(0., 1.), point(0., 1.));
                path.push_triangle((a + normal, b + normal, a - normal), st);
                path.push_triangle((a - normal, b + normal, b - normal), st);
                continue;
            }
            // Bound the four stroke corners once instead of performing six
            // dependent Bounds::union operations through push_triangle.
            let corners = [a + normal, b + normal, a - normal, b - normal];
            for corner in corners {
                minimum.x = minimum.x.min(corner.x.as_f32());
                minimum.y = minimum.y.min(corner.y.as_f32());
                maximum.x = maximum.x.max(corner.x.as_f32());
                maximum.y = maximum.y.max(corner.y.as_f32());
            }
            // Preserve the exact triangles, winding, and per-chunk alpha behavior.
            path.vertices.extend([0, 1, 2, 2, 1, 3].map(|i| PathVertex {
                xy_position: corners[i],
                st_position: point(0., 1.),
                content_mask: Default::default(),
            }));
        }
        if let Some(mut path) = path {
            if DIRECT_BOUNDS {
                path.bounds = Bounds::from_corners(minimum.map(px), maximum.map(px));
            }
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
    fn batched_bounds_keep_legacy_triangles_and_cover_every_chunk() {
        let edges: Vec<_> = (0..12345)
            .map(|i| {
                let x = (i % 137) as f32 * 3.125 - 100.;
                let y = (i / 137) as f32 * 1.375;
                (
                    ViewportPoint { x, y },
                    ViewportPoint {
                        x: x + 4.75,
                        y: y - 2.25,
                    },
                )
            })
            .collect();
        for width in [0.75, 1., 3.] {
            let mut old = Vec::new();
            let mut new = Vec::new();
            let origin = point(px(-13.25), px(117.75));
            paint_edges_impl::<false>(&edges, origin, width, |p| old.push(p));
            paint_edges_impl::<true>(&edges, origin, width, |p| new.push(p));
            assert_eq!(old.len(), new.len());
            for (old, new) in old.iter().zip(&new) {
                assert_eq!(old.vertices.len(), new.vertices.len());
                for (a, b) in old.vertices.iter().zip(&new.vertices) {
                    assert_eq!(a.xy_position, b.xy_position);
                    assert_eq!(a.st_position, b.st_position);
                    assert_eq!(a.content_mask, b.content_mask);
                    assert!(new.bounds.contains(&b.xy_position));
                }
                // Different accumulation avoids repeated f32 subtraction/addition.
                for (a, b) in [
                    (old.bounds.origin, new.bounds.origin),
                    (old.bounds.bottom_right(), new.bounds.bottom_right()),
                ] {
                    assert!((a.x - b.x).as_f32().abs() < 0.001);
                    assert!((a.y - b.y).as_f32().abs() < 0.001);
                }
            }
        }
    }

    #[test]
    fn owned_path_scaling_reuses_vertices_and_preserves_geometry() {
        for count in [0, 3, EDGES_PER_PATH * 6] {
            for factor in [0.5, 1., 1.25, 2., 3.] {
                let mut path = Path::new(point(px(-13.25), px(117.75)));
                path.vertices.reserve(count + 16);
                path.order = 7;
                path.color = gpui_kit::rgba(0x12345678).into();
                path.content_mask.bounds = Bounds::from_corners(
                    point(px(-20.5), px(100.25)),
                    point(px(300.75), px(400.5)),
                );
                for i in 0..count / 3 {
                    let x = i as f32 * 0.125 - 100.25;
                    path.push_triangle(
                        (
                            point(px(x), px(0.25)),
                            point(px(x + 2.5), px(7.75)),
                            point(px(x - 3.25), px(-1.5)),
                        ),
                        (point(0., 0.), point(0.5, 0.), point(1., 1.)),
                    );
                }
                for vertex in &mut path.vertices {
                    vertex.content_mask = path.content_mask;
                }
                let expected = path.scale(factor);
                let buffer = path.vertices.as_ptr() as usize;
                let capacity = path.vertices.capacity();
                let scaled = path.into_scaled(factor);
                if count > 0 {
                    assert_eq!(
                        scaled.vertices.as_ptr() as usize,
                        buffer,
                        "owned scaling allocated a second vertex buffer"
                    );
                    assert_eq!(scaled.vertices.capacity(), capacity);
                }
                assert_eq!(scaled.id, expected.id);
                assert_eq!(scaled.order, expected.order);
                assert_eq!(scaled.bounds, expected.bounds);
                assert_eq!(scaled.content_mask, expected.content_mask);
                assert_eq!(scaled.color, expected.color);
                assert_eq!(scaled.vertices.len(), expected.vertices.len());
                for (a, b) in scaled.vertices.iter().zip(&expected.vertices) {
                    assert_eq!(a.xy_position, b.xy_position);
                    assert_eq!(a.st_position, b.st_position);
                    assert_eq!(a.content_mask, b.content_mask);
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
