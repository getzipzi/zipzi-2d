//! Canvas-specific visual language, independent of the application accent.
use crate::mesh_edges;
use gpui_kit::{
    Bounds, FillOptions, FillRule, Hsla, PathBuilder, PathStyle, Pixels, Point, Window, fill,
    point, px, rgb, rgba, size,
};
use zipzi_editor_core::{
    GluePointState, OverlaySnapshot, OverlayVertex, OverlayVertexKind, ViewportPoint,
};

const GLUE_MARKER_RADIUS: f32 = 3.25;

/// The native regression tests paint these same primitives into a headless scene.
pub(crate) trait Painter {
    fn paint_quad(&mut self, quad: gpui_kit::PaintQuad);
    fn paint_path(&mut self, path: gpui_kit::Path<Pixels>, color: Hsla);
    /// Only quads may be emitted inside this scope. GPUI retains their insertion
    /// order, including overlaps, without an individual bounds-tree entry.
    fn quad_layer(&mut self, _bounds: Bounds<Pixels>, paint: impl FnOnce(&mut Self))
    where
        Self: Sized,
    {
        paint(self);
    }
}
impl Painter for Window {
    fn paint_quad(&mut self, quad: gpui_kit::PaintQuad) {
        Window::paint_quad(self, quad);
    }
    fn paint_path(&mut self, path: gpui_kit::Path<Pixels>, color: Hsla) {
        Window::paint_path(self, path, color);
    }
    fn quad_layer(&mut self, bounds: Bounds<Pixels>, paint: impl FnOnce(&mut Self)) {
        self.paint_layer(bounds, paint);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub mesh: Hsla,
    pub image_bounds: Hsla,
    pub deformer: Hsla,
    pub deformer_frame: Hsla,
    pub glue: Hsla,
    pub mesh_selected: Hsla,
    pub hollow: Hsla,
    pub restricted: Hsla,
    pub restricted_border: Hsla,
    pub selection_region: Hsla,
}
impl Palette {
    pub fn new(dark: bool) -> Self {
        Self {
            mesh: rgb(0x909090).into(),
            image_bounds: rgb(if dark { 0x6b91b5 } else { 0x476b8e }).into(),
            deformer: rgb(0x62c878).into(),
            deformer_frame: rgb(if dark { 0x669eff } else { 0x2864dc }).into(),
            glue: rgb(0xffd000).into(),
            mesh_selected: rgb(0xff0000).into(),
            selection_region: rgb(if dark { 0xb0b0b0 } else { 0x606060 }).into(),
            hollow: rgb(0xffffff).into(),
            restricted: rgb(0xb0b0b0).into(),
            restricted_border: rgb(0x505050).into(),
        }
    }
    #[cfg(target_os = "macos")]
    pub fn metal(self) -> zipzi_editor_render::OverlayStyle {
        let color = |c: Hsla| {
            let c: gpui_kit::Rgba = c.into();
            [c.r, c.g, c.b, c.a]
        };
        zipzi_editor_render::OverlayStyle {
            edge: color(self.mesh.opacity(0.45)),
            vertex: color(self.hollow),
            selected: color(self.mesh_selected),
            restricted: color(self.restricted),
        }
    }
}

pub(crate) fn paint_lines(
    overlay: &OverlaySnapshot,
    bounds: Bounds<Pixels>,
    p: Palette,
    tessellated_edges: bool,
    window: &mut impl Painter,
) {
    let mut lines = |edges: &[(ViewportPoint, ViewportPoint)], width, color| {
        let paint = |path| window.paint_path(path, color);
        if tessellated_edges {
            mesh_edges::paint_edges_with_width_tessellated(edges, bounds.origin, width, paint);
        } else {
            mesh_edges::paint_edges_with_width(edges, bounds.origin, width, paint);
        }
    };
    lines(&overlay.reference_lines, 0.75, p.mesh.opacity(0.5));
    let image_bounds: Vec<_> = overlay
        .image_bounds
        .iter()
        .flat_map(|&(a, b)| {
            dashed_segments(
                a,
                b,
                [bounds.size.width.as_f32(), bounds.size.height.as_f32()],
            )
        })
        .collect();
    lines(&image_bounds, 0.75, p.image_bounds.opacity(0.55));
    let automatic: Vec<_> = overlay
        .automatic_lines
        .iter()
        .flat_map(|&(a, b)| {
            dashed_segments(
                a,
                b,
                [bounds.size.width.as_f32(), bounds.size.height.as_f32()],
            )
        })
        .take(100_000)
        .collect();
    lines(&automatic, 0.75, p.mesh.opacity(0.55));
    if let Some(origin) = overlay.origin_crosshair {
        let size = 10.;
        let x_axis = [(
            ViewportPoint {
                x: origin.x - size,
                y: origin.y,
            },
            ViewportPoint {
                x: origin.x + size,
                y: origin.y,
            },
        )];
        let y_axis = [(
            ViewportPoint {
                x: origin.x,
                y: origin.y - size,
            },
            ViewportPoint {
                x: origin.x,
                y: origin.y + size,
            },
        )];
        lines(&x_axis, 1.0, rgb(0xe05555).opacity(0.6).into());
        lines(&y_axis, 1.0, rgb(0x55b555).opacity(0.6).into());
    }
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
    lines(
        &overlay.warp_frame_lines,
        1.,
        p.deformer_frame.opacity(0.95),
    );
    lines(&overlay.rotation_lines, 1., p.deformer.opacity(0.85));
    for glue in &overlay.glues {
        let color = p.glue;
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
fn restricted_marker(
    center: Point<Pixels>,
    radius: f32,
    round: bool,
    p: Palette,
    window: &mut impl Painter,
) {
    let r = px(radius);
    window.paint_quad(
        fill(
            Bounds::new(center - point(r, r), size(r * 2., r * 2.)),
            p.restricted,
        )
        .corner_radii(if round { r } else { px(0.) })
        .border_widths(px(1.))
        .border_color(p.restricted_border),
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
/// A closed, even-odd polygon with bounds that retain antialiased edge coverage.
pub(crate) fn polygon_fill_path(
    mut points: impl ExactSizeIterator<Item = Point<Pixels>>,
) -> Option<gpui_kit::Path<Pixels>> {
    if points.len() < 3 {
        return None;
    }
    let mut path = PathBuilder::fill().with_style(PathStyle::Fill(
        FillOptions::default().with_fill_rule(FillRule::EvenOdd),
    ));
    path.move_to(points.next()?);
    for point in points {
        path.line_to(point);
    }
    path.close();
    path.build().ok().map(|mut path| {
        path.bounds = path.bounds.dilate(px(1.));
        path
    })
}

pub(crate) fn paint_markers(
    overlay: &OverlaySnapshot,
    origin: Point<Pixels>,
    p: Palette,
    mesh_fallback: bool,
    layer_quads: bool,
    window: &mut impl Painter,
) {
    // Selected and hovered controls win when several markers overlap.
    for emphasized in [false, true] {
        // Rotation centers mix paths and quads. Leave them outside each quad
        // layer so GPUI's primitive-type batching cannot reorder the crosshair.
        for run in overlay.vertices.chunk_by(|a, b| {
            a.kind != OverlayVertexKind::RotationCenter
                && b.kind != OverlayVertexKind::RotationCenter
        }) {
            let visible = |v: &&OverlayVertex| {
                (v.selected || v.hovered) == emphasized
                    && (mesh_fallback || v.kind != OverlayVertexKind::Mesh)
            };
            let draw = |window: &mut _| {
                for v in run.iter().filter(visible) {
                    paint_vertex(v, origin, p, window);
                }
            };
            if layer_quads && run[0].kind != OverlayVertexKind::RotationCenter {
                let radius = px(zipzi_editor_render::OVERLAY_MARKER_RADIUS + 2.);
                let bounds = run
                    .iter()
                    .filter(visible)
                    .map(|v| {
                        let center = origin + point(px(v.position.x), px(v.position.y));
                        Bounds::new(
                            center - point(radius, radius),
                            size(radius * 2., radius * 2.),
                        )
                    })
                    .reduce(|a, b| a.union(&b));
                if let Some(bounds) = bounds {
                    window.quad_layer(bounds, draw);
                }
            } else {
                draw(window);
            }
        }
    }
    paint_other_markers(overlay, origin, p, window);
}

fn paint_vertex(v: &OverlayVertex, origin: Point<Pixels>, p: Palette, window: &mut impl Painter) {
    let center = origin + point(px(v.position.x), px(v.position.y));
    if !v.editable {
        let radius = zipzi_editor_render::OVERLAY_MARKER_RADIUS;
        restricted_marker(center, radius, v.kind != OverlayVertexKind::Warp, p, window);
        if v.kind == OverlayVertexKind::RotationCenter {
            let q = |x, y| ViewportPoint {
                x: v.position.x + x,
                y: v.position.y + y,
            };
            mesh_edges::paint_edges(
                &[(q(-6., 0.), q(6., 0.)), (q(0., -6.), q(0., 6.))],
                origin,
                |path| window.paint_path(path, p.restricted_border),
            );
        }
        if v.hovered {
            ring(center, radius + 2., window);
        }
        return;
    }
    let color = if v.kind == OverlayVertexKind::Mesh {
        if v.selected {
            p.mesh_selected
        } else {
            p.hollow
        }
    } else {
        p.deformer
    };
    let radius = zipzi_editor_render::OVERLAY_MARKER_RADIUS;
    match v.kind {
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
        OverlayVertexKind::Warp => marker(center, radius, false, v.selected, color, p, window),
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

fn paint_other_markers(
    overlay: &OverlaySnapshot,
    origin: Point<Pixels>,
    p: Palette,
    window: &mut impl Painter,
) {
    for handle in &overlay.handles {
        let center = origin + point(px(handle.position.x), px(handle.position.y));
        let color = p.deformer;
        if handle.editable {
            marker(
                center,
                zipzi_editor_render::OVERLAY_MARKER_RADIUS,
                handle.round,
                handle.selected,
                color,
                p,
                window,
            );
        } else {
            restricted_marker(
                center,
                zipzi_editor_render::OVERLAY_MARKER_RADIUS,
                handle.round,
                p,
                window,
            );
        }
        if handle.hovered {
            ring(
                center,
                zipzi_editor_render::OVERLAY_MARKER_RADIUS + 2.,
                window,
            );
        }
    }
    let mut endpoints: Vec<_> = overlay.glues.iter().flat_map(|g| g.points.iter()).collect();
    endpoints.sort_by_key(|point| (point.state, point.selected, point.hovered));
    let mut distinct: Vec<&zipzi_editor_core::OverlayGluePoint> = Vec::new();
    for endpoint in endpoints.into_iter().rev() {
        if !distinct.iter().any(|p| {
            (p.position.x - endpoint.position.x).hypot(p.position.y - endpoint.position.y) < 0.5
        }) {
            distinct.push(endpoint);
        }
    }
    for endpoint in distinct {
        let pos = endpoint.position;
        if !pos.x.is_finite() || !pos.y.is_finite() {
            continue;
        }
        let center = origin + point(px(pos.x), px(pos.y));
        let r = px(GLUE_MARKER_RADIUS);
        let bounds = Bounds::new(center - point(r, r), size(r * 2., r * 2.));
        match endpoint.state {
            GluePointState::Editable => {
                window.paint_quad(fill(bounds, p.glue));
                marker(
                    center,
                    2.75,
                    true,
                    endpoint.selected,
                    p.mesh_selected,
                    p,
                    window,
                );
            }
            GluePointState::Reference => {
                window.paint_quad(
                    fill(bounds, rgba(0))
                        .border_widths(px(0.75))
                        .border_color(p.glue.opacity(0.4)),
                );
            }
            GluePointState::Restricted => {
                window.paint_quad(fill(bounds, rgb(0x8b8b8b)));
                restricted_marker(center, 2.75, true, p, window);
            }
        }
        if endpoint.hovered {
            ring(center, GLUE_MARKER_RADIUS + 1.5, window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MarkerRecorder {
        operations: Vec<String>,
        layer: Option<Bounds<Pixels>>,
        layers: usize,
    }
    impl Painter for MarkerRecorder {
        fn paint_quad(&mut self, q: gpui_kit::PaintQuad) {
            if let Some(bounds) = self.layer {
                assert_eq!(
                    bounds.intersect(&q.bounds),
                    q.bounds,
                    "layer clips a marker"
                );
            }
            self.operations.push(format!(
                "quad {:?} {:?} {:?} {:?} {:?} {:?}",
                q.bounds,
                q.corner_radii,
                q.background,
                q.border_widths,
                q.border_color,
                q.border_style
            ));
        }
        fn paint_path(&mut self, path: gpui_kit::Path<Pixels>, color: Hsla) {
            assert!(
                self.layer.is_none(),
                "crosshair path reordered inside a quad layer"
            );
            let points: Vec<_> = path.vertices.iter().map(|v| v.xy_position).collect();
            self.operations
                .push(format!("path {:?} {:?} {:?}", path.bounds, color, points));
        }
        fn quad_layer(&mut self, bounds: Bounds<Pixels>, paint: impl FnOnce(&mut Self)) {
            assert!(self.layer.is_none());
            self.layers += 1;
            self.layer = Some(bounds);
            paint(self);
            self.layer = None;
        }
    }

    #[test]
    fn marker_layers_preserve_legacy_order_styles_and_hover_coverage() {
        let kinds = [
            OverlayVertexKind::Mesh,
            OverlayVertexKind::Warp,
            OverlayVertexKind::RotationCenter,
            OverlayVertexKind::RotationTip,
        ];
        let overlay = OverlaySnapshot {
            vertices: (0..24)
                .map(|i| OverlayVertex {
                    id: i,
                    kind: kinds[i as usize % kinds.len()],
                    position: ViewportPoint {
                        x: (i % 3) as f32 * 2.25 - 4.,
                        y: (i % 2) as f32 * 1.5,
                    },
                    selected: i % 3 == 0,
                    hovered: i % 2 == 0,
                    editable: i % 5 != 0,
                })
                .collect(),
            ..Default::default()
        };
        let origin = point(px(20.25), px(40.5));
        for dark in [false, true] {
            for mesh_fallback in [false, true] {
                let palette = Palette::new(dark);
                let mut old = MarkerRecorder::default();
                // The original painter's order, independently of the new runs.
                for emphasized in [false, true] {
                    for v in &overlay.vertices {
                        if (v.selected || v.hovered) == emphasized
                            && (mesh_fallback || v.kind != OverlayVertexKind::Mesh)
                        {
                            paint_vertex(v, origin, palette, &mut old);
                        }
                    }
                }
                let mut grouped = MarkerRecorder::default();
                paint_markers(&overlay, origin, palette, mesh_fallback, true, &mut grouped);
                assert_eq!(grouped.operations, old.operations);
                assert!(grouped.layers > 0);
            }
        }
    }

    #[test]
    fn thousands_of_mesh_markers_use_two_layers_without_dropping_points() {
        let overlay = OverlaySnapshot {
            vertices: (0..5000)
                .map(|i| OverlayVertex {
                    id: i,
                    kind: OverlayVertexKind::Mesh,
                    position: ViewportPoint {
                        x: (i % 100) as f32 * 3.,
                        y: (i / 100) as f32 * 3.,
                    },
                    selected: i % 2 == 0,
                    hovered: false,
                    editable: true,
                })
                .collect(),
            ..Default::default()
        };
        let mut grouped = MarkerRecorder::default();
        paint_markers(
            &overlay,
            point(px(0.), px(0.)),
            Palette::new(false),
            true,
            true,
            &mut grouped,
        );
        assert_eq!(grouped.layers, 2);
        assert_eq!(grouped.operations.len(), 5000);
        let mut unlayered = MarkerRecorder::default();
        paint_markers(
            &overlay,
            point(px(0.), px(0.)),
            Palette::new(false),
            true,
            false,
            &mut unlayered,
        );
        assert_eq!(grouped.operations, unlayered.operations);
        assert_eq!(unlayered.layers, 0);
    }

    #[test]
    fn quad_layers_bound_gpui_ordering_work_and_retain_overlapping_quad_order() {
        #[derive(Default)]
        struct ScenePainter(gpui_kit::Scene);
        impl Painter for ScenePainter {
            fn paint_quad(&mut self, q: gpui_kit::PaintQuad) {
                self.0.insert_primitive(gpui_kit::Quad {
                    order: 0,
                    bounds: q.bounds.scale(1.),
                    content_mask: gpui_kit::ContentMask {
                        bounds: Bounds::new(point(px(0.), px(0.)), size(px(400.), px(400.))),
                    }
                    .scale(1.),
                    background: q.background,
                    border_color: q.border_color,
                    corner_radii: q.corner_radii.scale(1.),
                    border_widths: q.border_widths.scale(1.),
                    border_style: q.border_style,
                });
            }
            fn paint_path(&mut self, _: gpui_kit::Path<Pixels>, _: Hsla) {
                panic!("mesh-only scene contains a path");
            }
            fn quad_layer(&mut self, bounds: Bounds<Pixels>, paint: impl FnOnce(&mut Self)) {
                self.0.push_layer(bounds.scale(1.));
                paint(self);
                self.0.pop_layer();
            }
        }
        let overlay = OverlaySnapshot {
            vertices: (0..1000)
                .map(|i| OverlayVertex {
                    id: i,
                    kind: OverlayVertexKind::Mesh,
                    position: ViewportPoint {
                        x: 10. + (i % 20) as f32 * 2.,
                        y: 10. + (i / 20) as f32 * 2.,
                    },
                    selected: i % 3 == 0,
                    hovered: i % 7 == 0,
                    editable: i % 5 != 0,
                })
                .collect(),
            ..Default::default()
        };
        let mut old = ScenePainter::default();
        let mut grouped = ScenePainter::default();
        for (painter, layer) in [(&mut old, false), (&mut grouped, true)] {
            paint_markers(
                &overlay,
                point(px(0.), px(0.)),
                Palette::new(false),
                true,
                layer,
                painter,
            );
            painter.0.finish();
        }
        let orders = |p: &ScenePainter| {
            p.0.quads
                .iter()
                .map(|q| q.order)
                .collect::<std::collections::HashSet<_>>()
        };
        assert_eq!(orders(&grouped).len(), 2);
        assert!(orders(&old).len() > 2);
        // GPUI already combines adjacent quads across orders into one GPU batch;
        // this optimization removes CPU ordering work, not one draw per marker.
        assert_eq!(grouped.0.batches().count(), old.0.batches().count());
        assert_eq!(grouped.0.quads.len(), old.0.quads.len());
        // Spatial sorting may exchange disjoint quads. Every overlapping pair
        // must retain painter order (including borders and hover rings).
        for i in 0..grouped.0.quads.len() {
            let a = &grouped.0.quads[i];
            for b in &grouped.0.quads[i + 1..] {
                if !a.bounds.intersect(&b.bounds).is_empty() {
                    let position = |q: &gpui_kit::Quad| {
                        old.0
                            .quads
                            .iter()
                            .position(|x| {
                                x.bounds == q.bounds
                                    && x.background == q.background
                                    && x.border_color == q.border_color
                            })
                            .unwrap()
                    };
                    // Identical primitives may compare equal; their blending is
                    // equivalent whichever identical instance is found first.
                    assert!(
                        position(a) <= position(b),
                        "overlapping marker order changed"
                    );
                }
            }
        }
    }

    #[test]
    fn polygon_fill_preserves_concavities_and_even_odd_overlap() {
        let p = |x: f32, y: f32| point(px(100. + x), px(200. + y));
        for (points, expected_area) in [
            (
                vec![
                    p(0., 0.),
                    p(10., 0.),
                    p(10., 4.),
                    p(4., 4.),
                    p(4., 10.),
                    p(0., 10.),
                ],
                64.,
            ),
            (vec![p(0., 0.), p(10., 10.), p(0., 10.), p(10., 0.)], 50.),
            // Two identical loops cancel under even-odd filling, matching selection.
            (
                vec![
                    p(0., 0.),
                    p(10., 0.),
                    p(10., 10.),
                    p(0., 10.),
                    p(0., 0.),
                    p(10., 0.),
                    p(10., 10.),
                    p(0., 10.),
                ],
                0.,
            ),
        ] {
            let path = polygon_fill_path(points.into_iter()).unwrap();
            let area: f32 = path
                .vertices
                .chunks_exact(3)
                .map(|triangle| {
                    let a = triangle[0].xy_position;
                    let b = triangle[1].xy_position;
                    let c = triangle[2].xy_position;
                    ((b.x - a.x).as_f32() * (c.y - a.y).as_f32()
                        - (b.y - a.y).as_f32() * (c.x - a.x).as_f32())
                    .abs()
                        * 0.5
                })
                .sum();
            assert!(
                (area - expected_area).abs() < 0.001,
                "area {area}, expected {expected_area}"
            );
        }
    }

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
