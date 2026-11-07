//! Source-bound UV sampling and complete, atomic dependency transfer.
use crate::{manual_mesh::edit_manual_uv_candidate, *};
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use std::collections::{HashMap, HashSet};
use zipzi_core::state::DocumentState;
use zipzi_core::{DeltaKeyforms, Mesh, Vec2, VertexId};

fn error(code: &str, message: impl Into<String>, ids: Vec<String>) -> Status {
    Status::error(code, format!("{} [{}]", message.into(), ids.join(", ")))
}
fn image_domain(g: &MeshGeometry, mesh_id: &str) -> Result<(), Status> {
    if let Some((slot, _)) = g.uvs.iter().enumerate().find(|(_, uv)| {
        !uv.x.is_finite()
            || !uv.y.is_finite()
            || !(0.0..=1.0).contains(&uv.x)
            || !(0.0..=1.0).contains(&uv.y)
    }) {
        let mut e = error(
            "UV_OUTSIDE_SOURCE_IMAGE",
            "UV lies outside the complete source image",
            vec![mesh_id.into()],
        );
        e.message.push_str(&format!("uvs[{slot}]"));
        return Err(e);
    }
    Ok(())
}
fn status(s: Status) -> Status {
    s
}
#[derive(Clone, Debug)]
struct UvTriangle {
    ids: [VertexId; 3],
    slots: [usize; 3],
    uv: [[f64; 2]; 3],
}
fn barycentric(p: [f64; 2], [a, b, c]: [[f64; 2]; 3]) -> Option<[f64; 3]> {
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let det = cross(a, b, c);
    if det == 0. {
        return None;
    }
    let w = [
        cross(p, b, c) / det,
        cross(a, p, c) / det,
        cross(a, b, p) / det,
    ];
    w.iter().all(|w| w.is_finite()).then_some(w)
}
impl PointDistance for UvTriangle {
    fn distance_2(&self, p: &[f64; 2]) -> f64 {
        if barycentric(*p, self.uv).is_some_and(|w| w.iter().all(|w| *w >= 0.)) {
            return 0.;
        }
        (0..3)
            .map(|i| {
                let a = self.uv[i];
                let b = self.uv[(i + 1) % 3];
                let d = [b[0] - a[0], b[1] - a[1]];
                let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1])
                    / (d[0] * d[0] + d[1] * d[1]))
                    .clamp(0., 1.);
                (p[0] - a[0] - t * d[0]).powi(2) + (p[1] - a[1] - t * d[1]).powi(2)
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// Unlike interpolation, boundary extension deliberately permits negative weights.
/// Keep that permission local to manual mesh transfer, including finite checks.
fn extrapolate_channel(values: &[Vec2], weights: &[(usize, f64)]) -> Result<Vec2, Status> {
    let (mut x, mut y) = (0., 0.);
    for &(slot, w) in weights {
        x += f64::from(values[slot].x) * w;
        y += f64::from(values[slot].y) * w;
    }
    let p = Vec2::new(x as f32, y as f32);
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err(error(
            "NON_FINITE",
            "Extended channel exceeds finite float32 coordinates",
            vec![],
        ));
    }
    Ok(p)
}
impl RTreeObject for UvTriangle {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        // A persisted edge split is rounded to f32. Admit only rounding-sized
        // misses; barycentric tests below reject genuine outside samples.
        let eps = f64::from(f32::EPSILON) * 2.;
        AABB::from_corners(
            [
                self.uv.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min) - eps,
                self.uv.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) - eps,
            ],
            [
                self.uv
                    .iter()
                    .map(|p| p[0])
                    .fold(f64::NEG_INFINITY, f64::max)
                    + eps,
                self.uv
                    .iter()
                    .map(|p| p[1])
                    .fold(f64::NEG_INFINITY, f64::max)
                    + eps,
            ],
        )
    }
}
#[derive(Clone, Debug)]
pub(crate) struct UvSampler {
    tree: RTree<UvTriangle>,
}
impl UvSampler {
    pub(crate) fn new(g: &MeshGeometry) -> Self {
        let slots: HashMap<_, _> = g
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, i))
            .collect();
        Self {
            tree: RTree::bulk_load(
                g.triangles
                    .iter()
                    .map(|t| {
                        let mut ids = *t;
                        ids.sort_unstable();
                        let slots = ids.map(|id| slots[&id]);
                        UvTriangle {
                            ids,
                            slots,
                            uv: slots.map(|i| [f64::from(g.uvs[i].x), f64::from(g.uvs[i].y)]),
                        }
                    })
                    .collect(),
            ),
        }
    }
    fn weights(&self, uv: Vec2) -> Option<([VertexId; 3], Vec<(usize, f64)>)> {
        let p = [f64::from(uv.x), f64::from(uv.y)];
        let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        };
        self.tree
            .locate_in_envelope_intersecting(&AABB::from_point(p))
            .filter_map(|t| {
                let [a, b, c] = t.uv;
                let det = cross(a, b, c);
                if det == 0. {
                    return None;
                }
                let mut w = [
                    cross(p, b, c) / det,
                    cross(a, p, c) / det,
                    cross(a, b, p) / det,
                ];
                // Bound geometric distance, not just weights (skinny triangles).
                for i in 0..3 {
                    if w[i] < 0. {
                        let x = t.uv[(i + 1) % 3];
                        let y = t.uv[(i + 2) % 3];
                        if cross(x, y, p).abs() / (y[0] - x[0]).hypot(y[1] - x[1])
                            > f64::from(f32::EPSILON) * 2.
                        {
                            return None;
                        }
                        w[i] = 0.;
                    }
                }
                let sum: f64 = w.iter().sum();
                if !sum.is_finite() || sum <= 0. {
                    return None;
                }
                Some((t.ids, t.slots.into_iter().zip(w.map(|v| v / sum)).collect()))
            })
            .min_by_key(|v| v.0)
    }
    fn extrapolation_weights(&self, uv: Vec2) -> Option<([VertexId; 3], Vec<(usize, f64)>)> {
        let p = [f64::from(uv.x), f64::from(uv.y)];
        if p.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let nearest = self.tree.nearest_neighbor(&p)?;
        let distance = nearest.distance_2(&p);
        // Stable IDs settle exact ties at corners, independently of tree order.
        let triangle = self
            .tree
            .nearest_neighbor_iter(&p)
            .take_while(|t| t.distance_2(&p) <= distance)
            .min_by_key(|t| t.ids)?;
        let w = barycentric(p, triangle.uv)?;
        Some((triangle.ids, triangle.slots.into_iter().zip(w).collect()))
    }
    pub(crate) fn sample_position(&self, g: &MeshGeometry, uv: Vec2) -> Result<Vec2, Status> {
        if !uv.x.is_finite() || !uv.y.is_finite() {
            return Err(error("INVALID_POSITION", "Invalid UV position", vec![]));
        }
        if let Some((_, w)) = self.weights(uv) {
            return zipzi_core::geometry::sample_vertex_channel(&g.positions, &w).map_err(status);
        }
        let (_, w) = self.extrapolation_weights(uv).ok_or_else(|| {
            error(
                "OUTSIDE_TRANSFER_DOMAIN",
                "UV lies outside the frozen source triangulation",
                vec![],
            )
        })?;
        extrapolate_channel(&g.positions, &w)
    }
}

