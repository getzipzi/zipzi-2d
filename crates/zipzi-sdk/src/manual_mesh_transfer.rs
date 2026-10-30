//! Source-bound UV sampling and complete, atomic dependency transfer.
use crate::{manual_mesh::edit_manual_uv_candidate, *};
use rstar::{RTree, RTreeObject, AABB};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use zipzi_core::{
    document::{AuthoringLimits, ConversionControl},
    DeltaKeyforms, Document, Mesh, Vec2, VertexId,
};

fn error(code: &str, message: impl Into<String>, ids: Vec<String>) -> SdkError {
    let mut e = SdkError::new(code, &message.into(), "plan_manual_mesh");
    e.object_ids = ids;
    e
}
fn image_domain(g: &MeshGeometry, mesh_id: &str) -> Result<(), SdkError> {
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
        e.field_path = Some(format!("uvs[{slot}]").into());
        return Err(e);
    }
    Ok(())
}
fn status(s: zipzi_core::Status) -> SdkError {
    SdkError::from_status(s, "plan_manual_mesh", vec![])
}

#[derive(Clone, Debug)]
struct UvTriangle {
    ids: [VertexId; 3],
    slots: [usize; 3],
    uv: [[f64; 2]; 3],
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
    pub(crate) fn sample_position(&self, g: &MeshGeometry, uv: Vec2) -> Result<Vec2, SdkError> {
        if let Some((_, w)) = self.weights(uv) {
            return zipzi_core::geometry::sample_vertex_channel(&g.positions, &w).map_err(status);
        }
        AffineUvMap::from_geometry(g)
            .map(|m| m.position(uv))
            .map_err(|_| {
                error(
                    "OUTSIDE_TRANSFER_DOMAIN",
                    "UV lies outside the frozen source triangulation",
                    vec![],
                )
            })
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
}

pub use zipzi_contract::authoring::ManualMeshTransferReport;

/// Complete frozen authoring source, including inactive and incomplete dependencies.
#[derive(Clone, Debug)]
pub struct ManualMeshSource {
    document: Document,
    mesh_id: String,
    geometry: MeshGeometry,
    slots: HashMap<VertexId, usize>,
    sampler: UvSampler,
    meshing: zipzi_core::MeshMeshing,
    estimated_bytes: usize,
    affine_extension: Option<AffineUvMap>,
    pub geometry_space: GeometrySpace,
    pub referenced_vertices: Vec<VertexId>,
}
impl ManualMeshSource {
    pub fn new(document: &Document, mesh_id: &str) -> Result<Self, SdkError> {
        let mesh = document.get_mesh(mesh_id).ok_or_else(|| {
            error(
                "MISSING_MESH",
                "Mesh no longer exists",
                vec![mesh_id.into()],
            )
        })?;
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
            .filter_map(|id| document.get_glue(id))
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
                .filter_map(|id| document.get_blend_binding(id))
                .any(|b| b.target_id == mesh_id)
            || document
                .glue_order()
                .iter()
                .filter_map(|id| document.get_glue(id))
                .any(|g| g.mesh_a_id == mesh_id || g.mesh_b_id == mesh_id);
        let geometry_space = if mesh.deformer_id.is_empty() {
            GeometrySpace::Model
        } else if document
            .get_transform(&mesh.deformer_id)
            .is_some_and(|t| matches!(t.data, zipzi_core::TransformData::Warp(_)))
        {
            GeometrySpace::WarpParameter(mesh.deformer_id.clone())
        } else {
            GeometrySpace::RotationLocal(mesh.deformer_id.clone())
        };
        Ok(Self {
            estimated_bytes: document
                .estimated_content_bytes()
                .saturating_add(geometry.vertex_ids.len() * 128)
                .saturating_add(geometry.triangles.len() * 160)
                .saturating_add(meshing.boundary.len() * std::mem::size_of::<VertexId>())
                .saturating_add(
                    meshing.constraints.len() * std::mem::size_of::<zipzi_core::MeshConstraint>(),
                ),
            meshing,
            document: document.clone(),
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
            geometry_space,
            referenced_vertices,
        })
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn geometry(&self) -> &MeshGeometry {
        &self.geometry
    }
    pub fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }
    pub fn can_rebuild(&self) -> bool {
        self.referenced_vertices.is_empty()
    }
    pub fn check_rebuild(&self) -> Result<(), SdkError> {
        if self.can_rebuild() {
            return Ok(());
        }
        let id = self.referenced_vertices[0];
        let glue = self
            .document
            .glue_order()
            .iter()
            .filter_map(|g| self.document.get_glue(g))
            .find(|g| {
                g.pairs.iter().any(|p| {
                    (g.mesh_a_id == self.mesh_id && p.vertex_a == Some(id))
                        || (g.mesh_b_id == self.mesh_id && p.vertex_b == Some(id))
                })
            })
            .unwrap();
        Err(error(
            "GLUE_VERTEX_REFERENCED",
            format!(
                "Vertex {id} is connected to Glue {}; use interior fill or release its pairs before rebuilding",
                glue.id
            ),
            vec![self.mesh_id.clone(), glue.id.clone(), id.to_string()],
        ))
    }

