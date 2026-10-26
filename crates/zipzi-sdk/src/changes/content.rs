use super::*;
use zipzi_core::document::SceneContentDomain as D;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneContentChange {
    pub entity: EntityRef,
    pub domain: D,
    pub topology: bool,
}
pub(super) fn summarize_content(
    before: &Document,
    after: &Document,
    ids: &[String],
    out: &mut ChangeSet,
) {
    let mut candidates: BTreeSet<String> = ids.iter().cloned().collect();
    candidates.extend(
        out.created
            .iter()
            .chain(&out.deleted)
            .chain(&out.modified)
            .map(|e| e.id.clone()),
    );
    for id in candidates.clone() {
        for d in [before, after] {
            if let Some(b) = d.get_binding(&id) {
                candidates.insert(b.mesh_id.clone());
            }
            if let Some(b) = d.get_scene_binding(&id) {
                candidates.insert(b.target_id().into());
                if let Some(os) = d.offscreen_for_part(b.target_id()) {
                    candidates.insert(os.id.clone());
                }
            }
            if let Some(b) = d.get_blend_binding(&id) {
                candidates.insert(b.target_id.clone());
            }
            if let Some(owner) = d.glue_pair_owner(&id) {
                candidates.insert(owner.into());
            }
        }
    }
    let mut changed = |id: &str, kind, domain, topology| {
        record_content_change(out, after.id(), id, kind, domain, topology);
    };
    for id in candidates {
        let id = id.as_str();
        if before.get_mesh(id).is_some() || after.get_mesh(id).is_some() {
            let a = before.get_mesh(id);
            let b = after.get_mesh(id);
            let topology = a.map(|m| {
                (
                    &m.vertex_ids,
                    &m.triangles,
                    &m.meshing,
                    &m.part_id,
                    &m.deformer_id,
                )
            }) != b.map(|m| {
                (
                    &m.vertex_ids,
                    &m.triangles,
                    &m.meshing,
                    &m.part_id,
                    &m.deformer_id,
                )
            });
            if topology
                || a.map(|m| (&m.base_positions, &m.uvs)) != b.map(|m| (&m.base_positions, &m.uvs))
                || before.binding_for_mesh(id) != after.binding_for_mesh(id)
                || before
                    .iter_blend_bindings_for_target(id)
                    .collect::<Vec<_>>()
                    != after.iter_blend_bindings_for_target(id).collect::<Vec<_>>()
            {
                changed(id, ObjectKind::Mesh, D::MeshTopology, topology);
            }
        }
        if before.get_transform(id).and_then(|t| t.warp()).is_some()
            || after.get_transform(id).and_then(|t| t.warp()).is_some()
        {
            let a = before.get_transform(id).and_then(|t| t.warp());
            let b = after.get_transform(id).and_then(|t| t.warp());
            let topology = a.map(|w| {
                (
                    w.rows,
                    w.columns,
                    w.quad,
                    w.bezier.as_ref().map(|v| (v.rows, v.columns)),
                )
            }) != b.map(|w| {
                (
                    w.rows,
                    w.columns,
                    w.quad,
                    w.bezier.as_ref().map(|v| (v.rows, v.columns)),
                )
            });
            if a != b
                || before.binding_for_scene(id) != after.binding_for_scene(id)
                || before
                    .iter_blend_bindings_for_target(id)
                    .collect::<Vec<_>>()
                    != after.iter_blend_bindings_for_target(id).collect::<Vec<_>>()
            {
                changed(id, ObjectKind::Transform, D::WarpTopology, topology);
            }
        }
        if before.get_glue(id).is_some() || after.get_glue(id).is_some() {
            let a = before.get_glue(id);
            let b = after.get_glue(id);
            let pairs = |g: Option<&zipzi_core::Glue>| {
                g.map(|g| g.pairs.iter().map(|p| p.id.clone()).collect::<Vec<_>>())
            };
            if a.map(|g| (&g.pairs, &g.mesh_a_id, &g.mesh_b_id))
                != b.map(|g| (&g.pairs, &g.mesh_a_id, &g.mesh_b_id))
            {
                changed(id, ObjectKind::Glue, D::GluePairs, pairs(a) != pairs(b));
            }
        }
        if before.get_offscreen(id).is_some() || after.get_offscreen(id).is_some() {
            let a = before.get_offscreen(id);
            let b = after.get_offscreen(id);
            let grid = |d: &Document, g: Option<&zipzi_core::Offscreen>| {
                g.and_then(|g| d.binding_for_scene(&g.part_id))
                    .map(|b| b.axes.clone())
            };
            let topology = a.map(|v| (&v.part_id, v.keyforms.len()))
                != b.map(|v| (&v.part_id, v.keyforms.len()))
                || grid(before, a) != grid(after, b);
            if topology
                || a.map(|v| (&v.keyforms, &v.part_keyform_indices))
                    != b.map(|v| (&v.keyforms, &v.part_keyform_indices))
            {
                changed(id, ObjectKind::Offscreen, D::OffscreenKeyforms, topology);
            }
        }
    }
    for c in &out.content_changes {
        let collection = CollectionRef::SceneContent {
            document_id: after.id().into(),
            owner_id: c.entity.id.clone(),
            domain: c.domain.clone(),
        };
        if c.topology && !out.collections.contains(&collection) {
            out.collections.push(collection);
        }
        if c.domain == D::OffscreenKeyforms {
            let reference = |d: &Document| {
                d.get_offscreen(&c.entity.id)
                    .map(|os| SceneContentReferenceValue::OffscreenMapping {
                        part_id: os.part_id.clone(),
                        indices: os.part_keyform_indices.clone(),
                    })
                    .unwrap_or(SceneContentReferenceValue::Absent)
            };
            let a = reference(before);
            let b = reference(after);
            if a != b {
                out.content_references.push(SceneContentReferenceChange {
                    entity: c.entity.clone(),
                    before: a,
                    after: b,
                });
            }
        }
    }
    // Stable pair objects are independently observable, including owner changes.
    for id in before
        .glue_order()
        .iter()
        .chain(after.glue_order())
        .collect::<BTreeSet<_>>()
    {
        let a = before.get_glue(id);
        let b = after.get_glue(id);
        let pairs_changed = a.map(|g| &g.pairs) != b.map(|g| &g.pairs);
        let endpoints_changed =
            a.map(|g| (&g.mesh_a_id, &g.mesh_b_id)) != b.map(|g| (&g.mesh_a_id, &g.mesh_b_id));
        if !pairs_changed && !endpoints_changed {
            continue;
        }
        let collection = CollectionRef::SceneContent {
            document_id: after.id().into(),
            owner_id: id.clone(),
            domain: D::GluePairs,
        };
        if pairs_changed && !out.collections.contains(&collection) {
            out.collections.push(collection);
        }
        for pair_id in a
            .into_iter()
            .flat_map(|g| &g.pairs)
            .chain(b.into_iter().flat_map(|g| &g.pairs))
            .map(|p| p.id.as_str())
            .collect::<BTreeSet<_>>()
        {
            let before_ref = pair_reference(before, pair_id);
            let after_ref = pair_reference(after, pair_id);
            if before_ref != after_ref
                && !out
                    .content_references
                    .iter()
                    .any(|r| r.entity.kind == ObjectKind::GlueVertexPair && r.entity.id == pair_id)
            {
                out.content_references.push(SceneContentReferenceChange {
                    entity: EntityRef::new(after.id(), ObjectKind::GlueVertexPair, pair_id),
                    before: before_ref,
                    after: after_ref,
                });
            }
        }
        for p in b.into_iter().flat_map(|g| &g.pairs) {
            if before.get_glue_pair(&p.id) != Some(p)
                || before.glue_pair_owner(&p.id) != after.glue_pair_owner(&p.id)
                || endpoints_changed
            {
                out.modified.push(EntityRef::new(
                    after.id(),
                    ObjectKind::GlueVertexPair,
                    &p.id,
                ));
            }
        }
    }
}

