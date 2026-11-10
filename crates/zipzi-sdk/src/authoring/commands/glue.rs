use super::*;
use zipzi_core::{state::DocumentState, Glue, GlueVertexPair, VertexId};

impl Commands<'_> {
    /// Add stable endpoint pairs. Positions are matched by the editor in its
    /// current evaluated pose before calling this operation.
    pub fn bind_glue_vertices(
        &mut self,
        target: Option<&str>,
        mesh_a: &str,
        mesh_b: &str,
        pairs: &[(VertexId, VertexId)],
    ) -> Result<String, Status> {
        self.run(|this| {
            this.unlocked(mesh_a)?;
            this.unlocked(mesh_b)?;
            let model = this.builder.preview()?;
            let additions = glue_binding_additions(&model, target, mesh_a, mesh_b, pairs)?;
            let mut glue = if let Some(id) = target {
                this.unlocked(id)?;
                model.glue(id).unwrap().clone()
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let a = model.mesh(mesh_a).unwrap();
                let b = model.mesh(mesh_b).unwrap();
                Glue {
                    id: id.clone(),
                    runtime_id: id,
                    name: format!("{} / {}", a.name, b.name),
                    mesh_a_id: mesh_a.into(),
                    mesh_b_id: mesh_b.into(),
                    ..Default::default()
                }
            };
            let changed = !additions.is_empty();
            glue.pairs
                .extend(additions.into_iter().map(|(a, b)| GlueVertexPair {
                    vertex_a: Some(a),
                    vertex_b: Some(b),
                    ..Default::default()
                }));
            let id = glue.id.clone();
            if target.is_none() {
                this.builder.create_glue(glue)?;
            } else if changed {
                this.builder.replace_glue(glue)?;
            }
            Ok(id)
        })
    }

    /// Remove only the named pairs, retaining the Glue and its strength bindings.
    pub fn unbind_glue_vertices(&mut self, id: &str, pairs: &[String]) -> Result<(), Status> {
        self.run(|this| {
            let mut glue = this
                .builder
                .glue(id)
                .cloned()
                .ok_or_else(|| error("MISSING_GLUE", id))?;
            let requested: HashSet<_> = pairs.iter().map(String::as_str).collect();
            let before = glue.pairs.len();
            glue.pairs
                .retain(|pair| !requested.contains(pair.id.as_str()));
            if before - glue.pairs.len() != requested.len() {
                return Err(error(
                    "MISSING_GLUE_PAIR",
                    "A selected Glue pair no longer exists",
                ));
            }
            if before != glue.pairs.len() {
                this.replace_glue(glue)?;
            }
            Ok(())
        })
    }

    /// Author both weights by stable pair ID, without changing topology or strength.
    pub fn set_glue_weights(
        &mut self,
        id: &str,
        weights: &[(&str, [f32; 2])],
    ) -> Result<(), Status> {
        self.run(|this| {
            let mut glue = this
                .builder
                .glue(id)
                .cloned()
                .ok_or_else(|| error("MISSING_GLUE", id))?;
            validate_glue_weight_edit(&this.builder.preview()?, &glue)?;
            this.unlocked(id)?;
            this.unlocked(glue.mesh_a_id.as_str())?;
            this.unlocked(glue.mesh_b_id.as_str())?;
            let mut updates: HashMap<_, _> = weights.iter().copied().collect();
            if updates.len() != weights.len() || updates.values().any(|&w| !valid_weights(w)) {
                return Err(error(
                    "INVALID_GLUE_WEIGHTS",
                    "Glue weights must be nonnegative and sum to at most one",
                ));
            }
            let mut changed = false;
            for pair in &mut glue.pairs {
                if let Some([a, b]) = updates.remove(pair.id.as_str()) {
                    changed |= pair.weight_a != a || pair.weight_b != b;
                    pair.weight_a = a;
                    pair.weight_b = b;
                }
            }
            if !updates.is_empty() {
                return Err(error(
                    "MISSING_GLUE_PAIR",
                    "A painted Glue pair no longer exists",
                ));
            }
            if changed {
                this.replace_glue(glue)?;
            }
            Ok(())
        })
    }
}