    /// Build one complete replacement without mutating the current candidate.
    /// A full rebuild must allocate fresh IDs before calling this method.
    pub fn plan_geometry(
        &self,
        current: &Document,
        version: Version,
        geometry: MeshGeometry,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshPlan, SdkError> {
        image_domain(&geometry, &self.mesh_id)?;
        if !current.is_same_snapshot(&self.document) {
            return Err(error(
                "STALE_MANUAL_MESH_PLAN",
                "Frozen dependencies changed",
                vec![self.mesh_id.clone()],
            ));
        }
        self.build_geometry(current, version, geometry, limits, control)
    }
    /// Explicit texture-padding path: sample in the previous image coordinates,
    /// then validate the final normalized UVs before returning a complete plan.
    pub fn plan_geometry_with_texture_uvs(
        &self,
        current: &Document,
        version: Version,
        geometry: MeshGeometry,
        texture_uvs: Vec<Vec2>,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshPlan, SdkError> {
        if !current.is_same_snapshot(&self.document) {
            return Err(error(
                "STALE_MANUAL_MESH_PLAN",
                "Frozen dependencies changed",
                vec![self.mesh_id.clone()],
            ));
        }
        let mut final_geometry = geometry.clone();
        final_geometry.uvs = texture_uvs;
        extract_manual_meshing(&final_geometry)?;
        image_domain(&final_geometry, &self.mesh_id)?;
        let mut plan = self.build_geometry(current, version, geometry, limits, control)?;
        let id = &self.mesh_id;
        let mut mesh = plan.validated.get_mesh(id).unwrap().clone();
        mesh.uvs = final_geometry.uvs;
        let kept: HashSet<_> = mesh.vertex_ids.iter().copied().collect();
        let mapping = self
            .geometry
            .vertex_ids
            .iter()
            .map(|&v| (v, kept.contains(&v).then_some(v)))
            .collect();
        let binding = plan.validated.binding_for_mesh(id).cloned();
        let blends = plan
            .validated
            .blend_bindings_for_target(id)
            .into_iter()
            .cloned()
            .collect();
        let glues = plan
            .validated
            .glues_for_mesh(id)
            .into_iter()
            .cloned()
            .collect();
        let mut candidate = current.clone();
        let result = candidate.replace_mesh_topology(mesh, binding, blends, glues, mapping);
        if !result.status.is_ok() {
            return Err(status(result.status));
        }
        plan.validated = candidate;
        plan.changes = result;
        Ok(plan)
    }
    fn build_geometry(
        &self,
        current: &Document,
        version: Version,
        geometry: MeshGeometry,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshPlan, SdkError> {
        control.check().map_err(status)?;
        extract_manual_meshing(&geometry)?;
        self.build_replacement(current, version, geometry, limits, control)
    }
    fn build_replacement(
        &self,
        current: &Document,
        version: Version,
        geometry: MeshGeometry,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshPlan, SdkError> {
        control.check().map_err(status)?;
        let old = &self.geometry;
        let doc = &self.document;
        let id = &self.mesh_id;
        let kept: HashSet<_> = geometry.vertex_ids.iter().copied().collect();
        for g in doc.glue_order().iter().filter_map(|id| doc.get_glue(id)) {
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
                        e.field_path = Some(format!("vertex_ids[{endpoint}]").into());
                        return Err(e);
                    }
                }
            }
        }
        let ordinary = doc.binding_for_mesh(id);
        let blends: Vec<_> = doc
            .blend_binding_order()
            .iter()
            .filter_map(|b| doc.get_blend_binding(b))
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
        // Include the old spatial index, candidate copy and retained history before
        // allocating any channel arrays. Limits remain caller-reducible.
        let cost = limits
            .check_conversion(shape_count, payload, self.estimated_bytes())
            .map_err(status)?;
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
            } else {
                let mut e = error(
                    "OUTSIDE_TRANSFER_DOMAIN",
                    format!(
                        "Vertex {vertex}: UV ({}, {}) lies outside the frozen source triangulation",
                        uv.x, uv.y
                    ),
                    vec![id.clone()],
                );
                e.field_path = Some(format!("vertex_ids[{vertex}]").into());
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
        let remap = |values: &mut Vec<Vec2>| -> Result<(), SdkError> {
            control.check().map_err(status)?;
            let sample = |i: usize| -> Result<Vec2, SdkError> {
                if i % 256 == 0 {
                    control.check().map_err(status)?;
                }
                match &provenance[i] {
                    VertexProvenance::Retained(id) => Ok(values[self.slots[id]]),
                    VertexProvenance::Sampled { weights, .. } => {
                        zipzi_core::geometry::sample_vertex_channel(values, weights).map_err(status)
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
                let updates: Result<Vec<_>, _> =
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
        let mut mesh: Mesh = doc.get_mesh(id).unwrap().clone();
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
            .filter_map(|g| doc.get_glue(g))
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
            let Some(t) = doc.get_transform(parent) else {
                break;
            };
            non_affine_parent |= matches!(t.data, zipzi_core::TransformData::Warp(_));
            parent = t.parent();
        }
        let reconnected = old.triangles != mesh.triangles;
        let mut appearance_limits = vec![];
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
        let mesh_id = mesh.id.clone();
        let mut candidate = current.clone();
        let check = candidate.replace_mesh_topology(mesh, binding, blend_bindings, glues, mapping);
        if !check.status.is_ok() {
            return Err(status(check.status));
        }
        Ok(ManualMeshPlan {
            version,
            publication_source: current.clone(),
            mesh_id,
            validated: candidate,
            changes: check,
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
            baseline: None,
            affected_edges: vec![],
            moving_ids: vec![],
        })
    }
    pub fn plan(
        &self,
        current: &Document,
        version: Version,
        operation: ManualMeshOperation,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshPlan, SdkError> {
        if !current.is_same_snapshot(&self.document) {
            return Err(error(
                "STALE_MANUAL_MESH_PLAN",
                "Frozen dependencies changed",
                vec![self.mesh_id.clone()],
            ));
        }
        self.plan_candidate(current, version, operation, limits, control)
    }
    fn plan_candidate(
        &self,
        current: &Document,
        version: Version,
        operation: ManualMeshOperation,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshPlan, SdkError> {
        // Bound topology workspace before cloning or triangulating input.
        limits
            .check_conversion(
                1,
                self.geometry
                    .vertex_ids
                    .len()
                    .saturating_add(1)
                    .saturating_mul(256),
                self.estimated_bytes(),
            )
            .map_err(status)?;
        let result = edit_manual_uv_candidate(&self.geometry, self.meshing.clone(), operation)
            .map_err(|mut e| {
                if e.object_ids.is_empty() {
                    e.object_ids.push(self.mesh_id.clone());
                }
                e
            })?;
        let mut plan = self
            .build_replacement(current, version, result.geometry, limits, control)
            .map_err(|e| {
                if e.code.as_ref() == "INVALID_MESH_MESHING" {
                    let mut error =
                        SdkError::new("INVALID_MANUAL_MESH", &e.message, "edit_manual_mesh");
                    error.object_ids.push(self.mesh_id.clone());
                    error
                } else {
                    e
                }
            })?;
        plan.affected_edges = result.affected_edges;
        Ok(plan)
    }
}

#[derive(Clone, Debug)]
pub struct ManualMeshPlan {
    version: Version,
    publication_source: Document,
    mesh_id: String,
    validated: Document,
    changes: zipzi_core::EditResult,
    pub provenance: Vec<VertexProvenance>,
    pub report: ManualMeshTransferReport,
    baseline: Option<Arc<ManualMeshSource>>,
    affected_edges: Vec<[VertexId; 2]>,
    moving_ids: Vec<VertexId>,
}
impl ManualMeshPlan {
    pub fn geometry(&self) -> MeshGeometry {
        let m = self.validated.get_mesh(&self.mesh_id).unwrap();
        MeshGeometry {
            vertex_ids: m.vertex_ids.clone(),
            positions: m.base_positions.clone(),
            uvs: m.uvs.clone(),
            triangles: m.triangles.clone(),
            meshing: m.meshing.clone(),
        }
    }
}

pub(crate) struct ManualMeshBaseline {
    pub source: Arc<ManualMeshSource>,
    pub publication: Document,
    pub moving_ids: Vec<VertexId>,
}
impl EditSession<'_> {
    /// Plans can fail without aborting an interactive candidate. Runtime performs
    /// this read-only step before entering its ordinary abort-on-error write path.
    pub fn plan_manual_mesh(
        &self,
        mesh_id: &str,
        operation: ManualMeshOperation,
    ) -> Result<ManualMeshPlan, SdkError> {
        self.plan_manual_mesh_with_limits(mesh_id, operation, self.authoring_limits)
    }
    pub fn plan_manual_mesh_with_limits(
        &self,
        mesh_id: &str,
        operation: ManualMeshOperation,
        limits: AuthoringLimits,
    ) -> Result<ManualMeshPlan, SdkError> {
        self.ensure_active("plan_manual_mesh")?;
        limits
            .check_conversion(1, 0, self.candidate_document().estimated_content_bytes())
            .map_err(status)?;
        let mut operation = operation;
        if let ManualMeshOperation::MoveVertices {
            targets,
            motion_from,
        } = &mut operation
        {
            let mesh = self.candidate_document().get_mesh(mesh_id).ok_or_else(|| {
                error(
                    "MISSING_MESH",
                    "Mesh no longer exists",
                    vec![mesh_id.into()],
                )
            })?;
            let actual: Vec<_> = targets
                .iter()
                .filter_map(|t| {
                    mesh.vertex_ids
                        .iter()
                        .position(|id| *id == t.id)
                        .map(|slot| ManualVertexTarget {
                            id: t.id,
                            uv: mesh.uvs[slot],
                        })
                })
                .collect();
            if let Some(from) = motion_from {
                if from.len() != actual.len() || from.iter().any(|t| !actual.contains(t)) {
                    return Err(error(
                        "STALE_MANUAL_MOTION",
                        "Motion must start at the latest accepted candidate",
                        vec![mesh_id.into()],
                    ));
                }
            } else {
                *motion_from = Some(actual);
            }
        }
        let mut moving_ids = match &operation {
            ManualMeshOperation::MoveVertices { targets, .. } => {
                targets.iter().map(|v| v.id).collect::<Vec<_>>()
            }
            _ => vec![],
        };
        moving_ids.sort_unstable();
        let is_move = matches!(operation, ManualMeshOperation::MoveVertices { .. });
        let source = if is_move {
            self.manual_baseline
                .as_ref()
                .filter(|b| {
                    b.source.mesh_id == mesh_id
                        && b.moving_ids == moving_ids
                        && b.publication.is_same_snapshot(self.candidate_document())
                })
                .map(|b| b.source.clone())
        } else {
            None
        };
        let source = match source {
            Some(s) => s,
            None => Arc::new(ManualMeshSource::new(self.candidate_document(), mesh_id)?),
        };
        let mut plan = source.plan_candidate(
            self.candidate_document(),
            self.base_version(),
            operation,
            limits,
            &ConversionControl::default(),
        )?;
        plan.baseline = is_move.then_some(source);
        plan.moving_ids = moving_ids;
        Ok(plan)
    }
    pub fn apply_manual_mesh_plan(
        &mut self,
        plan: ManualMeshPlan,
    ) -> Result<ManualMeshTransferReport, SdkError> {
        self.ensure_active("apply_manual_mesh_plan")?;
        if plan.version != self.base_version()
            || !plan
                .publication_source
                .is_same_snapshot(self.candidate_document())
        {
            return Err(error(
                "STALE_MANUAL_MESH_PLAN",
                "Manual mesh source changed",
                vec![plan.mesh_id.clone()],
            ));
        }
        // The private plan owns the fully validated Core result, bound to the
        // exact publication snapshot above. Do not clone/revalidate it on apply.
        // This transfer retains all object/sample identities; only vertex IDs
        // and channel arrays can change, so no object-key inventory is retired.
        self.candidate_content_bytes.take();
        self.candidate = Some(plan.validated);
        self.binding_cost = Some(plan.report.cost);
        self.record(plan.changes, "apply_manual_mesh_plan", &plan.mesh_id)?;
        self.manual_baseline = plan.baseline.map(|source| ManualMeshBaseline {
            source,
            publication: self.candidate_document().clone(),
            moving_ids: plan.moving_ids,
        });
        Ok(plan.report)
    }
    pub fn edit_manual_mesh(
        &mut self,
        mesh_id: &str,
        operation: ManualMeshOperation,
    ) -> Result<ManualMeshResult, SdkError> {
        let plan = self.plan_manual_mesh(mesh_id, operation)?;
        let old = self.candidate_document().get_mesh(mesh_id).unwrap();
        let geometry = plan.geometry();
        let kept: HashSet<_> = geometry.vertex_ids.iter().copied().collect();
        let old_ids: HashSet<_> = old.vertex_ids.iter().copied().collect();
        let result = ManualMeshResult {
            added: geometry
                .vertex_ids
                .iter()
                .filter(|id| !old_ids.contains(id))
                .copied()
                .collect(),
            removed: old
                .vertex_ids
                .iter()
                .filter(|id| !kept.contains(id))
                .copied()
                .collect(),
            vertex_mapping: old
                .vertex_ids
                .iter()
                .map(|&id| (id, kept.contains(&id).then_some(id)))
                .collect(),
            changed: geometry.vertex_ids != old.vertex_ids
                || geometry.positions != old.base_positions
                || geometry.uvs != old.uvs
                || geometry.triangles != old.triangles
                || geometry.meshing != old.meshing,
            geometry,
            affected_edges: plan.affected_edges.clone(),
        };
        self.apply_manual_mesh_plan(plan)?;
        Ok(result)
    }
}
