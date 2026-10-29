use super::*;
use crate::{BlendShapeTargetKind, DeltaKeyforms};
impl Document {
    pub(super) fn plan_mesh_content(
        &self,
        plan: &mut SceneContentPlan,
        edit: SceneContentEdit,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        use SceneContentEdit as E;
        let owned_id = match &edit {
            E::MeshVertices { mesh_id, .. }
            | E::InsertMeshVertices { mesh_id, .. }
            | E::DeleteMeshVertices { mesh_id, .. }
            | E::ReorderMeshVertices { mesh_id, .. }
            | E::InsertTriangle { mesh_id, .. }
            | E::PatchTriangle { mesh_id, .. }
            | E::DeleteTriangles { mesh_id, .. }
            | E::ReplaceTriangles { mesh_id, .. } => mesh_id,
            _ => unreachable!(),
        }
        .clone();
        let id = &owned_id;
        self.content_unlocked(id)?;
        let old = self
            .get_mesh(id)
            .ok_or_else(|| Status::error("MISSING_MESH", id))?;
        let mut mesh = old.clone();
        plan.domain.affected.push(id.clone());
        if let E::MeshVertices {
            target, updates, ..
        } = &edit
        {
            distinct(&updates.iter().map(|v| v.vertex_id).collect::<Vec<_>>())?;
            if *target != ShapeTarget::Base && updates.iter().any(|u| u.uv.is_some()) {
                return Err(Status::error("UV_IS_BASE_CONTENT", id));
            }
            let slots = updates
                .iter()
                .map(|u| {
                    self.vertex_slot(id, u.vertex_id)
                        .ok_or_else(|| Status::error("MISSING_VERTEX", u.vertex_id.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for u in updates {
                if u.position
                    .into_iter()
                    .chain(u.uv)
                    .any(|v| !v.x.is_finite() || !v.y.is_finite())
                {
                    return Err(Status::error("INVALID_POSITION", id));
                }
            }
            match target {
                ShapeTarget::Base => {
                    for (u, &i) in updates.iter().zip(&slots) {
                        if let Some(p) = u.position {
                            mesh.base_positions[i] = p;
                        }
                        if let Some(p) = u.uv {
                            mesh.uvs[i] = p;
                        }
                    }
                    plan.kind = if updates.iter().all(|u| u.uv.is_none()) {
                        ChangeKind::Positions
                    } else {
                        ChangeKind::Structure
                    };
                }
                ShapeTarget::Ordinary {
                    binding_id,
                    samples,
                } => {
                    self.content_unlocked(binding_id)?;
                    let mut b = self
                        .get_binding(binding_id)
                        .filter(|b| b.mesh_id == *id)
                        .ok_or_else(|| Status::error("WRONG_SHAPE_TARGET", binding_id))?
                        .clone();
                    let f = b
                        .keyforms
                        .iter_mut()
                        .find(|f| same_samples(&f.samples, samples))
                        .ok_or_else(|| Status::error("MISSING_KEYFORM", binding_id))?;
                    for (u, &i) in updates.iter().zip(&slots) {
                        if let Some(p) = u.position {
                            f.positions[i] = p;
                        }
                    }
                    plan.domain.affected.push(b.id.clone());
                    plan.domain.delta.bindings.push(b);
                }
                ShapeTarget::Blend {
                    binding_id,
                    sample_id,
                } => {
                    self.content_unlocked(binding_id)?;
                    let mut b = self
                        .get_blend_binding(binding_id)
                        .filter(|b| {
                            b.target_id == *id && b.target_kind == BlendShapeTargetKind::Mesh
                        })
                        .ok_or_else(|| Status::error("WRONG_SHAPE_TARGET", binding_id))?
                        .clone();
                    let DeltaKeyforms::Mesh(forms) = &mut b.keyforms else {
                        unreachable!()
                    };
                    let f = forms
                        .iter_mut()
                        .find(|f| f.sample_id == *sample_id)
                        .ok_or_else(|| Status::error("MISSING_KEYFORM", sample_id))?;
                    for (u, &i) in updates.iter().zip(&slots) {
                        if let Some(p) = u.position {
                            f.positions[i] = p;
                        }
                    }
                    plan.domain.affected.push(b.id.clone());
                    plan.domain.delta.blends.push(b);
                }
            }
            plan.domain.delta.meshes.push(mesh);
            return Ok(());
        }
        let mut indices: Vec<Option<usize>> = (0..old.vertex_ids.len()).map(Some).collect();
        let mut added = vec![];
        match edit {
            E::InsertMeshVertices {
                vertices, before, ..
            } => {
                distinct(&vertices.iter().map(|v| v.vertex_id).collect::<Vec<_>>())?;
                for v in &vertices {
                    if old.vertex_ids.contains(&v.vertex_id) {
                        return Err(Status::error("DUPLICATE_VERTEX", v.vertex_id.to_string()));
                    }
                    check_seed(old, v)?;
                }
                let at = match before {
                    Some(v) => old
                        .vertex_ids
                        .iter()
                        .position(|id| *id == v)
                        .ok_or_else(|| Status::error("MISSING_VERTEX", v.to_string()))?,
                    None => old.vertex_ids.len(),
                };
                mesh.vertex_ids
                    .splice(at..at, vertices.iter().map(|v| v.vertex_id));
                indices.splice(at..at, vertices.iter().map(|_| None));
                plan.report.vertex_sources = vertices.clone();
                added = vertices;
            }
            E::DeleteMeshVertices {
                vertices, policy, ..
            } => {
                distinct(&vertices)?;
                if vertices.iter().any(|v| !old.vertex_ids.contains(v)) {
                    return Err(Status::error("MISSING_VERTEX", id));
                }
                let deleted: HashSet<_> = vertices.iter().copied().collect();
                let triangle_slots: Vec<_> = old
                    .triangles
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.iter().any(|v| deleted.contains(v)))
                    .map(|(i, _)| i)
                    .collect();
                let mut users = vec![];
                if !triangle_slots.is_empty() {
                    users.push(id.clone());
                }
                for source in self.glues_for_mesh(id) {
                    let mut g = source.clone();
                    let retired: Vec<_> = g
                        .pairs
                        .iter()
                        .filter(|p| {
                            (g.mesh_a_id == id.as_str()
                                && p.vertex_a.is_some_and(|v| deleted.contains(&v)))
                                || (g.mesh_b_id == id.as_str()
                                    && p.vertex_b.is_some_and(|v| deleted.contains(&v)))
                        })
                        .map(|p| p.id.clone())
                        .collect();
                    if !retired.is_empty() {
                        users.extend(retired.clone());
                        self.content_unlocked(&g.id)?;
                        g.pairs.retain(|p| !retired.contains(&p.id));
                        plan.report.removed_pair_ids.extend(retired);
                        plan.domain.affected.push(g.id.clone());
                        plan.domain.delta.glues.push(g);
                    }
                }
                if policy == VertexDeletePolicy::RejectReferences && !users.is_empty() {
                    return Err(Status::error("VERTEX_REFERENCED", users.join(", ")));
                }
                mesh.triangles
                    .retain(|t| !t.iter().any(|v| deleted.contains(v)));
                plan.report.removed_triangle_slots = triangle_slots;
                plan.report.removed_vertices = vertices;
                indices = (0..old.vertex_ids.len())
                    .filter(|&i| !deleted.contains(&old.vertex_ids[i]))
                    .map(Some)
                    .collect();
                mesh.vertex_ids = indices.iter().map(|i| old.vertex_ids[i.unwrap()]).collect();
            }
            E::ReorderMeshVertices { order, .. } => {
                distinct(&order)?;
                if order.len() != old.vertex_ids.len() {
                    return Err(Status::error("INVALID_PERMUTATION", id));
                }
                indices = order
                    .iter()
                    .map(|&v| {
                        self.vertex_slot(id, v)
                            .map(Some)
                            .ok_or_else(|| Status::error("MISSING_VERTEX", v.to_string()))
                    })
                    .collect::<Result<_, _>>()?;
                mesh.vertex_ids = order;
            }
            E::InsertTriangle {
                vertices, before, ..
            } => {
                let at = before.unwrap_or(mesh.triangles.len());
                if at > mesh.triangles.len() {
                    return Err(Status::error("INVALID_TRIANGLE_SLOT", id));
                }
                mesh.triangles.insert(at, vertices);
            }
            E::PatchTriangle { slot, vertices, .. } => {
                *mesh
                    .triangles
                    .get_mut(slot)
                    .ok_or_else(|| Status::error("INVALID_TRIANGLE_SLOT", id))? = vertices;
            }
            E::DeleteTriangles { slots, .. } => {
                distinct(&slots)?;
                if slots.iter().any(|&i| i >= mesh.triangles.len()) {
                    return Err(Status::error("INVALID_TRIANGLE_SLOT", id));
                }
                plan.report.removed_triangle_slots = slots.clone();
                mesh.triangles = mesh
                    .triangles
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !slots.contains(i))
                    .map(|(_, v)| v)
                    .collect();
            }
            E::ReplaceTriangles { triangles, .. } => mesh.triangles = triangles,
            _ => unreachable!(),
        }
        // Every slot-bearing shape follows the same vertex permutation/source.
        if mesh.vertex_ids != old.vertex_ids {
            mesh.base_positions = remap(
                &old.base_positions,
                &indices,
                &added,
                false,
                true,
                old,
                control,
            )?;
            mesh.uvs = remap(&old.uvs, &indices, &added, false, false, old, control)?;
            if let Some(b) = self.binding_for_mesh(id) {
                self.content_unlocked(&b.id)?;
                let mut b = b.clone();
                for f in &mut b.keyforms {
                    f.positions = remap(&f.positions, &indices, &added, false, true, old, control)?;
                }
                plan.domain.affected.push(b.id.clone());
                plan.domain.delta.bindings.push(b);
            }
            for b in self.iter_blend_bindings_for_target(id) {
                self.content_unlocked(&b.id)?;
                let mut b = b.clone();
                if let DeltaKeyforms::Mesh(forms) = &mut b.keyforms {
                    for f in forms {
                        f.positions =
                            remap(&f.positions, &indices, &added, true, true, old, control)?;
                    }
                }
                plan.domain.affected.push(b.id.clone());
                plan.domain.delta.blends.push(b);
            }
        }
        plan.domain.delta.meshes.push(mesh);
        Ok(())
    }
}
pub(super) fn same_samples(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter().collect::<HashSet<_>>() == b.iter().collect()
        && a.iter().collect::<HashSet<_>>().len() == a.len()
}
fn check_seed(old: &Mesh, v: &NewMeshVertex) -> Result<(), Status> {
    if [v.position, v.uv]
        .iter()
        .any(|v| !v.x.is_finite() || !v.y.is_finite())
    {
        return Err(Status::error("INVALID_POSITION", v.vertex_id.to_string()));
    }
    if let VertexSeed::Interpolate { weights } = &v.seed {
        distinct(&weights.iter().map(|w| w.vertex_id).collect::<Vec<_>>())?;
        if weights.is_empty()
            || weights.iter().any(|w| {
                !w.weight.is_finite() || w.weight < 0. || !old.vertex_ids.contains(&w.vertex_id)
            })
            || (weights.iter().map(|w| w.weight).sum::<f32>() - 1.).abs() > 1e-5
        {
            return Err(Status::error(
                "INVALID_VERTEX_SOURCE",
                v.vertex_id.to_string(),
            ));
        }
    }
    Ok(())
}
fn remap(
    values: &[Vec2],
    indices: &[Option<usize>],
    added: &[NewMeshVertex],
    delta: bool,
    position: bool,
    old: &Mesh,
    control: &ConversionControl,
) -> Result<Vec<Vec2>, Status> {
    let mut next = added.iter();
    let mut out = Vec::with_capacity(indices.len());
    for (i, slot) in indices.iter().enumerate() {
        if i % 256 == 0 {
            control.check()?;
        }
        out.push(if let Some(i) = slot {
            values[*i]
        } else {
            let v = next.next().unwrap();
            if !position {
                v.uv
            } else if !delta && values.as_ptr() == old.base_positions.as_ptr() {
                v.position
            } else {
                match &v.seed {
                    VertexSeed::Base => {
                        if delta {
                            Vec2::default()
                        } else {
                            v.position
                        }
                    }
                    VertexSeed::Interpolate { weights } => {
                        let weights: Vec<_> = weights
                            .iter()
                            .map(|w| {
                                (
                                    old.vertex_ids
                                        .iter()
                                        .position(|id| *id == w.vertex_id)
                                        .unwrap(),
                                    f64::from(w.weight),
                                )
                            })
                            .collect();
                        crate::geometry::sample_vertex_channel(values, &weights)?
                    }
                }
            }
        });
    }
    Ok(out)
}