pub(super) fn record_content_change(
    out: &mut ChangeSet,
    document_id: &str,
    id: &str,
    kind: ObjectKind,
    domain: D,
    topology: bool,
) {
    let entity = EntityRef::new(document_id, kind, id);
    out.content_changes.push(SceneContentChange {
        entity: entity.clone(),
        domain,
        topology,
    });
    out.modified.push(entity);
    out.categories.insert(ChangeCategory::Content);
    if topology {
        out.categories.insert(ChangeCategory::Topology);
    }
    out.categories.insert(ChangeCategory::EvaluationInputs);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SceneContentReferenceValue {
    Absent,
    Pair {
        glue_id: String,
        mesh_a: zipzi_core::SceneReference,
        mesh_b: zipzi_core::SceneReference,
        vertex_a: Option<zipzi_core::VertexId>,
        vertex_b: Option<zipzi_core::VertexId>,
    },
    OffscreenMapping {
        part_id: String,
        indices: Vec<i32>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneContentReferenceChange {
    pub entity: EntityRef,
    pub before: SceneContentReferenceValue,
    pub after: SceneContentReferenceValue,
}
impl SceneContentReferenceChange {
    pub(super) fn extra_bytes(&self) -> usize {
        let bytes = |v: &SceneContentReferenceValue| match v {
            SceneContentReferenceValue::Absent => 0,
            SceneContentReferenceValue::Pair {
                glue_id,
                mesh_a,
                mesh_b,
                ..
            } => glue_id.capacity() + mesh_a.capacity() + mesh_b.capacity(),
            SceneContentReferenceValue::OffscreenMapping { part_id, indices } => {
                part_id.capacity() + indices.capacity() * std::mem::size_of::<i32>()
            }
        };
        self.entity.id.capacity()
            + self.entity.document_id.capacity()
            + bytes(&self.before)
            + bytes(&self.after)
    }
}
fn pair_reference(d: &Document, id: &str) -> SceneContentReferenceValue {
    match d
        .glue_pair_owner(id)
        .and_then(|owner| d.get_glue(owner))
        .zip(d.get_glue_pair(id))
    {
        Some((g, p)) => SceneContentReferenceValue::Pair {
            glue_id: g.id.clone(),
            mesh_a: g.mesh_a_id.clone(),
            mesh_b: g.mesh_b_id.clone(),
            vertex_a: p.vertex_a,
            vertex_b: p.vertex_b,
        },
        None => SceneContentReferenceValue::Absent,
    }
}
