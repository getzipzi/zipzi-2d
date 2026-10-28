// SPDX-License-Identifier: GPL-3.0-only
//! PSD2Live-derived automatic meshing. Independent contour/band and fill policies.
//! See README.md for provenance, numerical substitutions and focused validation.
mod bands;
mod contour;
mod existing;
mod fill;
mod geometry;
mod ribbon;
mod structured;
mod triangulate;
pub use existing::fill_existing_geometry;
use geometry::*;
use std::collections::{BTreeMap, BTreeSet};
use zipzi_core::Vec2;
use zipzi_sdk::{MeshGeometry, extract_manual_meshing};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshEdgeMode {
    Single,
    Double,
    Triple,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshFillAlgorithm {
    GradedPoisson,
    SimpleTriangles,
    AdaptiveQuadtree,
    TriangleFractal,
    ContourPaving,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FillParameters {
    pub edge_ratio: f64,
    pub gradation: f64,
    pub jitter: f64,
    pub angle: f64,
    pub max_rows: usize,
}
impl Default for FillParameters {
    fn default() -> Self {
        Self {
            edge_ratio: 2.,
            gradation: 1.,
            jitter: 0.45,
            angle: 0.,
            max_rows: 12,
        }
    }
}
/// Each strategy retains its controls when the selected strategy changes.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MeshFillParameters {
    pub poisson: FillParameters,
    pub quadtree: FillParameters,
    pub fractal: FillParameters,
    pub paving: FillParameters,
}
impl Default for MeshFillParameters {
    fn default() -> Self {
        Self {
            poisson: FillParameters::default(),
            quadtree: FillParameters::default(),
            fractal: FillParameters {
                gradation: 2.,
                ..FillParameters::default()
            },
            paving: FillParameters::default(),
        }
    }
}
impl MeshFillParameters {
    pub fn get(self, kind: MeshFillAlgorithm) -> FillParameters {
        match kind {
            MeshFillAlgorithm::GradedPoisson | MeshFillAlgorithm::SimpleTriangles => self.poisson,
            MeshFillAlgorithm::AdaptiveQuadtree => self.quadtree,
            MeshFillAlgorithm::TriangleFractal => self.fractal,
            MeshFillAlgorithm::ContourPaving => self.paving,
        }
    }
    pub fn get_mut(&mut self, kind: MeshFillAlgorithm) -> &mut FillParameters {
        match kind {
            MeshFillAlgorithm::GradedPoisson | MeshFillAlgorithm::SimpleTriangles => {
                &mut self.poisson
            }
            MeshFillAlgorithm::AdaptiveQuadtree => &mut self.quadtree,
            MeshFillAlgorithm::TriangleFractal => &mut self.fractal,
            MeshFillAlgorithm::ContourPaving => &mut self.paving,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MeshSettings {
    pub edge_mode: MeshEdgeMode,
    pub edge_spacing: f64,
    pub edge_width: f64,
    pub outer_margin: f64,
    pub fill_algorithm: MeshFillAlgorithm,
    pub interior_spacing: f64,
    pub fill_parameters: MeshFillParameters,
    pub suppress_boundary_diagonals: bool,
    pub alpha_threshold: u8,
    pub wrap: f64,
    /// Source pixels per mesh unit. 1 uses source pixels; document units use max(width,height)/2048.
    pub unit_scale: f64,
    /// Working pixels per mesh unit, clamped by source resolution and a 1 Mi-pixel tracing budget.
    pub detail: f64,
    pub clip_to_image: bool,
    pub max_vertices: usize,
}
impl Default for MeshSettings {
    fn default() -> Self {
        Self {
            edge_mode: MeshEdgeMode::Single,
            edge_spacing: 6.,
            edge_width: 10.,
            outer_margin: 1.,
            fill_algorithm: MeshFillAlgorithm::GradedPoisson,
            interior_spacing: 40.,
            fill_parameters: MeshFillParameters::default(),
            suppress_boundary_diagonals: false,
            alpha_threshold: 8,
            wrap: 0.,
            unit_scale: 1.,
            detail: 1.,
            clip_to_image: false,
            max_vertices: 65_534,
        }
    }
}
impl MeshSettings {
    pub fn validate(&self) -> Result<(), MeshError> {
        if [
            self.edge_spacing,
            self.interior_spacing,
            self.unit_scale,
            self.detail,
        ]
        .iter()
        .any(|n| !n.is_finite() || *n <= 0. || *n > 1e6)
            || [self.edge_width, self.outer_margin, self.wrap]
                .iter()
                .any(|n| !n.is_finite() || *n < 0.)
            || self.edge_width > 1e6
            || self.outer_margin > 1e6
            || self.wrap > 64.
            || !(3..=65_534).contains(&self.max_vertices)
        {
            return Err(MeshError::Options);
        }
        for p in [
            self.fill_parameters.poisson,
            self.fill_parameters.quadtree,
            self.fill_parameters.fractal,
            self.fill_parameters.paving,
        ] {
            if ![p.edge_ratio, p.gradation, p.jitter, p.angle]
                .iter()
                .all(|n| n.is_finite())
                || !(1. ..=4.).contains(&p.edge_ratio)
                || !(0.25..=4.).contains(&p.gradation)
                || !(0. ..=1.).contains(&p.jitter)
                || !(0. ..=90.).contains(&p.angle)
                || p.max_rows > 24
            {
                return Err(MeshError::Options);
            }
        }
        Ok(())
    }
}
#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    #[error("Invalid alpha mask dimensions")]
    Input,
    #[error("No foreground after alpha preprocessing")]
    Empty,
    #[error("Invalid mesh settings")]
    Options,
    #[error("Mesh generation exceeds its work or vertex limit")]
    Limit,
    #[error("Mesh generation failed: {0}")]
    Geometry(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshGenerationStage {
    Edges,
    Full,
}
#[derive(Clone, Debug)]
pub struct GeneratedMesh {
    pub geometry: MeshGeometry,
    pub boundary_loops: Vec<Vec<u32>>,
    pub middle_loops: Vec<Vec<u32>>,
    pub inner_loops: Vec<Vec<u32>>,
    /// New fill points only, excluding boundary and band rows.
    pub interior_uvs: Vec<Vec2>,
    /// Number of components that needed a narrower band or raw-contour recovery.
    pub recovered_components: usize,
}
struct Component {
    mesh: Mesh,
    boundaries: Vec<Vec<usize>>,
    middle: Vec<Vec<usize>>,
    inner: Vec<Vec<usize>>,
    fill_ids: Vec<usize>,
}
fn component(
    loops: &[Vec<Point>],
    settings: &MeshSettings,
    spacing: f64,
    empty: bool,
) -> Option<Component> {
    let mesh = fill::generate(
        loops,
        settings.fill_algorithm,
        settings.fill_parameters.get(settings.fill_algorithm),
        spacing,
        settings.suppress_boundary_diagonals,
        empty,
    )?;
    let n = loops.iter().map(Vec::len).sum::<usize>();
    let fill_ids = if empty {
        vec![]
    } else {
        (n..mesh.points.len()).collect()
    };
    Some(Component {
        mesh,
        boundaries: loop_ids(loops),
        middle: vec![],
        inner: vec![],
        fill_ids,
    })
}
fn banded(
    interleaved: &[Vec<Point>],
    neighbors: &[Vec<Point>],
    s: &MeshSettings,
    width: f64,
    spacing: f64,
    empty: bool,
) -> Option<Component> {
    let rows = bands::build(interleaved, s.edge_mode, width, neighbors)?;
    let loops: Loops = rows.iter().map(|r| r.last().unwrap().clone()).collect();
    let mut c = component(
        &loops,
        &MeshSettings {
            suppress_boundary_diagonals: false,
            ..*s
        },
        spacing,
        empty,
    )?;
    c.inner = c.boundaries.clone();
    c.boundaries.clear();
    let mut offset = 0;
    for r in rows {
        let count = r.last()?.len();
        let inner: Vec<_> = (offset..offset + count).collect();
        offset += count;
        let mut ids = vec![];
        for (i, row) in r.iter().enumerate() {
            if i == r.len() - 1 {
                ids.push(inner.clone())
            } else {
                let n = c.mesh.points.len();
                c.mesh.points.extend(row);
                ids.push((n..n + row.len()).collect());
            }
        }
        c.mesh
            .triangles
            .extend(bands::strip(&ids, &bands::staggered(s.edge_mode)));
        c.boundaries.push(ids[0].clone());
        if s.edge_mode == MeshEdgeMode::Triple {
            c.middle.push(ids[1].clone());
        }
    }
    valid_mesh(&c.mesh, &c.boundaries).then_some(c)
}
fn overlap(a: &[Vec<Point>], b: &[Vec<Point>], contact: bool) -> bool {
    a.iter().any(|r| {
        b.iter().any(|s| {
            if contact {
                (0..r.len()).any(|i| {
                    (0..s.len())
                        .any(|j| intersects(r[i], r[(i + 1) % r.len()], s[j], s[(j + 1) % s.len()]))
                })
            } else {
                touches(r, s)
            }
        })
    }) || a.iter().flatten().any(|&p| {
        in_domain(p, b)
            && !b
                .iter()
                .any(|r| (0..r.len()).any(|i| on_segment(p, r[i], r[(i + 1) % r.len()])))
    }) || b.iter().flatten().any(|&p| {
        in_domain(p, a)
            && !a
                .iter()
                .any(|r| (0..r.len()).any(|i| on_segment(p, r[i], r[(i + 1) % r.len()])))
    })
}
/// Pure, deterministic generation. Alpha is top-down; output positions and UVs are bottom-up.
pub fn generate_mesh(
    width: u32,
    height: u32,
    alpha: &[u8],
    settings: &MeshSettings,
    stage: MeshGenerationStage,
) -> Result<GeneratedMesh, MeshError> {
    settings.validate()?;
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || w.checked_mul(h) != Some(alpha.len()) {
        return Err(MeshError::Input);
    }
    if w > 16384 || h > 16384 || alpha.len() > 64 * 1024 * 1024 {
        return Err(MeshError::Limit);
    }
    // Maximum-alpha reduction preserves fine strokes. Settings and tolerances share mesh units.
    let detail = settings
        .detail
        .clamp(1., 4.)
        .min(settings.unit_scale.max(1.))
        .min(
            (1_048_576. / (w as f64 * h as f64 / settings.unit_scale.powi(2)))
                .sqrt()
                .max(1.),
        );
    let factor = (settings.unit_scale / detail).max(1.);
    let factor = if factor < 1.05 { 1. } else { factor };
    let rw = (w as f64 / factor).ceil() as usize;
    let rh = (h as f64 / factor).ceil() as usize;
    let mut reduced = vec![0; rw * rh];
    for y in 0..rh {
        for x in 0..rw {
            let mut peak = 0;
            for sy in (y as f64 * factor).floor() as usize
                ..(((y + 1) as f64 * factor).ceil() as usize).min(h)
            {
                for sx in (x as f64 * factor).floor() as usize
                    ..(((x + 1) as f64 * factor).ceil() as usize).min(w)
                {
                    peak = peak.max(alpha[sy * w + sx]);
                }
            }
            reduced[y * rw + x] = peak;
        }
    }
    let unit = if factor > 1. {
        settings.unit_scale / factor
    } else if detail > 1. {
        settings.unit_scale.max(1.)
    } else {
        1.
    };
    // Work in mesh units, including all geometric tolerances, then scale once on output.
    // Bound the densest possible Poisson candidate lattice before allocation.
    if (rw as f64 / unit / 2.88).ceil() * (rh as f64 / unit / 2.88).ceil() > 1_048_576. {
        return Err(MeshError::Limit);
    }
    let raw = contour::domains(
        rw,
        rh,
        &reduced,
        &MeshSettings {
            wrap: settings.wrap * unit,
            ..*settings
        },
    )?;
    let raw: Vec<Loops> = raw
        .into_iter()
        .map(|ls| {
            ls.into_iter()
                .map(|r| r.into_iter().map(|p| mul(p, 1. / unit)).collect())
                .collect()
        })
        .collect();
    if raw.len() > 1024 || raw.iter().flatten().map(Vec::len).sum::<usize>() > 65_534 {
        return Err(MeshError::Limit);
    }
    let edge_spacing = settings.edge_spacing.max(6.);
    let spacing = settings
        .interior_spacing
        .max(6.)
        .max((rw as f64 * rh as f64 / (1200. * H)).sqrt() / unit);
    let empty = stage == MeshGenerationStage::Edges;
    let mut guides: Vec<Loops> = raw
        .iter()
        .map(|loops| {
            loops
                .iter()
                .map(|r| contour::guide(r, edge_spacing))
                .collect()
        })
        .collect();
    for i in 0..guides.len() {
        let stations: Loops = guides[i]
            .iter()
            .map(|r| r.iter().step_by(2).copied().collect())
            .collect();
        if !valid_domain(&stations)
            || raw
                .iter()
                .enumerate()
                .any(|(j, d)| i != j && overlap(&stations, d, false))
        {
            guides[i] = raw[i].iter().map(|r| contour::interleave(r)).collect();
        }
    }
    let mut components: Vec<Component> = vec![];
    let mut recovered = 0;
    for (i, domain) in raw.iter().enumerate() {
        let neighbors: Loops = raw
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .flat_map(|(_, d)| d.clone())
            .collect();
        let isolated = |c: &Component, contact: bool| {
            let b: Loops = c
                .boundaries
                .iter()
                .map(|r| r.iter().map(|&i| c.mesh.points[i]).collect())
                .collect();
            raw.iter()
                .enumerate()
                .all(|(j, d)| j == i || !overlap(&b, d, contact))
                && components.iter().all(|c| {
                    let other: Loops = c
                        .boundaries
                        .iter()
                        .map(|r| r.iter().map(|&i| c.mesh.points[i]).collect())
                        .collect();
                    !overlap(&b, &other, contact)
                })
        };
        let mut built = None;
        let mut recovery = false;
        if domain.len() == 1 {
            for scale in [1., 0.5, 0.25, 0.] {
                if let Some((mesh, border)) = ribbon::build(
                    &domain[0],
                    &neighbors,
                    edge_spacing,
                    scale,
                    if scale < 0.5 { 0.25 } else { 1. },
                ) {
                    let mut c = Component {
                        mesh,
                        boundaries: vec![border],
                        middle: vec![],
                        inner: vec![],
                        fill_ids: vec![],
                    };
                    if settings.suppress_boundary_diagonals {
                        let loops: Loops = c
                            .boundaries
                            .iter()
                            .map(|r| r.iter().map(|&i| c.mesh.points[i]).collect())
                            .collect();
                        if let Some(rebuilt) = component(&loops, settings, spacing, empty) {
                            c = rebuilt
                        }
                    }
                    if isolated(&c, false) {
                        built = Some(c);
                        recovery = scale != 1.;
                        break;
                    }
                }
            }
        }
        if built.is_none() && settings.edge_mode != MeshEdgeMode::Single {
            for scale in [1., 0.6, 0.35] {
                if let Some(c) = banded(
                    &guides[i],
                    &neighbors,
                    settings,
                    settings.edge_width * scale,
                    spacing,
                    empty,
                ) {
                    if isolated(&c, false) {
                        built = Some(c);
                        recovery = scale != 1.;
                        break;
                    }
                }
            }
        }
        let stations: Loops = guides[i]
            .iter()
            .map(|r| r.iter().step_by(2).copied().collect())
            .collect();
        if built.is_none() {
            for scale in [1., 0.5, 0.25, 0.125, 0.0625] {
                let outer: Loops = stations
                    .iter()
                    .enumerate()
                    .map(|(ri, r)| {
                        let mut barriers = neighbors.clone();
                        barriers.extend(
                            stations
                                .iter()
                                .enumerate()
                                .filter(|(j, _)| *j != ri)
                                .map(|(_, r)| r.clone()),
                        );
                        bands::offset(r, -settings.outer_margin * scale, &barriers)
                    })
                    .collect();
                if let Some(c) = component(&outer, settings, spacing, empty) {
                    if isolated(&c, false) {
                        built = Some(c);
                        recovery = scale != 1. || settings.edge_mode != MeshEdgeMode::Single;
                        break;
                    }
                }
            }
        }
        if built.is_none() {
            for loops in [&stations, domain] {
                for suppress in [settings.suppress_boundary_diagonals, false] {
                    if let Some(c) = component(
                        loops,
                        &MeshSettings {
                            suppress_boundary_diagonals: suppress,
                            ..*settings
                        },
                        spacing,
                        empty,
                    ) {
                        if isolated(&c, true) {
                            built = Some(c);
                            break;
                        }
                    }
                }
                if built.is_some() {
                    break;
                }
            }
            recovery = true;
        }
        let c = built.ok_or(MeshError::Geometry("could not mesh a contour component"))?;
        if recovery {
            recovered += 1
        }
        components.push(c);
    }
    let mut mesh = Mesh::default();
    let mut boundaries = vec![];
    let mut middle = vec![];
    let mut inner = vec![];
    let mut fill_ids = vec![];
    for c in components {
        let offset = mesh.points.len();
        let shift = |rows: Vec<Vec<usize>>| {
            rows.into_iter()
                .map(|r| {
                    r.into_iter()
                        .rev()
                        .map(|i| (i + offset) as u32)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        fill_ids.extend(c.fill_ids.iter().map(|i| i + offset));
        boundaries.extend(shift(c.boundaries));
        middle.extend(shift(c.middle));
        inner.extend(shift(c.inner));
        mesh.points.extend(
            c.mesh
                .points
                .into_iter()
                .map(|p| snap([p[0] * unit * factor, height as f64 - p[1] * unit * factor])),
        );
        mesh.triangles.extend(
            c.mesh
                .triangles
                .into_iter()
                .map(|t| [t[0] + offset, t[2] + offset, t[1] + offset]),
        );
    }
    if mesh.points.len() > settings.max_vertices {
        return Err(MeshError::Limit);
    }
    let interior_uvs = fill_ids
        .into_iter()
        .map(|i| {
            Vec2::new(
                mesh.points[i][0] as f32 / width as f32,
                mesh.points[i][1] as f32 / height as f32,
            )
        })
        .collect();
    if settings.clip_to_image
        && mesh
            .points
            .iter()
            .any(|p| p[0] < 0. || p[1] < 0. || p[0] > width as f64 || p[1] > height as f64)
    {
        mesh = clip(mesh, width as f64, height as f64);
        boundaries = trace_boundaries(&mesh)?;
        middle.clear();
        inner.clear();
    }
    if mesh.points.len() > settings.max_vertices {
        return Err(MeshError::Limit);
    }
    if mesh.triangles.iter().any(|t| {
        let [a, b, c] = t.map(|i| mesh.points[i]);
        cross(a, b, c) <= EPS
    }) {
        return Err(MeshError::Geometry(
            "triangle collapses at output precision",
        ));
    }
    let mut geometry = MeshGeometry {
        positions: mesh
            .points
            .iter()
            .map(|p| Vec2::new(p[0] as f32, p[1] as f32))
            .collect(),
        uvs: mesh
            .points
            .iter()
            .map(|p| Vec2::new(p[0] as f32 / width as f32, p[1] as f32 / height as f32))
            .collect(),
        vertex_ids: (0..mesh.points.len() as u32).collect(),
        triangles: mesh.triangles.iter().map(|t| t.map(|i| i as u32)).collect(),
        meshing: None,
    };
    // Constraints used to build bands and patterned fills are generation-only.
    // Publish their triangles as ordinary editable edges; persistent constraints
    // are created only by explicit user edits.
    if let Ok(meshing) = extract_manual_meshing(&geometry) {
        if boundaries.is_empty() {
            boundaries.push(meshing.boundary.clone())
        }
        geometry.meshing = Some(meshing);
    }
    if geometry.triangles.is_empty() {
        return Err(MeshError::Geometry("empty generated geometry"));
    }
    Ok(GeneratedMesh {
        geometry,
        boundary_loops: boundaries,
        middle_loops: middle,
        inner_loops: inner,
        interior_uvs,
        recovered_components: recovered,
    })
}
fn trace_boundaries(mesh: &Mesh) -> Result<Vec<Vec<u32>>, MeshError> {
    let mut directed = BTreeSet::new();
    for t in &mesh.triangles {
        for i in 0..3 {
            directed.insert((t[i], t[(i + 1) % 3]));
        }
    }
    let mut next = BTreeMap::new();
    for &(a, b) in &directed {
        if !directed.contains(&(b, a)) && next.insert(a, b).is_some() {
            return Err(MeshError::Geometry("clipped boundary has a pinch"));
        }
    }
    let mut loops = vec![];
    while let Some((&start, _)) = next.first_key_value() {
        let mut row = vec![];
        let mut v = start;
        loop {
            row.push(v as u32);
            v = next
                .remove(&v)
                .ok_or(MeshError::Geometry("clipped boundary is open"))?;
            if v == start {
                break;
            }
        }
        if row.len() < 3 {
            return Err(MeshError::Geometry("clipped boundary collapsed"));
        }
        loops.push(row);
    }
    Ok(loops)
}
fn clip(mesh: Mesh, w: f64, h: f64) -> Mesh {
    // Keep corner-touching islands separate, as the upstream generator does.
    let mut parent: Vec<_> = (0..mesh.points.len()).collect();
    fn root(parent: &[usize], mut i: usize) -> usize {
        while parent[i] != i {
            i = parent[i];
        }
        i
    }
    for t in &mesh.triangles {
        let a = root(&parent, t[0]);
        for &b in &t[1..] {
            let b = root(&parent, b);
            parent[b] = a;
        }
    }
    let groups: Vec<_> = (0..parent.len()).map(|i| root(&parent, i)).collect();
    let mut out = Mesh::default();
    let mut ids = BTreeMap::new();
    for t in mesh.triangles {
        let mut poly: Vec<_> = t.map(|i| mesh.points[i]).to_vec();
        for (axis, bound, sign) in [(0, 0., 1.), (0, w, -1.), (1, 0., 1.), (1, h, -1.)] {
            let mut next = vec![];
            for i in 0..poly.len() {
                let a = poly[i];
                let b = poly[(i + 1) % poly.len()];
                let ia = (a[axis] - bound) * sign >= 0.;
                let ib = (b[axis] - bound) * sign >= 0.;
                if ia {
                    next.push(a)
                }
                if ia != ib {
                    let mut p = lerp(a, b, (bound - a[axis]) / (b[axis] - a[axis]));
                    p[axis] = bound;
                    next.push(snap(p));
                }
            }
            poly = next;
        }
        if poly.len() < 3 {
            continue;
        }
        for j in 1..poly.len() - 1 {
            let p = [poly[0], poly[j], poly[j + 1]];
            if cross(p[0], p[1], p[2]) <= EPS {
                continue;
            }
            let t = p.map(|p| {
                *ids.entry((groups[t[0]], p[0].to_bits(), p[1].to_bits()))
                    .or_insert_with(|| {
                        let i = out.points.len();
                        out.points.push(p);
                        i
                    })
            });
            out.triangles.push(t);
        }
    }
    out
}
#[cfg(test)]
mod tests;

pub mod remote;
