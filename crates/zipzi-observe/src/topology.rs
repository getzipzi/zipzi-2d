//! Deterministic CPU diagnostic compositing over the GPU's exact view mapping.
use crate::{ObservationError, RenderRequest, ViewMapping};
use std::collections::{HashMap, HashSet};
use zipzi_core::{DrawableFrame, Mesh, Part};
use zipzi_sdk::{MeshQuality, TopologyOverlay};

type Point = (f64, f64);
const CYAN: [u8; 4] = [0, 220, 255, 255];
const YELLOW: [u8; 4] = [255, 230, 40, 255];
const GREEN: [u8; 4] = [40, 255, 100, 255];
const MAGENTA: [u8; 4] = [255, 40, 200, 255];

pub(crate) fn selection(
    meshes: &[Mesh],
    parts: &[Part],
    overlay: Option<&TopologyOverlay>,
) -> Result<Option<HashSet<String>>, ObservationError> {
    let Some(targets) = overlay.and_then(|o| o.targets.as_ref()) else {
        return Ok(None);
    };
    zipzi_sdk::resolve_mesh_ids(meshes, parts, targets)
        .map(|ids| Some(ids.into_iter().collect()))
        .map_err(|e| {
            let code = match &e {
                zipzi_sdk::SpatialError::MissingMesh(_) => "MISSING_MESH",
                zipzi_sdk::SpatialError::MissingPart(_) => "MISSING_PART",
                _ => "INVALID_TOPOLOGY_OPTIONS",
            };
            ObservationError {
                code: code.into(),
                message: format!("Invalid topology target: {e:?}"),
                asset_id: None,
            }
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}
impl Rect {
    fn intersects(self, b: Self) -> bool {
        self.x < b.x + b.w && b.x < self.x + self.w && self.y < b.y + b.h && b.y < self.y + self.h
    }
}
#[derive(Default)]
struct LabelLayout {
    cells: HashMap<(i32, i32), Vec<Rect>>,
    placed: usize,
}
impl LabelLayout {
    fn keys(r: Rect) -> impl Iterator<Item = (i32, i32)> {
        (r.y.div_euclid(32)..=(r.y + r.h - 1).div_euclid(32)).flat_map(move |y| {
            (r.x.div_euclid(32)..=(r.x + r.w - 1).div_euclid(32)).map(move |x| (x, y))
        })
    }
    fn place(
        &mut self,
        p: (i32, i32),
        w: i32,
        h: i32,
        width: i32,
        height: i32,
        gap: i32,
        max_labels: usize,
    ) -> Option<Rect> {
        if self.placed >= max_labels {
            return None;
        }
        let (x, y) = p;
        for (x, y) in [
            (x + 4, y - h / 2),
            (x - w - 4, y - h / 2),
            (x - w / 2, y - h - 4),
            (x - w / 2, y + 4),
            (x + 8, y - h - 8),
            (x + 8, y + 8),
            (x - w - 8, y - h - 8),
            (x - w - 8, y + 8),
        ] {
            let rect = Rect { x, y, w, h };
            if x < 0 || y < 0 || x + w > width || y + h > height {
                continue;
            }
            if Self::keys(rect).any(|key| {
                self.cells
                    .get(&key)
                    .is_some_and(|rs| rs.iter().any(|r| r.intersects(rect)))
            }) {
                continue;
            }
            let padded = Rect {
                x: x - gap,
                y: y - gap,
                w: w + 2 * gap,
                h: h + 2 * gap,
            };
            for key in Self::keys(padded) {
                self.cells.entry(key).or_default().push(padded);
            }
            self.placed += 1;
            return Some(rect);
        }
        None
    }
}

struct HeatLayer {
    colors: Vec<[u8; 4]>,
    ranks: Vec<f32>,
}
impl HeatLayer {
    fn new(pixels: usize) -> Self {
        Self {
            colors: vec![[0; 4]; pixels],
            ranks: vec![f32::NEG_INFINITY; pixels],
        }
    }
    fn paint(&mut self, i: usize, color: [u8; 4], rank: f32) {
        if rank > self.ranks[i] {
            self.ranks[i] = rank;
            self.colors[i] = color;
        }
    }
    fn composite(self, canvas: &mut Canvas<'_>) -> Result<(), ObservationError> {
        canvas.spend(self.colors.len())?;
        for (i, c) in self.colors.into_iter().enumerate() {
            if c[3] > 0 {
                canvas.pixel(i as i32 % canvas.width, i as i32 / canvas.width, c);
            }
        }
        Ok(())
    }
}

struct Canvas<'a> {
    rgba: &'a mut [u8],
    width: i32,
    height: i32,
    remaining: usize,
    labels: LabelLayout,
}
impl Canvas<'_> {
    fn spend(&mut self, n: usize) -> Result<(), ObservationError> {
        self.remaining = self.remaining.checked_sub(n).ok_or_else(|| ObservationError {
            code: "TOPOLOGY_BUDGET_EXCEEDED".into(),
            message: "Topology overlay exceeded 64000000 pixel visits; reduce resolution or disable strain/labels".into(),
            asset_id: None,
        })?;
        Ok(())
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let offset = ((y * self.width + x) * 4) as usize;
        let alpha = color[3] as u32;
        // Source-over in the renderer's premultiplied RGBA target.
        for (dst, src) in self.rgba[offset..offset + 3].iter_mut().zip(color) {
            *dst = ((src as u32 * alpha + *dst as u32 * (255 - alpha) + 127) / 255) as u8;
        }
        self.rgba[offset + 3] =
            (alpha + (self.rgba[offset + 3] as u32 * (255 - alpha) + 127) / 255) as u8;
    }
    fn line(&mut self, a: Point, b: Point, color: [u8; 4]) -> Result<(), ObservationError> {
        // Liang-Barsky clipping bounds work even for edges far outside the ROI.
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut low, mut high) = (0f64, 1f64);
        for (p, q) in [
            (-dx, a.0),
            (dx, self.width as f64 - 1. - a.0),
            (-dy, a.1),
            (dy, self.height as f64 - 1. - a.1),
        ] {
            if p == 0. {
                if q < 0. {
                    return Ok(());
                }
            } else if p < 0. {
                low = low.max(q / p);
            } else {
                high = high.min(q / p);
            }
        }
        if low > high {
            return Ok(());
        }
        let start = (a.0 + low * dx, a.1 + low * dy);
        let end = (a.0 + high * dx, a.1 + high * dy);
        let steps = (end.0 - start.0).abs().max((end.1 - start.1).abs()).ceil() as usize;
        self.spend(steps + 1)?;
        for i in 0..=steps {
            let t = if steps == 0 {
                0.
            } else {
                i as f64 / steps as f64
            };
            self.pixel(
                (start.0 + t * (end.0 - start.0)).round() as i32,
                (start.1 + t * (end.1 - start.1)).round() as i32,
                color,
            );
        }
        Ok(())
    }
    fn triangle(
        &mut self,
        p: [Point; 3],
        color: [u8; 4],
        heat: &mut HeatLayer,
        rank: f32,
    ) -> Result<(), ObservationError> {
        let cross =
            |a: Point, b: Point, c: Point| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        let signed = cross(p[0], p[1], p[2]);
        if signed == 0. {
            return Ok(());
        }
        let x0 = p
            .iter()
            .map(|p| p.0)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .clamp(0., self.width as f64) as i32;
        let x1 = p
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0., self.width as f64) as i32;
        let y0 = p
            .iter()
            .map(|p| p.1)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .clamp(0., self.height as f64) as i32;
        let y1 = p
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0., self.height as f64) as i32;
        self.spend(((x1 - x0) as usize).saturating_mul((y1 - y0) as usize))?;
        for y in y0..y1 {
            for x in x0..x1 {
                let q = (x as f64 + 0.5, y as f64 + 0.5);
                if (0..3).all(|i| cross(p[i], p[(i + 1) % 3], q) * signed >= 0.) {
                    heat.paint((y * self.width + x) as usize, color, rank);
                }
            }
        }
        Ok(())
    }
    fn vertex(
        &mut self,
        p: Point,
        id: Option<u32>,
        dot: bool,
        overlay: &TopologyOverlay,
    ) -> Result<(), ObservationError> {
        if p.0 < -100. || p.1 < -10. || p.0 > self.width as f64 || p.1 > self.height as f64 {
            return Ok(());
        }
        let (x, y) = (p.0.round() as i32, p.1.round() as i32);
        if dot {
            self.spend(9)?;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    self.pixel(x + dx, y + dy, YELLOW);
                }
            }
        }
        if let Some(id) = id {
            // 3x5 decimal glyphs, opaque black backing for readable persistent IDs.
            const DIGITS: [u16; 10] = [
                0b111_101_101_101_111,
                0b010_110_010_010_111,
                0b111_001_111_100_111,
                0b111_001_111_001_111,
                0b101_101_111_001_001,
                0b111_100_111_001_111,
                0b111_100_111_101_111,
                0b111_001_001_001_001,
                0b111_101_111_101_111,
                0b111_101_111_001_111,
            ];
            let label = id.to_string();
            let scale = overlay.label_scale as i32;
            let Some(rect) = self.labels.place(
                (x, y),
                label.len() as i32 * 4 * scale + 2,
                5 * scale + 2,
                self.width,
                self.height,
                overlay.label_spacing as i32,
                overlay.max_labels,
            ) else {
                return Ok(());
            };
            self.spend((rect.w * rect.h) as usize)?;
            self.line(
                p,
                (
                    x.clamp(rect.x, rect.x + rect.w - 1) as f64,
                    y.clamp(rect.y, rect.y + rect.h - 1) as f64,
                ),
                [255, 230, 40, 100],
            )?;
            for row in 0..rect.h {
                for col in 0..rect.w {
                    self.pixel(rect.x + col, rect.y + row, [0, 0, 0, 220]);
                }
            }
            for (i, digit) in label.bytes().enumerate() {
                let bits = DIGITS[(digit - b'0') as usize];
                for row in 0..5 {
                    for col in 0..3 {
                        if bits & (1 << (14 - row * 3 - col)) != 0 {
                            for dy in 0..scale {
                                for dx in 0..scale {
                                    self.pixel(
                                        rect.x + 1 + (i as i32 * 4 + col) * scale + dx,
                                        rect.y + 1 + row * scale + dy,
                                        YELLOW,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn draw(
    rgba: &mut [u8],
    request: &RenderRequest,
    view: ViewMapping,
    frame: &DrawableFrame,
    meshes: &[Mesh],
    parts: &[Part],
    quality: &[MeshQuality],
    overlay: &TopologyOverlay,
) -> Result<(), ObservationError> {
    let selected = selection(meshes, parts, Some(overlay))?;
    if overlay.topology_only {
        rgba.fill(0);
    }
    let mut canvas = Canvas {
        rgba,
        width: request.width as i32,
        height: request.height as i32,
        remaining: 64_000_000,
        labels: LabelLayout::default(),
    };
    let authored: HashMap<_, _> = meshes.iter().map(|m| (m.id.as_str(), m)).collect();
    let metrics: HashMap<_, _> = quality.iter().map(|m| (m.mesh_id.as_str(), m)).collect();
    let prepared: Vec<_> = frame
        .drawables
        .iter()
        .filter_map(|d| {
            if selected.as_ref().is_some_and(|ids| !ids.contains(&d.id))
                || (!overlay.include_hidden && !(d.enabled && d.visible && d.opacity > 0.))
            {
                return None;
            }
            let mesh = *authored.get(d.id.as_str())?;
            let positions: Vec<Point> = d
                .positions
                .iter()
                .map(|p| {
                    (
                        p.x as f64 * view.scale as f64 + view.offset.0 as f64,
                        -p.y as f64 * view.scale as f64 + view.offset.1 as f64,
                    )
                })
                .collect();
            let q = metrics.get(d.id.as_str()).copied();
            let active: Vec<bool> = (0..d.indices.len() / 3)
                .map(|i| {
                    !overlay.anomalies_only
                        || q.is_some_and(|q| overlay.thresholds.is_anomaly(&q.triangles[i]))
                })
                .collect();
            Some((d, mesh, positions, q, active))
        })
        .collect();
    // Compose the most severe heat once. Shared edges and overlapping meshes cannot
    // exceed the requested fill opacity, and later wire/label passes stay legible.
    if overlay.strain && overlay.heat_opacity > 0. {
        let mut heat = HeatLayer::new(request.width as usize * request.height as usize);
        for (d, _, positions, q, active) in &prepared {
            if let Some(q) = q {
                for ((indices, t), active) in
                    d.indices.chunks_exact(3).zip(&q.triangles).zip(active)
                {
                    if !active {
                        continue;
                    }
                    let invalid = t.area == 0.
                        || t.relative_area.is_none_or(|v| v < 0.)
                        || t.edge_stretch.is_none();
                    let stretch = t.edge_stretch.unwrap_or(4.).clamp(1., 4.);
                    let shape = overlay.anomalies_only
                        && (t.min_angle_degrees < overlay.thresholds.min_angle_degrees
                            || t.aspect_ratio
                                .is_none_or(|v| v > overlay.thresholds.max_aspect_ratio));
                    let mut color = if invalid {
                        MAGENTA
                    } else if shape && stretch <= overlay.thresholds.max_edge_stretch {
                        [255, 170, 30, 255]
                    } else {
                        [
                            ((stretch - 1.).min(1.) * 255.) as u8,
                            ((4. - stretch) / 3. * 100.) as u8,
                            ((2. - stretch).clamp(0., 1.) * 255.) as u8,
                            255,
                        ]
                    };
                    color[3] = (overlay.heat_opacity * 255.).round() as u8;
                    let rank = if invalid {
                        f32::INFINITY
                    } else if shape {
                        (t.edge_stretch.unwrap_or(4.) as f32).max(2.)
                    } else {
                        t.edge_stretch.unwrap_or(4.) as f32
                    };
                    canvas.triangle(
                        std::array::from_fn(|i| positions[indices[i] as usize]),
                        color,
                        &mut heat,
                        rank,
                    )?;
                }
            }
        }
        heat.composite(&mut canvas)?;
    }
    let mut labels = Vec::new();
    for (d, mesh, positions, q, active) in &prepared {
        let mut edges = HashSet::new();
        let mut vertices = vec![!overlay.anomalies_only; positions.len()];
        for (tri, active) in d.indices.chunks_exact(3).zip(active) {
            if !active {
                continue;
            }
            for i in 0..3 {
                let (a, b) = (tri[i], tri[(i + 1) % 3]);
                edges.insert((a.min(b), a.max(b)));
                vertices[a as usize] = true;
            }
        }
        if overlay.wireframe {
            // Preserve source triangle ordering for deterministic antialiased crossings.
            let mut drawn = HashSet::new();
            for (tri, active) in d.indices.chunks_exact(3).zip(active) {
                if *active {
                    for i in 0..3 {
                        let (a, b) = (tri[i], tri[(i + 1) % 3]);
                        if drawn.insert((a.min(b), a.max(b))) {
                            canvas.line(positions[a as usize], positions[b as usize], CYAN)?;
                        }
                    }
                }
            }
        }
        if overlay.strain && overlay.heat_opacity > 0. {
            if let Some(q) = q {
                for ((tri, t), active) in d.indices.chunks_exact(3).zip(&q.triangles).zip(active) {
                    if *active && t.area == 0. {
                        for i in 0..3 {
                            canvas.line(
                                positions[tri[i] as usize],
                                positions[tri[(i + 1) % 3] as usize],
                                MAGENTA,
                            )?;
                        }
                    }
                }
            }
        }
        if overlay.constraints {
            let slots: HashMap<_, _> = mesh
                .vertex_ids
                .iter()
                .enumerate()
                .map(|(i, &v)| (v, i))
                .collect();
            if let Some(meshing) = &mesh.meshing {
                let mut edge = |a, b, color| -> Result<(), ObservationError> {
                    if let (Some(&a), Some(&b)) = (slots.get(&a), slots.get(&b)) {
                        if !overlay.anomalies_only
                            || edges.contains(&(a.min(b) as u32, a.max(b) as u32))
                        {
                            canvas.line(positions[a], positions[b], color)?;
                        }
                    }
                    Ok(())
                };
                if meshing.boundary.len() > 1 {
                    for i in 0..meshing.boundary.len() {
                        edge(
                            meshing.boundary[i],
                            meshing.boundary[(i + 1) % meshing.boundary.len()],
                            GREEN,
                        )?;
                    }
                }
                for c in &meshing.constraints {
                    edge(c.endpoints[0], c.endpoints[1], MAGENTA)?;
                }
            }
        }
        for (i, (&p, &id)) in positions.iter().zip(&mesh.vertex_ids).enumerate() {
            if vertices[i] {
                if overlay.vertices {
                    canvas.vertex(p, None, true, overlay)?;
                }
                if overlay.vertex_ids {
                    labels.push((p, id));
                }
            }
        }
    }
    // Labels are placed after every fill, edge and dot, so other mesh layers cannot paint over them.
    for (p, id) in labels {
        canvas.vertex(p, Some(id), false, overlay)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clips_lines_and_composites_premultiplied_heat() {
        let mut rgba = vec![0; 8 * 8 * 4];
        let mut c = Canvas {
            rgba: &mut rgba,
            width: 8,
            height: 8,
            remaining: 1000,
            labels: LabelLayout::default(),
        };
        c.line((-1e6, 4.), (1e6, 4.), CYAN).unwrap();
        assert_eq!(&c.rgba[(4 * 8 + 4) * 4..(4 * 8 + 4) * 4 + 4], &CYAN);
        let mut heat = HeatLayer::new(64);
        c.triangle(
            [(0., 0.), (8., 0.), (0., 8.)],
            [255, 0, 0, 128],
            &mut heat,
            1.,
        )
        .unwrap();
        heat.composite(&mut c).unwrap();
        assert_eq!(&c.rgba[0..4], &[128, 0, 0, 128]);
        c.vertex((2., 2.), Some(42), true, &TopologyOverlay::default())
            .unwrap();
        assert_eq!(&c.rgba[(2 * 8 + 2) * 4..(2 * 8 + 2) * 4 + 4], &YELLOW);
        c.remaining = 0;
        assert_eq!(
            c.line((0., 0.), (7., 7.), CYAN).unwrap_err().code,
            "TOPOLOGY_BUDGET_EXCEEDED"
        );
    }

    #[test]
    fn labels_find_nearby_space_and_omit_dense_or_outside_ids_deterministically() {
        let layout = || {
            let mut labels = LabelLayout::default();
            (0..50)
                .filter_map(|_| labels.place((100, 100), 26, 12, 200, 200, 3, 256))
                .collect::<Vec<_>>()
        };
        let placed = layout();
        assert_eq!(placed, layout());
        assert!(!placed.is_empty() && placed.len() < 50);
        for (i, r) in placed.iter().enumerate() {
            assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= 200 && r.y + r.h <= 200);
            let padded = Rect {
                x: r.x - 3,
                y: r.y - 3,
                w: r.w + 6,
                h: r.h + 6,
            };
            for other in &placed[i + 1..] {
                assert!(!padded.intersects(*other));
            }
        }
        let mut labels = LabelLayout::default();
        assert!(labels.place((20, 20), 26, 12, 200, 200, 3, 1).is_some());
        assert!(labels.place((100, 100), 26, 12, 200, 200, 3, 1).is_none());
        assert!(labels.place((-1000, 0), 26, 12, 200, 200, 3, 256).is_none());
    }

    #[test]
    fn overlapping_heat_uses_worst_triangle_and_blends_once() {
        let mut rgba = vec![0; 64 * 4];
        let mut canvas = Canvas {
            rgba: &mut rgba,
            width: 8,
            height: 8,
            remaining: 1000,
            labels: LabelLayout::default(),
        };
        let mut heat = HeatLayer::new(64);
        let tri = [(0., 0.), (8., 0.), (0., 8.)];
        canvas
            .triangle(tri, [0, 0, 255, 64], &mut heat, 1.)
            .unwrap();
        canvas
            .triangle(tri, [255, 0, 0, 64], &mut heat, 4.)
            .unwrap();
        canvas
            .triangle(tri, [0, 0, 255, 64], &mut heat, 1.)
            .unwrap();
        heat.composite(&mut canvas).unwrap();
        assert_eq!(&canvas.rgba[0..4], &[64, 0, 0, 64]);
    }

    #[test]
    fn selections_union_meshes_and_nested_parts_and_validate_missing_ids() {
        use zipzi_sdk::ObjectTarget::{Mesh as Target, Part as Group};
        let meshes = vec![
            Mesh {
                id: "a".into(),
                part_id: "root".into(),
                ..Default::default()
            },
            Mesh {
                id: "b".into(),
                part_id: "child".into(),
                ..Default::default()
            },
            Mesh {
                id: "c".into(),
                ..Default::default()
            },
        ];
        let parts = vec![
            Part {
                id: "root".into(),
                ..Default::default()
            },
            Part {
                id: "child".into(),
                parent_id: "root".into(),
                ..Default::default()
            },
        ];
        let overlay = TopologyOverlay {
            targets: Some(vec![Group("root".into()), Target("b".into())]),
            ..Default::default()
        };
        assert_eq!(
            selection(&meshes, &parts, Some(&overlay)).unwrap().unwrap(),
            HashSet::from(["a".into(), "b".into()])
        );
        assert_eq!(
            selection(
                &meshes,
                &parts,
                Some(&TopologyOverlay {
                    targets: Some(vec![]),
                    ..Default::default()
                })
            )
            .unwrap(),
            Some(HashSet::new())
        );
        assert_eq!(selection(&meshes, &parts, None).unwrap(), None);
        assert_eq!(
            selection(
                &meshes,
                &parts,
                Some(&TopologyOverlay {
                    targets: Some(vec![Target("missing".into())]),
                    ..Default::default()
                })
            )
            .unwrap_err()
            .code,
            "MISSING_MESH"
        );
        assert_eq!(
            selection(
                &meshes,
                &parts,
                Some(&TopologyOverlay {
                    targets: Some(vec![Group("missing".into())]),
                    ..Default::default()
                })
            )
            .unwrap_err()
            .code,
            "MISSING_PART"
        );
    }

    #[test]
    fn anomalies_filter_wire_vertices_and_labels_while_zero_heat_leaves_context_unchanged() {
        use zipzi_core::{Drawable, Vec2};
        let mut reference = DrawableFrame::default();
        reference.drawables.push(Drawable {
            id: "mesh".into(),
            positions: vec![
                Vec2::new(8., -8.),
                Vec2::new(40., -8.),
                Vec2::new(8., -40.),
                Vec2::new(40., -40.),
            ],
            indices: vec![0, 1, 2, 1, 3, 2].into(),
            ..Default::default()
        });
        let mut frame = reference.clone();
        frame.drawables[0].positions[3] = Vec2::new(20., -20.);
        let quality = zipzi_sdk::mesh_quality(&frame, &reference, &Default::default()).unwrap();
        assert!(quality[0].triangles[1].relative_area.unwrap() < 0.);
        let meshes = vec![Mesh {
            id: "mesh".into(),
            vertex_ids: vec![10, 20, 30, 40],
            ..Default::default()
        }];
        let request = RenderRequest {
            width: 64,
            height: 64,
            roi: crate::ModelRoi {
                x0: 0.,
                y0: -64.,
                x1: 64.,
                y1: 0.,
            },
            padding_model: 0.,
            topology: None,
            quality: false,
        };
        let mapping = request.mapping().unwrap();
        let mut rgba = vec![0; 64 * 64 * 4];
        draw(
            &mut rgba,
            &request,
            mapping,
            &frame,
            &meshes,
            &[],
            &quality,
            &TopologyOverlay {
                anomalies_only: true,
                vertices: true,
                include_hidden: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(&rgba[(8 * 64 + 8) * 4..(8 * 64 + 8) * 4 + 4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[(8 * 64 + 40) * 4..(8 * 64 + 40) * 4 + 4], &YELLOW);
        let mut untouched = vec![128; 64 * 64 * 4];
        let original = untouched.clone();
        draw(
            &mut untouched,
            &request,
            mapping,
            &frame,
            &meshes,
            &[],
            &quality,
            &TopologyOverlay {
                wireframe: false,
                strain: true,
                heat_opacity: 0.,
                include_hidden: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(untouched, original);
    }
}
