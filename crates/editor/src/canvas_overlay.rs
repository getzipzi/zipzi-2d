//! Canvas-specific visual language, independent of the application accent.
use crate::mesh_edges;
use editor_core::{OverlaySnapshot, OverlayVertexKind, ViewportPoint};
use gpui_kit::{
    Bounds, Hsla, PathBuilder, Pixels, Point, Window, fill, point, px, rgb, rgba, size,
};

const GLUE_MARKER_RADIUS: f32 = 5.;

/// The native regression tests paint these same primitives into a headless scene.
pub(crate) trait Painter {
    fn paint_quad(&mut self, quad: gpui_kit::PaintQuad);
    fn paint_path(&mut self, path: gpui_kit::Path<Pixels>, color: Hsla);
}
impl Painter for Window {
    fn paint_quad(&mut self, quad: gpui_kit::PaintQuad) {
        Window::paint_quad(self, quad);
    }
    fn paint_path(&mut self, path: gpui_kit::Path<Pixels>, color: Hsla) {
        Window::paint_path(self, path, color);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub mesh: Hsla,
    pub deformer: Hsla,
    pub glue: Hsla,
    pub mesh_selected: Hsla,
    pub hollow: Hsla,
    pub selected: Hsla,
    pub surface: Hsla,
}
impl Palette {
    pub fn new(dark: bool) -> Self {
        Self {
            mesh: rgb(0x909090).into(),
            deformer: rgb(0x62c878).into(),
            glue: rgb(0x00c8ff).into(),
            mesh_selected: rgb(0xff0000).into(),
            selected: rgb(0xff5000).into(),
            hollow: rgb(0xffffff).into(),
            surface: rgb(if dark { 0x25272b } else { 0xe9ebef }).into(),
        }
    }
    pub fn metal(self) -> editor_render::OverlayStyle {
        let color = |c: Hsla| {
            let c: gpui_kit::Rgba = c.into();
            [c.r, c.g, c.b, c.a]
        };
        editor_render::OverlayStyle {
            edge: color(self.mesh.opacity(0.45)),
            vertex: color(self.hollow),
            selected: color(self.mesh_selected),
        }
    }
}

pub(crate) fn paint_lines(
    overlay: &OverlaySnapshot,
    bounds: Bounds<Pixels>,
    p: Palette,
    window: &mut impl Painter,
) {
    let mut lines = |edges: &[(ViewportPoint, ViewportPoint)], width, color| {
        mesh_edges::paint_edges_with_width(edges, bounds.origin, width, |path| {
            window.paint_path(path, color)
        });
    };
    lines(&overlay.reference_lines, 0.75, p.mesh.opacity(0.5));
    lines(&overlay.guide_lines, 0.75, p.deformer.opacity(0.3));
    // Hover raises contrast rather than widening the outline over the artwork.
    lines(&overlay.accent_lines, 1., p.mesh.opacity(0.8));
    lines(&overlay.hover_lines, 1., p.mesh);
    for deformer in &overlay.deformers {
        let (width, alpha) = if deformer.hovered {
            (1., 1.)
        } else if deformer.selected {
            (1., 0.85)
        } else {
            (0.75, 0.3)
        };
        lines(&deformer.lines, width, p.deformer.opacity(alpha));
    }
    lines(&overlay.gizmo_lines, 0.75, p.deformer.opacity(0.45));
    for glue in &overlay.glues {
        let color = if glue.selected { p.selected } else { p.glue };
        let dashed: Vec<_> = glue
            .pairs
            .iter()
            .flat_map(|&(a, b)| {
                dashed_segments(
                    a,
                    b,
                    [bounds.size.width.as_f32(), bounds.size.height.as_f32()],
                )
            })
            .collect();
        lines(
            &dashed,
            1.,
            color.opacity(if glue.hovered { 1. } else { 0.85 }),
        );
    }
}

/// Clip before subdivision: extreme zoom or offscreen pairs cannot allocate unbounded dashes.
fn dashed_segments(
    a: ViewportPoint,
    b: ViewportPoint,
    extent: [f32; 2],
) -> Vec<(ViewportPoint, ViewportPoint)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy);
    if !length.is_finite() || length < 2. {
        return Vec::new();
    }
    let mut lo = 0_f32;
    let mut hi = 1_f32;
    for (p, q) in [
        (-dx, a.x + 4.),
        (dx, extent[0] + 4. - a.x),
        (-dy, a.y + 4.),
        (dy, extent[1] + 4. - a.y),
    ] {
        if p == 0. {
            if q < 0. {
                return Vec::new();
            }
        } else {
            let t = q / p;
            if p < 0. {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
    }
    if lo > hi {
        return Vec::new();
    }
    let start = lo * length;
    let end = hi * length;
    let position = |distance: f32| ViewportPoint {
        x: a.x + dx * (distance / length),
        y: a.y + dy * (distance / length),
    };
    // Iterate by a bounded integer count rather than adding to a large float.
    let phase = start.rem_euclid(6.);
    let count = ((end - start) / 6.).ceil() as usize + 1;
    (0..count)
        .filter_map(|i| {
            let from = (i as f32 * 6. - phase).max(0.);
            let to = (i as f32 * 6. - phase + 3.).min(end - start);
            (to > from).then(|| (position(start + from), position(start + to)))
        })
        .collect()
}

/// Every deformer control (anchor, tangent, frame or rotation handle) uses
/// a black border and white center, with matching green fill and border when active.
fn marker(
    center: Point<Pixels>,
    r: f32,
    round: bool,
    solid: bool,
    color: Hsla,
    p: Palette,
    window: &mut impl Painter,
) {
    let r = px(r);
    window.paint_quad(
        fill(
            Bounds::new(center - point(r, r), size(r * 2., r * 2.)),
            if solid { color } else { p.hollow },
        )
        .corner_radii(if round { r } else { px(0.) })
        .border_widths(px(1.))
        .border_color(if solid { color } else { rgb(0x000000).into() }),
    );
}
fn ring(center: Point<Pixels>, r: f32, window: &mut impl Painter) {
    let r = px(r);
    window.paint_quad(
        fill(
            Bounds::new(center - point(r, r), size(r * 2., r * 2.)),
            rgba(0),
        )
        .corner_radii(r)
        .border_widths(px(1.))
        .border_color(rgb(0xffffff)),
    );
}
fn diamond(
    center: Point<Pixels>,
    solid: bool,
    half: bool,
    color: Hsla,
    p: Palette,
    window: &mut impl Painter,
) {
    let points = |r: f32| {
        [
            center + point(px(0.), px(-r)),
            center + point(px(r), px(0.)),
            center + point(px(0.), px(r)),
            center + point(px(-r), px(0.)),
        ]
    };
    let polygon = |points: &[Point<Pixels>]| {
        let mut path = PathBuilder::fill();
        path.move_to(points[0]);
        for &point in &points[1..] {
            path.line_to(point);
        }
        path.close();
        path.build().ok().map(|mut path| {
            path.bounds = path.bounds.dilate(px(1.));
            path
        })
    };
    // Both states use the same outer silhouette; the hollow border stays inside it.
    let outer = points(GLUE_MARKER_RADIUS);
    if let Some(path) = polygon(&outer) {
        window.paint_path(path, if solid { color } else { p.hollow });
    }
    if !solid {
        let inner = points(GLUE_MARKER_RADIUS - std::f32::consts::SQRT_2);
        if let Some(path) = polygon(&inner) {
            window.paint_path(path, p.surface);
        }
    }
    if half {
        if let Some(path) = polygon(&outer[..3]) {
            window.paint_path(path, color);
        }
    }
}

pub(crate) fn paint_markers(
    overlay: &OverlaySnapshot,
    origin: Point<Pixels>,
    p: Palette,
    mesh_fallback: bool,
    window: &mut impl Painter,
) {
    // Selected and hovered controls win when several markers overlap.
    for emphasized in [false, true] {
        for v in overlay
            .vertices
            .iter()
            .filter(|v| (v.selected || v.hovered) == emphasized)
        {
            let center = origin + point(px(v.position.x), px(v.position.y));
            let color = if v.kind == OverlayVertexKind::Mesh {
                if v.selected {
                    p.mesh_selected
                } else {
                    p.hollow
                }
            } else {
                p.deformer
            };
            let radius = editor_render::OVERLAY_MARKER_RADIUS;
            match v.kind {
                OverlayVertexKind::Mesh if !mesh_fallback => continue,
                OverlayVertexKind::Mesh => {
                    let r = px(radius);
                    window.paint_quad(
                        fill(
                            Bounds::new(center - point(r, r), size(r * 2., r * 2.)),
                            color,
                        )
                        .corner_radii(r)
                        .border_widths(px(1.))
                        .border_color(if v.selected {
                            p.mesh_selected
                        } else {
                            rgb(0x000000).into()
                        }),
                    );
                }
                OverlayVertexKind::Warp => {
                    marker(center, radius, false, v.selected, color, p, window)
                }
                OverlayVertexKind::RotationTip => {
                    marker(center, radius, true, v.selected, color, p, window)
                }
                OverlayVertexKind::RotationCenter => {
                    marker(center, radius, true, v.selected, color, p, window);
                    let q = |x, y| ViewportPoint {
                        x: v.position.x + x,
                        y: v.position.y + y,
                    };
                    mesh_edges::paint_edges(
                        &[(q(-6., 0.), q(6., 0.)), (q(0., -6.), q(0., 6.))],
                        origin,
                        |path| window.paint_path(path, color),
                    );
                }
            }
            if v.hovered {
                ring(center, radius + 2., window);
            }
        }
    }
    for handle in &overlay.handles {
        let center = origin + point(px(handle.position.x), px(handle.position.y));
        let color = p.deformer;
        marker(
            center,
            editor_render::OVERLAY_MARKER_RADIUS,
            handle.round,
            handle.selected,
            color,
            p,
            window,
        );
        if handle.hovered {
            ring(center, editor_render::OVERLAY_MARKER_RADIUS + 2., window);
        }
    }
    for emphasized in [false, true] {
        for glue in overlay
            .glues
            .iter()
            .filter(|g| (g.selected || g.hovered) == emphasized)
        {
            let color = if glue.selected { p.selected } else { p.glue };
            for &(a, b) in &glue.pairs {
                if ![a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite()) {
                    continue;
                }
                if (a.x - b.x).hypot(a.y - b.y) < 2. {
                    let center = origin + point(px((a.x + b.x) * 0.5), px((a.y + b.y) * 0.5));
                    diamond(center, false, true, color, p, window);
                    if glue.hovered {
                        ring(center, GLUE_MARKER_RADIUS + 2., window);
                    }
                } else {
                    for (pos, solid) in [(a, false), (b, true)] {
                        let center = origin + point(px(pos.x), px(pos.y));
                        diamond(center, solid, false, color, p, window);
                        if glue.hovered {
                            ring(center, GLUE_MARKER_RADIUS + 2., window);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glue_dashes_are_clipped_and_zero_length_pairs_have_no_line() {
        let pt = |x, y| ViewportPoint { x, y };
        assert!(dashed_segments(pt(5., 5.), pt(5., 5.), [100., 100.]).is_empty());
        assert!(dashed_segments(pt(-10., -20.), pt(110., -20.), [100., 100.]).is_empty());
        let dashes = dashed_segments(pt(-1e6, 50.), pt(1e6, 50.), [100., 100.]);
        assert!(!dashes.is_empty() && dashes.len() <= 20);
        for (a, b) in dashes {
            assert!(a.x >= -4.1 && b.x <= 104.1 && b.x > a.x);
        }
    }
}
