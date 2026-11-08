//! Ordered canvas geometry shared by Metal, D3D11 and the GPUI fallback.
//! Coordinates and stroke widths are logical pixels, colors are straight RGBA.
use anyhow::{Result, ensure};
use zipzi_editor_core::{
    GluePointState, OverlaySnapshot, OverlayVertex, OverlayVertexKind as K, ViewportPoint,
    canvas_style as style,
};

pub type Color = [f32; 4];
pub type Point = [f32; 2];
const MAX_PRIMITIVES: usize = 128 * 1024 * 1024 / size_of::<CanvasPrimitive>();

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasPalette {
    pub mesh: Color,
    pub image_bounds: Color,
    pub deformer: Color,
    pub deformer_frame: Color,
    pub glue: Color,
    pub mesh_selected: Color,
    pub hollow: Color,
    pub restricted: Color,
    pub restricted_border: Color,
    pub selection_region: Color,
}
impl CanvasPalette {
    pub fn new(dark: bool) -> Self {
        let rgb = |v: u32| {
            [
                ((v >> 16) & 255) as f32 / 255.,
                ((v >> 8) & 255) as f32 / 255.,
                (v & 255) as f32 / 255.,
                1.,
            ]
        };
        Self {
            mesh: rgb(0x909090),
            image_bounds: rgb(if dark { 0x6b91b5 } else { 0x476b8e }),
            deformer: rgb(0x62c878),
            deformer_frame: rgb(if dark { 0x669eff } else { 0x2864dc }),
            glue: rgb(0xffd000),
            mesh_selected: rgb(0xff0000),
            hollow: rgb(0xffffff),
            restricted: rgb(0xb0b0b0),
            restricted_border: rgb(0x505050),
            selection_region: rgb(if dark { 0xb0b0b0 } else { 0x606060 }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasOptions {
    pub lines: bool,
    pub markers: bool,
}
impl Default for CanvasOptions {
    fn default() -> Self {
        Self {
            lines: true,
            markers: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    Line,
    Marker,
    Triangle,
    WeightedTriangle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CanvasRenderStats {
    pub instances: usize,
    pub upload_bytes: usize,
    pub draw_calls: usize,
    pub cached: bool,
}

/// Four float4s, with exactly the same initialized layout on both GPU backends.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasPrimitive {
    /// Line endpoints; marker center/half size; triangle first two points.
    pub geometry: [f32; 4],
    /// Kind (0/1/2), then line width; marker corner radius/border width;
    /// or the triangle's third point.
    pub parameters: [f32; 4],
    pub color: Color,
    pub border: Color,
}
impl CanvasPrimitive {
    pub fn kind(&self) -> PrimitiveKind {
        match self.parameters[0] {
            0. => PrimitiveKind::Line,
            1. => PrimitiveKind::Marker,
            3. => PrimitiveKind::WeightedTriangle,
            _ => PrimitiveKind::Triangle,
        }
    }
    pub fn bounds(&self) -> [f32; 4] {
        let [x, y, a, b] = self.geometry;
        match self.kind() {
            PrimitiveKind::Line => {
                let r = self.parameters[1] / 2. + 1.;
                [x.min(a) - r, y.min(b) - r, x.max(a) + r, y.max(b) + r]
            }
            PrimitiveKind::Marker => [x - a - 1., y - b - 1., x + a + 1., y + b + 1.],
            PrimitiveKind::Triangle | PrimitiveKind::WeightedTriangle => {
                let [_, c, d, _] = self.parameters;
                [
                    x.min(a).min(c) - 1.,
                    y.min(b).min(d) - 1.,
                    x.max(a).max(c) + 1.,
                    y.max(b).max(d) + 1.,
                ]
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasDrawList {
    pub primitives: Vec<CanvasPrimitive>,
}

/// Per-viewport cache. Unchanged repaints reuse both the instance allocation and
/// the native texture; pointer-only state changes rebuild without model evaluation.
#[derive(Default)]
pub struct CanvasOverlayCache {
    cached: Option<(
        OverlaySnapshot,
        Point,
        CanvasPalette,
        CanvasOptions,
        std::sync::Arc<CanvasDrawList>,
    )>,
}
impl CanvasOverlayCache {
    pub fn prepare(
        &mut self,
        overlay: &OverlaySnapshot,
        extent: Point,
        palette: CanvasPalette,
        options: CanvasOptions,
    ) -> Result<std::sync::Arc<CanvasDrawList>> {
        if let Some((old, e, p, o, list)) = &self.cached {
            if old == overlay && *e == extent && *p == palette && *o == options {
                return Ok(list.clone());
            }
        }
        let list = std::sync::Arc::new(CanvasDrawList::new(overlay, extent, palette, options)?);
        self.cached = Some((overlay.clone(), extent, palette, options, list.clone()));
        Ok(list)
    }
}
impl CanvasDrawList {
    pub fn new(
        overlay: &OverlaySnapshot,
        extent: Point,
        p: CanvasPalette,
        options: CanvasOptions,
    ) -> Result<Self> {
        ensure!(
            extent.iter().all(|v| v.is_finite() && *v > 0.),
            "Invalid canvas extent"
        );
        let mut b = Builder {
            list: Self::default(),
            extent,
            overflow: false,
        };
        if let Some(weights) = &overlay.selection_weights {
            for (points, influences) in &weights.triangles {
                b.push(CanvasPrimitive {
                    geometry: [points[0].x, points[0].y, points[1].x, points[1].y],
                    parameters: [3., points[2].x, points[2].y, 0.],
                    color: weights.color,
                    border: [influences[0], influences[1], influences[2], 0.],
                });
            }
        }
        if options.lines {
            b.lines(&overlay.lines, 1., alpha(p.mesh, 0.45));
            b.lines(
                &overlay.reference_lines,
                style::IDLE_LINE_WIDTH,
                alpha(p.mesh, 0.5),
            );
            b.dashes(
                &overlay.image_bounds,
                style::IDLE_LINE_WIDTH,
                alpha(p.image_bounds, 0.55),
            );
            b.dashes(
                &overlay.automatic_lines,
                style::IDLE_LINE_WIDTH,
                alpha(p.mesh, 0.55),
            );
            if let Some(o) = overlay.origin_crosshair {
                b.line(
                    [o.x - 10., o.y],
                    [o.x + 10., o.y],
                    1.,
                    [224. / 255., 85. / 255., 85. / 255., 0.6],
                );
                b.line(
                    [o.x, o.y - 10.],
                    [o.x, o.y + 10.],
                    1.,
                    [85. / 255., 181. / 255., 85. / 255., 0.6],
                );
            }
            b.lines(&overlay.guide_lines, 0.5, alpha(p.mesh, 0.18));
            b.lines(&overlay.accent_lines, 1., alpha(p.mesh, 0.8));
            b.lines(&overlay.hover_lines, 1., p.mesh);
            for d in &overlay.deformers {
                let opacity = if d.outline_only {
                    if d.hovered {
                        0.75
                    } else {
                        style::WARP_OUTLINE_OPACITY
                    }
                } else if d.hovered {
                    0.5
                } else if d.selected {
                    0.32
                } else {
                    0.2
                };
                let color = if d.kind == zipzi_editor_core::ControlKind::Warp {
                    p.mesh
                } else {
                    p.deformer_frame
                };
                b.lines(
                    &d.lines,
                    style::line_width(d.selected),
                    alpha(color, opacity),
                );
            }
            b.lines(
                &overlay.bezier_lines,
                style::ACTIVE_LINE_WIDTH,
                alpha(p.deformer, 0.28),
            );
            b.lines(&overlay.bezier_tangents, 1., alpha(p.deformer, 0.85));
            b.lines(
                &overlay.gizmo_lines,
                style::IDLE_LINE_WIDTH,
                alpha(p.deformer, 0.45),
            );
            b.lines(&overlay.warp_frame_lines, 1., alpha(p.deformer_frame, 0.95));
            for glue in &overlay.glues {
                b.dashes(
                    &glue.pairs,
                    style::line_width(glue.selected),
                    alpha(p.glue, if glue.hovered { 1. } else { 0.85 }),
                );
            }
            for selected in [false, true] {
                for bone in overlay
                    .rotation_bones
                    .iter()
                    .filter(|b| b.selected == selected)
                {
                    let color = if selected {
                        p.deformer_frame
                    } else {
                        alpha(
                            p.mesh,
                            if bone.hovered {
                                0.65
                            } else {
                                style::ROTATION_IDLE_OPACITY
                            },
                        )
                    };
                    let [a, bone_shoulder, c, d] = bone.points.map(pt);
                    b.triangle(a, bone_shoulder, c, color);
                    b.triangle(a, c, d, color);
                }
            }
        }
        if options.markers {
            for grid in [true, false] {
                for emphasized in [false, true] {
                    for v in overlay.vertices.iter().filter(|v| {
                        (v.kind == K::WarpGrid) == grid && (v.selected || v.hovered) == emphasized
                    }) {
                        b.vertex(v, p);
                    }
                }
            }
            if let Some(weights) = &overlay.selection_weights {
                for &(position, weight, kind) in &weights.points {
                    let r = if kind == K::WarpGrid {
                        style::WARP_GRID_POINT_RADIUS
                    } else {
                        style::POINT_RADIUS
                    };
                    let mut color = weights.color;
                    for c in &mut color[..3] {
                        *c = 1. - (1. - *c) * weight;
                    }
                    color[3] = 1.;
                    b.marker(
                        pt(position),
                        [r; 2],
                        if kind == K::Mesh { r } else { 0. },
                        1.,
                        color,
                        [0., 0., 0., 1.],
                    );
                }
            }
            for h in &overlay.handles {
                b.control(
                    [h.position.x, h.position.y],
                    style::POINT_RADIUS,
                    h.round || h.tangent,
                    h.selected || h.tangent,
                    h.hovered,
                    h.editable,
                    p.deformer,
                    p,
                );
            }
            // Highest-priority coincident endpoint wins. Spatial buckets keep seams linear.
            let mut endpoints: Vec<_> = overlay.glues.iter().flat_map(|g| &g.points).collect();
            endpoints.sort_by_key(|v| (v.state, v.selected, v.hovered));
            let mut cells: std::collections::HashMap<(i64, i64), Vec<Point>> = Default::default();
            for v in endpoints.into_iter().rev() {
                let pos = [v.position.x, v.position.y];
                if !pos.iter().all(|v| v.is_finite()) {
                    continue;
                }
                let cell = ((pos[0] * 2.).floor() as i64, (pos[1] * 2.).floor() as i64);
                if (-1..=1).any(|dx: i64| {
                    (-1..=1).any(|dy: i64| {
                        cells
                            .get(&(cell.0.saturating_add(dx), cell.1.saturating_add(dy)))
                            .is_some_and(|ps| {
                                ps.iter()
                                    .any(|p| (p[0] - pos[0]).hypot(p[1] - pos[1]) < 0.5)
                            })
                    })
                }) {
                    continue;
                }
                cells.entry(cell).or_default().push(pos);
                match v.state {
                    GluePointState::Editable => b.marker(
                        pos,
                        [style::POINT_RADIUS; 2],
                        0.,
                        1.,
                        if v.selected {
                            p.mesh_selected
                        } else {
                            p.hollow
                        },
                        if v.hovered { p.deformer } else { p.glue },
                    ),
                    GluePointState::Reference => b.marker(
                        pos,
                        [style::POINT_RADIUS; 2],
                        0.,
                        1.,
                        [0.; 4],
                        alpha(p.glue, if v.hovered { 1. } else { 0.4 }),
                    ),
                    GluePointState::Restricted => b.control(
                        pos,
                        style::POINT_RADIUS,
                        false,
                        false,
                        v.hovered,
                        false,
                        p.glue,
                        p,
                    ),
                }
            }
        }
        if let Some((a, c)) = overlay.marquee {
            let min = [a.x.min(c.x), a.y.min(c.y)];
            let max = [a.x.max(c.x), a.y.max(c.y)];
            b.marker(
                [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.],
                [(max[0] - min[0]) / 2., (max[1] - min[1]) / 2.],
                0.,
                0.,
                alpha(p.selection_region, 0.12),
                [0.; 4],
            );
            b.lines(
                &[
                    (a, ViewportPoint { x: c.x, y: a.y }),
                    (ViewportPoint { x: c.x, y: a.y }, c),
                    (c, ViewportPoint { x: a.x, y: c.y }),
                    (ViewportPoint { x: a.x, y: c.y }, a),
                ],
                1.,
                p.selection_region,
            );
        }
        if overlay.lasso.len() >= 2 {
            b.polygon(&overlay.lasso, alpha(p.selection_region, 0.12))?;
            for pair in overlay.lasso.windows(2) {
                b.line(pt(pair[0]), pt(pair[1]), 1., p.selection_region);
            }
            // Keep the legacy eight-pixel closure cadence, including its 4096 bound.
            let first = pt(overlay.lasso[0]);
            let last = pt(*overlay.lasso.last().unwrap());
            let steps = ((first[0] - last[0]).hypot(first[1] - last[1]) / 8.)
                .ceil()
                .clamp(1., 4096.) as usize;
            let at = |t| {
                [
                    last[0] + (first[0] - last[0]) * t,
                    last[1] + (first[1] - last[1]) * t,
                ]
            };
            for i in 0..steps {
                b.line(
                    at(i as f32 / steps as f32),
                    at((i as f32 + 0.5) / steps as f32),
                    1.,
                    alpha(p.selection_region, 0.7),
                );
            }
        }
        if let Some(brush) = overlay.selection_brush {
            let center = pt(brush.position);
            let r = brush.radius;
            if brush.shape == zipzi_editor_core::BrushShape::Circle {
                b.marker(center, [r; 2], r, 1., [0.; 4], p.selection_region);
            } else {
                let h = if brush.shape == zipzi_editor_core::BrushShape::Line {
                    r * 0.1
                } else {
                    r
                };
                let (sin, cos) = brush.angle.to_radians().sin_cos();
                let corners = [[-r, -h], [r, -h], [r, h], [-r, h]]
                    .map(|[x, y]| [center[0] + x * cos - y * sin, center[1] + x * sin + y * cos]);
                for i in 0..4 {
                    b.line(corners[i], corners[(i + 1) % 4], 1., p.selection_region);
                }
            }
            // An erase stroke has a minus sign distinct from ordinary painting.
            if brush.subtract {
                b.line(
                    [center[0] - 4., center[1]],
                    [center[0] + 4., center[1]],
                    1.,
                    p.selection_region,
                );
            }
        }
        ensure!(!b.overflow, "Canvas overlay exceeds instance budget");
        Ok(b.list)
    }
}

fn alpha(mut c: Color, a: f32) -> Color {
    c[3] *= a;
    c
}
fn pt(p: ViewportPoint) -> Point {
    [p.x, p.y]
}
struct Builder {
    list: CanvasDrawList,
    extent: Point,
    overflow: bool,
}
impl Builder {
    fn push(&mut self, p: CanvasPrimitive) {
        if !p
            .geometry
            .iter()
            .chain(&p.parameters)
            .chain(&p.color)
            .chain(&p.border)
            .all(|v| v.is_finite())
        {
            return;
        }
        let bounds = p.bounds();
        if bounds[2] < 0.
            || bounds[3] < 0.
            || bounds[0] > self.extent[0]
            || bounds[1] > self.extent[1]
        {
            return;
        }
        if self.list.primitives.len() >= MAX_PRIMITIVES {
            self.overflow = true;
            return;
        }
        self.list.primitives.push(p);
    }
    fn line(&mut self, a: Point, b: Point, width: f32, color: Color) {
        if !width.is_finite() || width <= 0. || a == b {
            return;
        }
        let Some((lo, hi)) = clip_interval(a, b, self.extent, width / 2. + 1.) else {
            return;
        };
        let at = |t: f64| {
            std::array::from_fn(|i| {
                (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t) as f32
            })
        };
        let (a, b) = if lo == 0. && hi == 1. {
            (a, b)
        } else {
            (at(lo), at(hi))
        };
        self.push(CanvasPrimitive {
            geometry: [a[0], a[1], b[0], b[1]],
            parameters: [0., width, 0., 0.],
            color,
            border: [0.; 4],
        });
    }
    fn lines(&mut self, edges: &[(ViewportPoint, ViewportPoint)], width: f32, color: Color) {
        for &(a, b) in edges {
            self.line(pt(a), pt(b), width, color);
        }
    }
    fn dashes(&mut self, edges: &[(ViewportPoint, ViewportPoint)], width: f32, color: Color) {
        for &(a, b) in edges {
            for (a, b) in dashed_segments(pt(a), pt(b), self.extent) {
                self.line(a, b, width, color);
            }
        }
    }
    fn marker(
        &mut self,
        center: Point,
        half: Point,
        radius: f32,
        width: f32,
        color: Color,
        border: Color,
    ) {
        if half.iter().any(|v| *v <= 0.) {
            return;
        }
        self.push(CanvasPrimitive {
            geometry: [center[0], center[1], half[0], half[1]],
            parameters: [1., radius, width, 0.],
            color,
            border,
        });
    }
    fn control(
        &mut self,
        center: Point,
        r: f32,
        round: bool,
        selected: bool,
        hovered: bool,
        editable: bool,
        color: Color,
        p: CanvasPalette,
    ) {
        let fill = if !editable {
            p.restricted
        } else if selected {
            color
        } else {
            p.hollow
        };
        let border = if hovered {
            p.deformer
        } else if !editable {
            p.restricted_border
        } else if selected {
            color
        } else {
            [0., 0., 0., 1.]
        };
        self.marker(center, [r; 2], if round { r } else { 0. }, 1., fill, border);
    }
    fn vertex(&mut self, v: &OverlayVertex, p: CanvasPalette) {
        let r = if v.kind == K::WarpGrid {
            style::WARP_GRID_POINT_RADIUS
        } else {
            style::POINT_RADIUS
        };
        let round = matches!(v.kind, K::Mesh | K::RotationCenter | K::RotationTip);
        if !v.editable || !matches!(v.kind, K::WarpGrid | K::Warp) {
            self.control(
                pt(v.position),
                r,
                round,
                v.selected,
                v.hovered,
                v.editable,
                if v.kind == K::Mesh {
                    p.mesh_selected
                } else {
                    p.deformer
                },
                p,
            );
        } else if v.kind == K::WarpGrid {
            self.marker(
                pt(v.position),
                [r; 2],
                0.,
                if v.selected || v.hovered { 1. } else { 0. },
                if v.selected {
                    p.hollow
                } else {
                    alpha(p.mesh, if v.hovered { 1. } else { 0.6 })
                },
                if v.hovered { p.deformer } else { p.mesh },
            );
        } else {
            self.marker(
                pt(v.position),
                [r; 2],
                0.,
                1.,
                if v.selected { p.deformer } else { p.hollow },
                alpha(p.deformer, if v.selected || v.hovered { 1. } else { 0.65 }),
            );
        }
    }
    fn polygon(&mut self, points: &[ViewportPoint], color: Color) -> Result<()> {
        use lyon::{
            math::point,
            path::Path,
            tessellation::{
                BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
            },
        };
        if points.len() < 3 || !points.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            return Ok(());
        }
        let mut path = Path::builder();
        path.begin(point(points[0].x, points[0].y));
        for p in &points[1..] {
            path.line_to(point(p.x, p.y));
        }
        path.close();
        let mut geometry: VertexBuffers<Point, u32> = VertexBuffers::new();
        FillTessellator::new().tessellate_path(
            &path.build(),
            &FillOptions::default().with_fill_rule(FillRule::EvenOdd),
            &mut BuffersBuilder::new(&mut geometry, |v: FillVertex<'_>| {
                let p = v.position();
                [p.x, p.y]
            }),
        )?;
        for t in geometry.indices.chunks_exact(3) {
            let [a, b, c] = [
                geometry.vertices[t[0] as usize],
                geometry.vertices[t[1] as usize],
                geometry.vertices[t[2] as usize],
            ];
            self.triangle(a, b, c, color);
        }
        Ok(())
    }
    fn triangle(&mut self, a: Point, b: Point, c: Point, color: Color) {
        self.push(CanvasPrimitive {
            geometry: [a[0], a[1], b[0], b[1]],
            parameters: [2., c[0], c[1], 0.],
            color,
            border: [0.; 4],
        });
    }
}

/// Clip before subdivision so offscreen captures cannot generate unbounded dashes.
fn dashed_segments(a: Point, b: Point, extent: Point) -> Vec<(Point, Point)> {
    let (dx, dy) = (
        f64::from(b[0]) - f64::from(a[0]),
        f64::from(b[1]) - f64::from(a[1]),
    );
    let length = dx.hypot(dy);
    if !length.is_finite() || length < 2. {
        return vec![];
    }
    let Some((lo, hi)) = clip_interval(a, b, extent, 4.) else {
        return vec![];
    };
    let at = |d: f64| {
        [
            (f64::from(a[0]) + dx * d / length) as f32,
            (f64::from(a[1]) + dy * d / length) as f32,
        ]
    };
    let start = lo * length;
    let end = hi * length;
    let phase = start.rem_euclid(6.);
    // Visible dash generation is bounded even for a captured pointer far offscreen.
    let count = (((end - start) / 6.).ceil() as usize)
        .saturating_add(1)
        .min(8192);
    (0..count)
        .filter_map(|i| {
            let s = (i as f64 * 6. - phase).max(0.);
            let e = (i as f64 * 6. - phase + 3.).min(end - start);
            (e > s).then(|| (at(start + s), at(start + e)))
        })
        .collect()
}

fn clip_interval(a: Point, b: Point, extent: Point, padding: f32) -> Option<(f64, f64)> {
    if !a.iter().chain(&b).all(|v| v.is_finite()) {
        return None;
    }
    if [a, b]
        .iter()
        .all(|p| (0..2).all(|i| p[i] >= 0. && p[i] <= extent[i]))
    {
        return Some((0., 1.));
    }
    let (dx, dy) = (
        f64::from(b[0]) - f64::from(a[0]),
        f64::from(b[1]) - f64::from(a[1]),
    );
    let (mut lo, mut hi) = (0_f64, 1_f64);
    for (p, q) in [
        (-dx, f64::from(a[0]) + f64::from(padding)),
        (
            dx,
            f64::from(extent[0]) + f64::from(padding) - f64::from(a[0]),
        ),
        (-dy, f64::from(a[1]) + f64::from(padding)),
        (
            dy,
            f64::from(extent[1]) + f64::from(padding) - f64::from(a[1]),
        ),
    ] {
        if p == 0. {
            if q < 0. {
                return None;
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
    (lo <= hi).then_some((lo, hi))
}

#[cfg(test)]
#[path = "canvas_overlay/tests.rs"]
mod tests;