/// Frozen piecewise map for manual gestures. Outside the outline, continue the
/// nearest UV triangle's affine map. Publication still enforces image bounds.
pub struct ManualMeshUvMap {
    geometry: MeshGeometry,
    sampler: UvSampler,
}
impl ManualMeshUvMap {
    pub fn new(geometry: MeshGeometry) -> Result<Self, Status> {
        extract_manual_meshing(&geometry)?;
        Ok(Self {
            sampler: UvSampler::new(&geometry),
            geometry,
        })
    }
    pub fn position(&self, uv: Vec2) -> Result<Vec2, Status> {
        self.sampler.sample_position(&self.geometry, uv)
    }
    /// Reject overlapping inverse branches, including extrapolated branches.
    pub fn uv(&self, point: Vec2) -> Result<Vec2, Status> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(error("INVALID_POSITION", "Invalid model position", vec![]));
        }
        let ambiguous = || {
            error(
                "AMBIGUOUS_MESH_SURFACE",
                "This pose overlaps multiple UV regions; use Edit on source image",
                vec![],
            )
        };
        let mut result: Option<Vec2> = None;
        // Prefer the actual surface. Only consult exterior branches when no
        // original triangle covers the requested position.
        for exterior in [false, true] {
            for t in self.sampler.tree.iter() {
                let positions = t.slots.map(|i| self.geometry.positions[i]);
                if !exterior {
                    let eps = f32::EPSILON * point.x.abs().max(point.y.abs()).max(1.) * 4.;
                    if [point.x, point.y].into_iter().enumerate().any(|(axis, p)| {
                        let values = positions.map(|v| if axis == 0 { v.x } else { v.y });
                        p < values.into_iter().fold(f32::INFINITY, f32::min) - eps
                            || p > values.into_iter().fold(f32::NEG_INFINITY, f32::max) + eps
                    }) {
                        continue;
                    }
                }
                let Some(w) = barycentric(
                    [point.x.into(), point.y.into()],
                    positions.map(|p| [p.x.into(), p.y.into()]),
                ) else {
                    continue;
                };
                if !exterior && w.iter().any(|w| *w < -1e-6 || *w > 1. + 1e-6) {
                    continue;
                }
                let w = if exterior {
                    w
                } else {
                    let w = w.map(|w| w.max(0.));
                    let sum: f64 = w.iter().sum();
                    w.map(|w| w / sum)
                };
                let Ok(uv) = extrapolate_channel(
                    &self.geometry.uvs,
                    &t.slots.into_iter().zip(w).collect::<Vec<_>>(),
                ) else {
                    continue;
                };
                if exterior {
                    // An inverse extension is valid only for the same triangle
                    // selected by the forward UV map used to transfer all shapes.
                    if self.sampler.weights(uv).is_some()
                        || !self
                            .sampler
                            .extrapolation_weights(uv)
                            .is_some_and(|(ids, _)| ids == t.ids)
                    {
                        continue;
                    }
                }
                if let Some(previous) = result {
                    if (previous.x - uv.x).hypot(previous.y - uv.y) >= 1e-5 {
                        return Err(ambiguous());
                    }
                } else {
                    result = Some(uv);
                }
            }
            if let Some(uv) = result {
                return Ok(uv);
            }
        }
        Err(error(
            "NON_INVERTIBLE_MESH_SURFACE",
            "This position is outside the invertible mesh surface; use Edit on source image",
            vec![],
        ))
    }
}

