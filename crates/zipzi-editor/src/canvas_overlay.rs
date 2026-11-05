//! Canvas-specific visual language, independent of the application accent.
use crate::mesh_edges;
use gpui_kit::{Bounds, Hsla, Pixels, Point, Window, fill, point, px, size};
#[cfg(test)]
use gpui_kit::{FillOptions, FillRule, PathBuilder, PathStyle, rgb, rgba};
use zipzi_editor_core::ViewportPoint;
#[cfg(test)]
use zipzi_editor_core::{GluePointState, OverlaySnapshot, OverlayVertex, OverlayVertexKind};

#[cfg(test)]
use zipzi_editor_core::canvas_style as style;
#[cfg(test)]
const GLUE_MARKER_RADIUS: f32 = style::POINT_RADIUS;

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
        let p = zipzi_editor_render::canvas_overlay::CanvasPalette::new(dark);
        let color = |c: [f32; 4]| {
            gpui_kit::Rgba {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            }
            .into()
        };
        Self {
            mesh: color(p.mesh),
            image_bounds: color(p.image_bounds),
            deformer: color(p.deformer),
            deformer_frame: color(p.deformer_frame),
            glue: color(p.glue),
            mesh_selected: color(p.mesh_selected),
            selection_region: color(p.selection_region),
            hollow: color(p.hollow),
            restricted: color(p.restricted),
            restricted_border: color(p.restricted_border),
        }
    }
    pub fn canvas(self) -> zipzi_editor_render::canvas_overlay::CanvasPalette {
        let color = |c: Hsla| {
            let c: gpui_kit::Rgba = c.into();
            [c.r, c.g, c.b, c.a]
        };
        zipzi_editor_render::canvas_overlay::CanvasPalette {
            mesh: color(self.mesh),
            image_bounds: color(self.image_bounds),
            deformer: color(self.deformer),
            deformer_frame: color(self.deformer_frame),
            glue: color(self.glue),
            mesh_selected: color(self.mesh_selected),
            hollow: color(self.hollow),
            restricted: color(self.restricted),
            restricted_border: color(self.restricted_border),
            selection_region: color(self.selection_region),
        }
    }
    #[cfg(test)]
    pub fn native(self) -> zipzi_editor_render::OverlayStyle {
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

/// The fallback consumes exactly the same ordered geometry as both native backends.
#[cfg(test)]
pub(crate) fn paint_prepared<P: Painter>(
    list: &zipzi_editor_render::canvas_overlay::CanvasDrawList,
    origin: Point<Pixels>,
    layer_quads: bool,
    window: &mut P,
) {
    paint_prepared_with_options(
        list,
        origin,
        FallbackOptions {
            layer_quads,
            ..Default::default()
        },
        window,
    );
}

pub(crate) struct FallbackOptions {
    pub layer_quads: bool,
    pub batch_lines: bool,
    pub tessellated_edges: bool,
}
impl Default for FallbackOptions {
    fn default() -> Self {
        Self {
            layer_quads: true,
            batch_lines: true,
            tessellated_edges: false,
        }
    }
}
pub(crate) fn paint_prepared_with_options<P: Painter>(
    list: &zipzi_editor_render::canvas_overlay::CanvasDrawList,
    origin: Point<Pixels>,
    options: FallbackOptions,
    window: &mut P,
) {
    use zipzi_editor_render::canvas_overlay::PrimitiveKind as K;
    let color = |c: [f32; 4]| -> Hsla {
        gpui_kit::Rgba {
            r: c[0],
            g: c[1],
            b: c[2],
            a: c[3],
        }
        .into()
    };
    let point_at = |x, y| origin + point(px(x), px(y));
    for run in list.primitives.chunk_by(|a, b| {
        a.kind() == b.kind()
            && (a.kind() == K::Marker || (a.color == b.color && a.parameters[1] == b.parameters[1]))
    }) {
        match run[0].kind() {
            K::Marker => {
                let draw = |window: &mut P| {
                    for primitive in run {
                        let [x, y, w, h] = primitive.geometry;
                        window.paint_quad(
                            fill(
                                Bounds::new(point_at(x - w, y - h), size(px(w * 2.), px(h * 2.))),
                                color(primitive.color),
                            )
                            .corner_radii(px(primitive.parameters[1]))
                            .border_widths(px(primitive.parameters[2]))
                            .border_color(color(primitive.border)),
                        );
                    }
                };
                let bounds = run
                    .iter()
                    .map(|p| {
                        let [a, b, c, d] = p.bounds();
                        Bounds::from_corners(point_at(a, b), point_at(c, d))
                    })
                    .reduce(|a, b| a.union(&b))
                    .unwrap();
                if options.layer_quads {
                    window.quad_layer(bounds, draw);
                } else {
                    draw(window);
                }
            }
            K::Line => {
                let edges: Vec<_> = run
                    .iter()
                    .map(|p| {
                        let [x, y, a, b] = p.geometry;
                        (ViewportPoint { x, y }, ViewportPoint { x: a, y: b })
                    })
                    .collect();
                let width = run[0].parameters[1];
                let mut paint = |path| window.paint_path(path, color(run[0].color));
                if !options.batch_lines {
                    for edge in &edges {
                        mesh_edges::paint_edges_with_width(
                            std::slice::from_ref(edge),
                            origin,
                            width,
                            &mut paint,
                        );
                    }
                } else if options.tessellated_edges {
                    if width == 1. {
                        mesh_edges::paint_edges_tessellated(&edges, origin, paint);
                    } else {
                        mesh_edges::paint_edges_with_width_tessellated(
                            &edges, origin, width, paint,
                        );
                    }
                } else if width == 1. {
                    mesh_edges::paint_edges(&edges, origin, paint);
                } else {
                    mesh_edges::paint_edges_with_width(&edges, origin, width, paint);
                }
            }
            K::Triangle => {
                let first = run[0].geometry;
                let mut path = gpui_kit::Path::new(point_at(first[0], first[1]));
                for p in run {
                    let [x, y, a, b] = p.geometry;
                    path.push_triangle(
                        (
                            point_at(x, y),
                            point_at(a, b),
                            point_at(p.parameters[1], p.parameters[2]),
                        ),
                        (point(0., 1.), point(0., 1.), point(0., 1.)),
                    );
                }
                path.bounds = path.bounds.dilate(px(1.));
                window.paint_path(path, color(run[0].color));
            }
        }
    }
}

#[cfg(test)]
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
    lines(
        &overlay.reference_lines,
        style::IDLE_LINE_WIDTH,
        p.mesh.opacity(0.5),
    );
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
    lines(
        &image_bounds,
        style::IDLE_LINE_WIDTH,
        p.image_bounds.opacity(0.55),
    );
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
    lines(&automatic, style::IDLE_LINE_WIDTH, p.mesh.opacity(0.55));
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
    lines(&overlay.guide_lines, 0.5, p.mesh.opacity(0.18));
    // Hover raises contrast rather than widening the outline over the artwork.
    lines(&overlay.accent_lines, 1., p.mesh.opacity(0.8));
    lines(&overlay.hover_lines, 1., p.mesh);
    for deformer in &overlay.deformers {
        let width = style::line_width(deformer.selected);
        let alpha = if deformer.outline_only {
            if deformer.hovered {
                0.75
            } else {
                style::WARP_OUTLINE_OPACITY
            }
        } else if deformer.hovered {
            0.5
        } else if deformer.selected {
            0.32
        } else {
            0.2
        };
        let color = if deformer.kind == zipzi_editor_core::ControlKind::Warp {
            p.mesh
        } else {
            p.deformer_frame
        };
        lines(&deformer.lines, width, color.opacity(alpha));
    }
    lines(
        &overlay.bezier_lines,
        style::ACTIVE_LINE_WIDTH,
        p.deformer.opacity(0.28),
    );
    lines(&overlay.bezier_tangents, 1., p.deformer.opacity(0.85));
    lines(
        &overlay.gizmo_lines,
        style::IDLE_LINE_WIDTH,
        p.deformer.opacity(0.45),
    );
    lines(
        &overlay.warp_frame_lines,
        1.,
        p.deformer_frame.opacity(0.95),
    );
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
            style::line_width(glue.selected),
            color.opacity(if glue.hovered { 1. } else { 0.85 }),
        );
    }
    // Subdue unselected bones so the artwork remains visible beneath them.
    // Selected bones paint last, matching their priority in hit testing.
    for selected in [false, true] {
        for bone in overlay
            .rotation_bones
            .iter()
            .filter(|b| b.selected == selected)
        {
            let color = if bone.selected {
                p.deformer_frame
            } else {
                p.mesh.opacity(if bone.hovered {
                    0.65
                } else {
                    style::ROTATION_IDLE_OPACITY
                })
            };
            if let Some(path) = polygon_fill_path(
                bone.points
                    .iter()
                    .map(|p| bounds.origin + point(px(p.x), px(p.y))),
            ) {
                window.paint_path(path, color);
            }
        }
    }
}

