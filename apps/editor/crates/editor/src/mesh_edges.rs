use editor_core::ViewportPoint;
use gpui_kit::{Path, PathBuilder, Pixels, Point, point, px};

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