/// Persistent identity or one source-triangle sample, shared by every channel.
#[derive(Clone, Debug, PartialEq)]
pub enum VertexProvenance {
    Retained(VertexId),
    Sampled {
        triangle: [VertexId; 3],
        weights: Vec<(usize, f64)>,
    },
    AffineExtension,
    Extrapolated {
        triangle: [VertexId; 3],
        weights: Vec<(usize, f64)>,
    },
}

#[derive(Clone, Debug)]
pub struct ManualMeshTransferReport {
    pub added: usize,
    pub moved: usize,
    pub removed: usize,
    pub shape_count: usize,
    pub reconnected: bool,
    pub glue_endpoints: bool,
    pub non_affine_parent: bool,
    pub affine_extension: bool,
    pub appearance_limits: Vec<String>,
    pub cost: ConversionWork,
}

/// Complete frozen authoring source, including inactive and incomplete dependencies.
#[derive(Clone, Debug)]
pub struct ManualMeshInspection {
    pub geometry: MeshGeometry,
    pub referenced_vertices: Vec<VertexId>,
}
impl ManualMeshInspection {
    pub fn capture(
        document: &DocumentState,
        mesh_id: &str,
        limits: ConversionLimits,
    ) -> Result<Self, Status> {
        let source = ManualMeshSource::new(document, mesh_id, limits)?;
        Ok(Self {
            geometry: source.geometry.clone(),
            referenced_vertices: source.referenced_vertices,
        })
    }
    pub fn can_rebuild(&self) -> bool {
        self.referenced_vertices.is_empty()
    }
    pub fn check_rebuild(&self) -> Result<(), Status> {
        if self.can_rebuild() {
            Ok(())
        } else {
            Err(Status::error(
                "GLUE_VERTEX_REFERENCED",
                "Full rebuild would remove Glue endpoints",
            ))
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ManualMeshSource<'a> {
    document: &'a DocumentState,
    mesh_id: String,
    geometry: MeshGeometry,
    slots: HashMap<VertexId, usize>,
    sampler: UvSampler,
    meshing: zipzi_core::MeshMeshing,
    workspace_bytes: usize,
    dependency_bytes: usize,
    affine_extension: Option<AffineUvMap>,
    pub referenced_vertices: Vec<VertexId>,
}
impl<'a> ManualMeshSource<'a> {
    pub fn new(
        document: &'a DocumentState,
        mesh_id: &str,
        limits: ConversionLimits,
    ) -> Result<Self, Status> {
        let mesh = document.mesh(mesh_id).ok_or_else(|| {
            error(
                "MISSING_MESH",
                "Mesh no longer exists",
                vec![mesh_id.into()],
            )
        })?;
        let workspace = mesh
            .vertex_ids
            .len()
            .saturating_mul(128)
            .saturating_add(mesh.triangles.len().saturating_mul(160))
            .saturating_add(mesh.meshing.as_ref().map_or(0, |m| {
                m.boundary.len().saturating_mul(4).saturating_add(
                    m.constraints
                        .len()
                        .saturating_mul(std::mem::size_of::<zipzi_core::MeshConstraint>()),
                )
            }));
        limits.check_conversion(
            1,
            workspace.saturating_add(document.mesh_dependency_bytes(mesh_id)),
        )?;
        let geometry = MeshGeometry {
            vertex_ids: mesh.vertex_ids.clone(),
            positions: mesh.base_positions.clone(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            meshing: mesh.meshing.clone(),
        };
        let meshing = extract_manual_meshing(&geometry)?;
        image_domain(&geometry, mesh_id)?;
        let mut referenced_vertices: Vec<_> = document
            .glue_order()
            .iter()
            .filter_map(|id| document.glue(id))
            .flat_map(|g| {
                g.pairs.iter().flat_map(|p| {
                    [
                        (g.mesh_a_id == mesh_id).then_some(p.vertex_a).flatten(),
                        (g.mesh_b_id == mesh_id).then_some(p.vertex_b).flatten(),
                    ]
                    .into_iter()
                    .flatten()
                })
            })
            .collect();
        referenced_vertices.sort_unstable();
        referenced_vertices.dedup();
        let dependent = !mesh.deformer_id.is_empty()
            || document.binding_for_mesh(mesh_id).is_some()
            || document
                .blend_binding_order()
                .iter()
                .filter_map(|id| document.blend_binding(id))
                .any(|b| b.target_id == mesh_id)
            || document
                .glue_order()
                .iter()
                .filter_map(|id| document.glue(id))
                .any(|g| g.mesh_a_id == mesh_id || g.mesh_b_id == mesh_id);
        let workspace_bytes = geometry
            .vertex_ids
            .len()
            .saturating_mul(128)
            .saturating_add(geometry.triangles.len() * 160)
            .saturating_add(meshing.boundary.len() * std::mem::size_of::<VertexId>())
            .saturating_add(
                meshing.constraints.len() * std::mem::size_of::<zipzi_core::MeshConstraint>(),
            );
        Ok(Self {
            workspace_bytes,
            dependency_bytes: document.mesh_dependency_bytes(mesh_id),
            meshing,
            document,
            mesh_id: mesh_id.into(),
            sampler: UvSampler::new(&geometry),
            slots: geometry
                .vertex_ids
                .iter()
                .enumerate()
                .map(|(i, &id)| (id, i))
                .collect(),
            affine_extension: (!dependent)
                .then(|| AffineUvMap::from_geometry(&geometry).ok())
                .flatten(),
            geometry,
            referenced_vertices,
        })
    }
    pub fn geometry(&self) -> &MeshGeometry {
        &self.geometry
    }
    fn cost(
        &self,
        combinations: usize,
        payload: usize,
        limits: ConversionLimits,
    ) -> Result<ConversionWork, Status> {
        limits.check_conversion(
            combinations,
            payload
                .saturating_add(self.workspace_bytes)
                .saturating_add(self.dependency_bytes),
        )
    }

    /// Build one complete replacement without mutating the current candidate.
    /// A full rebuild must allocate fresh IDs before calling this method.
    pub fn plan_geometry(
        &self,
        geometry: MeshGeometry,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<MeshTransfer, Status> {
        image_domain(&geometry, &self.mesh_id)?;
        self.build_geometry(geometry, limits, control)
    }
    /// Explicit texture-padding path: sample in the previous image coordinates,
    /// then validate the final normalized UVs before returning a complete plan.
    pub fn plan_geometry_with_texture_uvs(
        &self,
        geometry: MeshGeometry,
        texture_uvs: Vec<Vec2>,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<MeshTransfer, Status> {
        let mut final_geometry = geometry.clone();
        final_geometry.uvs = texture_uvs;
        extract_manual_meshing(&final_geometry)?;
        image_domain(&final_geometry, &self.mesh_id)?;
        let mut plan = self.build_geometry(geometry, limits, control)?;
        plan.mesh.uvs = final_geometry.uvs;
        Ok(plan)
    }
    fn build_geometry(
        &self,
        geometry: MeshGeometry,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<MeshTransfer, Status> {
        control.check().map_err(status)?;
        extract_manual_meshing(&geometry)?;
        self.build_replacement(geometry, limits, control, false)
    }
    fn build_replacement(
        &self,
        geometry: MeshGeometry,
        limits: ConversionLimits,
        control: &ConversionControl,
        extend_outline: bool,
    ) -> Result<MeshTransfer, Status> {
        control.check().map_err(status)?;
        let old = &self.geometry;
        let doc = &self.document;
        let id = &self.mesh_id;
        let kept: HashSet<_> = geometry.vertex_ids.iter().copied().collect();
        for g in doc.glue_order().iter().filter_map(|id| doc.glue(id)) {
            for pair in &g.pairs {
                for endpoint in [
                    (g.mesh_a_id == *id).then_some(pair.vertex_a).flatten(),
                    (g.mesh_b_id == *id).then_some(pair.vertex_b).flatten(),
                ]
                .into_iter()
                .flatten()
                {
                    if !kept.contains(&endpoint) {
                        let mut e = error(
                            "GLUE_VERTEX_REFERENCED",
                            format!(
                                "Vertex {endpoint} is connected to Glue {}; release pair {} before deleting it",
                                g.id, pair.id
                            ),
                            vec![id.clone(), g.id.clone(), pair.id.clone()],
                        );
                        e.message.push_str(&format!("vertex_ids[{endpoint}]"));
                        return Err(e);
                    }
                }
            }
        }
        let ordinary = doc.binding_for_mesh(id);
        let blends: Vec<_> = doc
            .blend_binding_order()
            .iter()
            .filter_map(|b| doc.blend_binding(b))
            .filter(|b| b.target_id == *id)
            .collect();
        let shape_count = 1
            + ordinary.map_or(0, |b| b.keyforms.len())
            + blends
                .iter()
                .map(|b| match &b.keyforms {
                    DeltaKeyforms::Mesh(forms) => forms.len(),
                    _ => 0,
                })
                .sum::<usize>();
        let payload = geometry
            .vertex_ids
            .len()
            .saturating_mul(shape_count.saturating_mul(8).saturating_add(128))
            .saturating_add(geometry.triangles.len().saturating_mul(160));
        // Admit channel arrays and the source spatial index before allocation.
        let cost = self.cost(shape_count, payload, limits)?;
        let mut provenance = Vec::with_capacity(geometry.vertex_ids.len());
        let (mut added, mut moved, mut extension) = (0, 0, false);
        for (i, (&vertex, &uv)) in geometry.vertex_ids.iter().zip(&geometry.uvs).enumerate() {
            if i % 256 == 0 {
                control.check().map_err(status)?;
            }
            let slot = self.slots.get(&vertex).copied();
            if slot.is_some_and(|s| old.uvs[s] == uv) {
                provenance.push(VertexProvenance::Retained(vertex));
                continue;
            }
            if slot.is_some() {
                moved += 1;
            } else {
                added += 1;
            }
            if let Some((triangle, weights)) = self.sampler.weights(uv) {
                provenance.push(VertexProvenance::Sampled { triangle, weights });
            } else if self.affine_extension.is_some() {
                extension = true;
                provenance.push(VertexProvenance::AffineExtension);
            } else if let Some((triangle, weights)) = extend_outline
                .then(|| self.sampler.extrapolation_weights(uv))
                .flatten()
            {
                provenance.push(VertexProvenance::Extrapolated { triangle, weights });
            } else {
                let mut e = error(
                    "OUTSIDE_TRANSFER_DOMAIN",
                    format!(
                        "Vertex {vertex}: UV ({}, {}) lies outside the frozen source triangulation",
                        uv.x, uv.y
                    ),
                    vec![id.clone()],
                );
                e.message.push_str(&format!("vertex_ids[{vertex}]"));
                return Err(e);
            }
        }
        let same_slots = geometry.vertex_ids == old.vertex_ids;
        let changed_slots: Vec<_> = provenance
            .iter()
            .enumerate()
            .filter(|(_, p)| !matches!(p, VertexProvenance::Retained(_)))
            .map(|(i, _)| i)
            .collect();
        let remap = |values: &mut Vec<Vec2>| -> Result<(), Status> {
            control.check().map_err(status)?;
            let sample = |i: usize| -> Result<Vec2, Status> {
                if i % 256 == 0 {
                    control.check().map_err(status)?;
                }
                match &provenance[i] {
                    VertexProvenance::Retained(id) => Ok(values[self.slots[id]]),
                    VertexProvenance::Sampled { weights, .. } => {
                        zipzi_core::geometry::sample_vertex_channel(values, weights).map_err(status)
                    }
                    VertexProvenance::Extrapolated { weights, .. } => {
                        extrapolate_channel(values, weights)
                    }
                    VertexProvenance::AffineExtension => Ok(self
                        .affine_extension
                        .as_ref()
                        .unwrap()
                        .position(geometry.uvs[i])),
                }
            };
            if same_slots {
                // The owned channel was already cloned once with its binding.
                // Compute all samples before writing, then patch only changed slots.
                let updates: Result<Vec<_>, Status> =
                    changed_slots.iter().map(|&i| Ok((i, sample(i)?))).collect();
                for (i, value) in updates? {
                    values[i] = value;
                }
            } else {
                let result: Result<Vec<_>, _> = (0..provenance.len()).map(sample).collect();
                *values = result?;
            }
            Ok(())
        };
        let mut mesh: Mesh = doc.mesh(id).unwrap().clone();
        remap(&mut mesh.base_positions)?;
        mesh.vertex_ids = geometry.vertex_ids.clone();
        mesh.uvs = geometry.uvs.clone();
        mesh.triangles = geometry.triangles.clone();
        mesh.meshing = geometry.meshing.clone();
        let mut binding = ordinary.cloned();
        if let Some(b) = &mut binding {
            for f in &mut b.keyforms {
                remap(&mut f.positions)?;
            }
        }
        let mut blend_bindings: Vec<_> = blends.into_iter().cloned().collect();
        for b in &mut blend_bindings {
            if let DeltaKeyforms::Mesh(forms) = &mut b.keyforms {
                for f in forms {
                    remap(&mut f.positions)?;
                }
            }
        }
        let glues: Vec<_> = doc
            .glue_order()
            .iter()
            .filter_map(|g| doc.glue(g))
            .filter(|g| g.mesh_a_id == *id || g.mesh_b_id == *id)
            .cloned()
            .collect();
        let mapping: HashMap<_, _> = old
            .vertex_ids
            .iter()
            .map(|&v| (v, kept.contains(&v).then_some(v)))
            .collect();
        let removed = old.vertex_ids.len() - mapping.values().flatten().count();
        let glue_endpoints = geometry.vertex_ids.iter().zip(&provenance).any(|(id, p)| {
            self.referenced_vertices.contains(id) && !matches!(p, VertexProvenance::Retained(_))
        });
        let mut parent = mesh.deformer_id.as_str();
        let mut seen = HashSet::new();
        let mut non_affine_parent = false;
        while !parent.is_empty() && seen.insert(parent) {
            let Some(t) = doc.transform(parent) else {
                break;
            };
            non_affine_parent |= matches!(t.data, zipzi_core::TransformData::Warp(_));
            parent = t.parent();
        }
        let reconnected = old.triangles != mesh.triangles;
        let mut appearance_limits = vec![];
        if provenance
            .iter()
            .any(|p| matches!(p, VertexProvenance::Extrapolated { .. }))
        {
            appearance_limits.push(
                "Shapes outside the previous outline use boundary triangle extrapolation".into(),
            );
        }
        if reconnected || moved > 0 || removed > 0 {
            appearance_limits
                .push("Edited piecewise interpolation may change the rendered surface".into());
        }
        if non_affine_parent {
            appearance_limits.push("A Warp in the parent chain is nonlinear".into());
        }
        if !glues.is_empty() {
            appearance_limits.push("Glue is evaluated after local shape transfer".into());
        }
        Ok(MeshTransfer {
            mesh,
            binding,
            blend_bindings,
            glues,
            vertex_mapping: mapping,
            provenance,
            report: ManualMeshTransferReport {
                added,
                moved,
                removed,
                shape_count,
                reconnected,
                glue_endpoints,
                non_affine_parent,
                affine_extension: extension,
                appearance_limits,
                cost,
            },
            affected_edges: vec![],
        })
    }
    pub fn plan(
        &self,
        operation: ManualMeshOperation,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<MeshTransfer, Status> {
        self.plan_candidate(operation, limits, control)
    }
    fn plan_candidate(
        &self,
        operation: ManualMeshOperation,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<MeshTransfer, Status> {
        // Bound topology workspace before cloning or triangulating input.
        self.cost(
            1,
            self.geometry
                .vertex_ids
                .len()
                .saturating_add(1)
                .saturating_mul(256),
            limits,
        )?;
        let result = edit_manual_uv_candidate(&self.geometry, self.meshing.clone(), operation)?;
        let mut plan = self
            .build_replacement(result.geometry, limits, control, true)
            .map_err(|e| {
                if e.code == "INVALID_MESH_MESHING" {
                    error("INVALID_MANUAL_MESH", e.message, vec![self.mesh_id.clone()])
                } else {
                    e
                }
            })?;
        plan.affected_edges = result.affected_edges;
        Ok(plan)
    }
}

pub(crate) struct MeshTransfer {
    pub mesh: Mesh,
    pub binding: Option<zipzi_core::MeshBinding>,
    pub blend_bindings: Vec<zipzi_core::BlendShapeBinding>,
    pub glues: Vec<zipzi_core::Glue>,
    pub vertex_mapping: HashMap<VertexId, Option<VertexId>>,
    pub provenance: Vec<VertexProvenance>,
    pub report: ManualMeshTransferReport,
    pub affected_edges: Vec<[VertexId; 2]>,
}

#[derive(Debug, Clone)]
pub struct ManualMeshOutcome {
    pub report: ManualMeshTransferReport,
    pub provenance: Vec<VertexProvenance>,
    pub affected_edges: Vec<[VertexId; 2]>,
}