/// Preflight the complete binding batch and return the new pairs in request order.
pub fn glue_binding_additions(
    model: &DocumentState,
    target: Option<&str>,
    mesh_a: &str,
    mesh_b: &str,
    pairs: &[(VertexId, VertexId)],
) -> Result<Vec<(VertexId, VertexId)>, Status> {
    if pairs.is_empty() || mesh_a == mesh_b {
        return Err(error(
            "GLUE_SELECTION",
            "Select overlapping vertices on two different meshes",
        ));
    }
    let a = model
        .mesh(mesh_a)
        .ok_or_else(|| error("MISSING_MESH", mesh_a))?;
    let b = model
        .mesh(mesh_b)
        .ok_or_else(|| error("MISSING_MESH", mesh_b))?;
    let a_ids: HashSet<_> = a.vertex_ids.iter().copied().collect();
    let b_ids: HashSet<_> = b.vertex_ids.iter().copied().collect();
    let counts = glue_endpoint_counts(model);
    let mut existing = HashSet::new();
    if let Some(id) = target {
        let glue = model.glue(id).ok_or_else(|| error("MISSING_GLUE", id))?;
        if glue.mesh_a_id.resolved_id() != Some(mesh_a)
            || glue.mesh_b_id.resolved_id() != Some(mesh_b)
        {
            return Err(error(
                "GLUE_MESHES",
                "Glue connects a different pair of meshes",
            ));
        }
        validate_glue_pairs(glue, &counts, mesh_a, mesh_b)?;
        existing.extend(glue.pairs.iter().map(|pair| (pair.vertex_a, pair.vertex_b)));
    }
    let mut requested_a = HashSet::new();
    let mut requested_b = HashSet::new();
    let mut additions = Vec::new();
    for &(a, b) in pairs {
        if !a_ids.contains(&a) || !b_ids.contains(&b) {
            return Err(error(
                "MISSING_VERTEX",
                "A selected vertex no longer exists",
            ));
        }
        if !requested_a.insert(a) || !requested_b.insert(b) {
            return Err(error(
                "GLUE_OCCUPIED",
                "A vertex can only participate in one Glue pair",
            ));
        }
        if existing.contains(&(Some(a), Some(b))) {
            continue;
        }
        if counts.contains_key(&(mesh_a, a)) || counts.contains_key(&(mesh_b, b)) {
            return Err(error(
                "GLUE_OCCUPIED",
                "A selected vertex already belongs to a Glue pair",
            ));
        }
        additions.push((a, b));
    }
    Ok(additions)
}

fn glue_endpoint_counts(model: &DocumentState) -> HashMap<(&str, VertexId), usize> {
    let mut counts = HashMap::new();
    for glue in model.glue_order().iter().filter_map(|id| model.glue(id)) {
        for pair in &glue.pairs {
            for (mesh, vertex) in [
                (&glue.mesh_a_id, pair.vertex_a),
                (&glue.mesh_b_id, pair.vertex_b),
            ] {
                if let (Some(mesh), Some(vertex)) = (mesh.resolved_id(), vertex) {
                    *counts.entry((mesh, vertex)).or_default() += 1;
                }
            }
        }
    }
    counts
}

/// Restrictions for painting or extending a Glue created by another authoring
/// tool or imported from an external format.
pub fn validate_glue_weight_edit(model: &DocumentState, glue: &Glue) -> Result<(), Status> {
    let [a, b] = [&glue.mesh_a_id, &glue.mesh_b_id]
        .map(|mesh| mesh.resolved_id().filter(|id| model.mesh(id).is_some()));
    let (Some(a), Some(b)) = (a, b) else {
        return Err(error("GLUE_MESHES", "Glue is missing an endpoint mesh"));
    };
    if a == b {
        return Err(error("GLUE_MESHES", "Glue connects a mesh to itself"));
    }
    validate_glue_pairs(glue, &glue_endpoint_counts(model), a, b)
}

fn validate_glue_pairs(
    glue: &Glue,
    counts: &HashMap<(&str, VertexId), usize>,
    a: &str,
    b: &str,
) -> Result<(), Status> {
    for pair in &glue.pairs {
        let (Some(va), Some(vb)) = (pair.vertex_a, pair.vertex_b) else {
            return Err(error("GLUE_ENDPOINT", "Glue has an unattached vertex"));
        };
        if counts.get(&(a, va)) != Some(&1) || counts.get(&(b, vb)) != Some(&1) {
            return Err(error(
                "GLUE_OCCUPIED",
                "Glue shares a vertex with another pair",
            ));
        }
        if !valid_weights([pair.weight_a, pair.weight_b]) {
            return Err(error(
                "INVALID_GLUE_WEIGHTS",
                "Glue has weights outside the brush range",
            ));
        }
    }
    Ok(())
}

fn valid_weights([a, b]: [f32; 2]) -> bool {
    a >= 0. && b >= 0. && a + b <= 1.
}