/// Clip before subdivision: extreme zoom or offscreen pairs cannot allocate unbounded dashes.
#[cfg(test)]
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

/// Shapes identify controls; selection and hover never change their extent.
#[cfg(test)]
fn marker(
    center: Point<Pixels>,
    r: f32,
    round: bool,
    solid: bool,
    hovered: bool,
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
        .border_color(if hovered {
            p.deformer
        } else if solid {
            color
        } else {
            rgb(0x000000).into()
        }),
    );
}
#[cfg(test)]
fn restricted_marker(
    center: Point<Pixels>,
    radius: f32,
    round: bool,
    hovered: bool,
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
        .border_color(if hovered {
            p.deformer
        } else {
            p.restricted_border
        }),
    );
}
/// A closed, even-odd polygon with bounds that retain antialiased edge coverage.
#[cfg(test)]
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

#[cfg(test)]
pub(crate) fn paint_markers(
    overlay: &OverlaySnapshot,
    origin: Point<Pixels>,
    p: Palette,
    mesh_fallback: bool,
    layer_quads: bool,
    window: &mut impl Painter,
) {
    // Selected and hovered controls win when several markers overlap.
    for grid in [true, false] {
        for emphasized in [false, true] {
            // Rotation centers mix paths and quads. Leave them outside each quad
            // layer so GPUI's primitive-type batching cannot reorder the crosshair.
            for run in overlay.vertices.chunk_by(|a, b| {
                a.kind != OverlayVertexKind::RotationCenter
                    && b.kind != OverlayVertexKind::RotationCenter
            }) {
                let visible = |v: &&OverlayVertex| {
                    (v.selected || v.hovered) == emphasized
                        && (v.kind == OverlayVertexKind::WarpGrid) == grid
                        && (mesh_fallback || v.kind != OverlayVertexKind::Mesh)
                };
                let draw = |window: &mut _| {
                    for v in run.iter().filter(visible) {
                        paint_vertex(v, origin, p, window);
                    }
                };
                if layer_quads && run[0].kind != OverlayVertexKind::RotationCenter {
                    let radius = px(zipzi_editor_render::OVERLAY_MARKER_RADIUS + 1.);
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
    }
    paint_other_markers(overlay, origin, p, window);
}

#[cfg(test)]
fn paint_vertex(v: &OverlayVertex, origin: Point<Pixels>, p: Palette, window: &mut impl Painter) {
    let center = origin + point(px(v.position.x), px(v.position.y));
    let radius = if v.kind == OverlayVertexKind::WarpGrid {
        style::WARP_GRID_POINT_RADIUS
    } else {
        style::POINT_RADIUS
    };
    let round = matches!(
        v.kind,
        OverlayVertexKind::Mesh
            | OverlayVertexKind::RotationCenter
            | OverlayVertexKind::RotationTip
    );
    if !v.editable {
        restricted_marker(center, radius, round, v.hovered, p, window);
        return;
    }
    match v.kind {
        OverlayVertexKind::WarpGrid => {
            let r = px(radius);
            window.paint_quad(
                fill(
                    Bounds::new(center - point(r, r), size(r * 2., r * 2.)),
                    if v.selected {
                        p.hollow
                    } else {
                        p.mesh.opacity(if v.hovered { 1. } else { 0.6 })
                    },
                )
                .border_widths(px(if v.selected || v.hovered { 1. } else { 0. }))
                .border_color(if v.hovered { p.deformer } else { p.mesh }),
            );
        }
        OverlayVertexKind::Warp => {
            let r = px(radius);
            window.paint_quad(
                fill(
                    Bounds::new(center - point(r, r), size(r * 2., r * 2.)),
                    if v.selected { p.deformer } else { p.hollow },
                )
                .border_widths(px(1.))
                .border_color(
                    p.deformer
                        .opacity(if v.selected || v.hovered { 1. } else { 0.65 }),
                ),
            );
        }
        _ => marker(
            center,
            radius,
            round,
            v.selected,
            v.hovered,
            if v.kind == OverlayVertexKind::Mesh {
                p.mesh_selected
            } else {
                p.deformer
            },
            p,
            window,
        ),
    }
}

#[cfg(test)]
fn paint_other_markers(
    overlay: &OverlaySnapshot,
    origin: Point<Pixels>,
    p: Palette,
    window: &mut impl Painter,
) {
    for handle in &overlay.handles {
        let center = origin + point(px(handle.position.x), px(handle.position.y));
        if !handle.editable {
            restricted_marker(
                center,
                style::POINT_RADIUS,
                handle.round || handle.tangent,
                handle.hovered,
                p,
                window,
            );
        } else {
            marker(
                center,
                style::POINT_RADIUS,
                handle.round || handle.tangent,
                handle.selected || handle.tangent,
                handle.hovered,
                p.deformer,
                p,
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
                window.paint_quad(
                    fill(
                        bounds,
                        if endpoint.selected {
                            p.mesh_selected
                        } else {
                            p.hollow
                        },
                    )
                    .border_widths(px(1.))
                    .border_color(if endpoint.hovered {
                        p.deformer
                    } else {
                        p.glue
                    }),
                );
            }
            GluePointState::Reference => {
                window.paint_quad(fill(bounds, rgba(0)).border_widths(px(1.)).border_color(
                    if endpoint.hovered {
                        p.glue
                    } else {
                        p.glue.opacity(0.4)
                    },
                ));
            }
            GluePointState::Restricted => restricted_marker(
                center,
                style::POINT_RADIUS,
                false,
                endpoint.hovered,
                p,
                window,
            ),
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
    fn point_sizes_stay_fixed_with_secondary_warp_grid_controls() {
        #[derive(Default)]
        struct BoundsRecorder(Vec<Bounds<Pixels>>);
        impl Painter for BoundsRecorder {
            fn paint_quad(&mut self, q: gpui_kit::PaintQuad) {
                self.0.push(q.bounds);
            }
            fn paint_path(&mut self, _: gpui_kit::Path<Pixels>, _: Hsla) {
                panic!("point must not grow a hover path");
            }
        }
        for kind in [
            OverlayVertexKind::Mesh,
            OverlayVertexKind::WarpGrid,
            OverlayVertexKind::Warp,
            OverlayVertexKind::RotationCenter,
            OverlayVertexKind::RotationTip,
        ] {
            for selected in [false, true] {
                for hovered in [false, true] {
                    for editable in [false, true] {
                        let mut recorder = BoundsRecorder::default();
                        paint_vertex(
                            &OverlayVertex {
                                id: 0,
                                position: ViewportPoint { x: 20., y: 20. },
                                selected,
                                hovered,
                                editable,
                                kind,
                            },
                            point(px(0.), px(0.)),
                            Palette::new(true),
                            &mut recorder,
                        );
                        assert_eq!(recorder.0, {
                            let size = if kind == OverlayVertexKind::WarpGrid {
                                5.
                            } else {
                                6.
                            };
                            vec![Bounds::new(
                                point(px(20. - size / 2.), px(20. - size / 2.)),
                                gpui_kit::size(px(size), px(size)),
                            )]
                        });
                    }
                }
            }
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
    fn rotation_markers_keep_screen_size_at_every_bone_scale() {
        use zipzi_editor_core::{OverlayHandle, OverlayRotationBone};
        let center = ViewportPoint { x: 40., y: 40. };
        let tip = ViewportPoint { x: 160., y: 40. };
        for editable in [false, true] {
            for hovered in [false, true] {
                let mut reference = None;
                for scale in [0.1, 1., 8.] {
                    let overlay = OverlaySnapshot {
                        rotation_bones: vec![OverlayRotationBone {
                            points: [center, center, tip, tip],
                            scale,
                            selected: true,
                            hovered: false,
                        }],
                        vertices: vec![OverlayVertex {
                            id: 0,
                            kind: OverlayVertexKind::RotationCenter,
                            position: center,
                            selected: false,
                            hovered,
                            editable,
                        }],
                        handles: vec![OverlayHandle {
                            tangent: false,
                            rotation: true,
                            position: tip,
                            round: false,
                            hovered,
                            selected: true,
                            editable,
                        }],
                        ..Default::default()
                    };
                    let mut recorder = MarkerRecorder::default();
                    paint_markers(
                        &overlay,
                        point(px(0.), px(0.)),
                        Palette::new(false),
                        true,
                        true,
                        &mut recorder,
                    );
                    if let Some(expected) = &reference {
                        assert_eq!(&recorder.operations, expected, "bone scale={scale}");
                    } else {
                        assert!(!recorder.operations.is_empty());
                        reference = Some(recorder.operations);
                    }
                }
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
