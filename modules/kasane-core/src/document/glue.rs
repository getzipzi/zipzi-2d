use super::*;
use crate::types::{ChangeKind, EditResult, Glue, ParameterKind, Status};
use std::collections::HashSet;

impl Document {
    fn pair_lookup(&self) -> &std::collections::HashMap<String, (String, usize)> {
        self.pair_lookup.get_or_init(|| {
            std::sync::Arc::new(
                self.content
                    .glues
                    .values()
                    .flat_map(|g| {
                        g.pairs
                            .iter()
                            .enumerate()
                            .map(|(slot, p)| (p.id.clone(), (g.id.clone(), slot)))
                    })
                    .collect(),
            )
        })
    }
    pub fn get_glue_pair(&self, id: &str) -> Option<&crate::GlueVertexPair> {
        let (owner, slot) = self.pair_lookup().get(id)?;
        self.get_glue(owner)?
            .pairs
            .get(*slot)
            .filter(|p| p.id == id)
    }
    pub fn glue_pair_owner(&self, id: &str) -> Option<&str> {
        self.pair_lookup().get(id).map(|v| v.0.as_str())
    }
    pub fn glue_order(&self) -> &[String] {
        &self.content.glue_order
    }

    pub fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.content.glues.get(id)
    }

    pub fn glues_for_mesh(&self, mesh_id: &str) -> Vec<&Glue> {
        self.content
            .glue_order
            .iter()
            .filter_map(|id| self.content.glues.get(id))
            .filter(|g| g.mesh_a_id == mesh_id || g.mesh_b_id == mesh_id)
            .collect()
    }

    pub(super) fn validate_glue(&self, glue: &Glue) -> Status {
        if !valid_uuid(&glue.id) {
            return Status::error("INVALID_ID", &glue.id);
        }
        for reference in [&glue.mesh_a_id, &glue.mesh_b_id] {
            let status = reference.validate();
            if !status.is_ok() {
                return status;
            }
            if reference
                .resolved_id()
                .is_some_and(|id| !self.content.meshes.contains_key(id))
            {
                return Status::error("MISSING_MESH", reference.as_str());
            }
        }
        let slots_a = glue
            .mesh_a_id
            .resolved_id()
            .and_then(|id| self.vertex_slots.get(id));
        let slots_b = glue
            .mesh_b_id
            .resolved_id()
            .and_then(|id| self.vertex_slots.get(id));
        let mut ids = HashSet::new();
        for pair in &glue.pairs {
            if !valid_uuid(&pair.id)
                || pair.id == glue.id
                || glue.binding.as_ref().is_some_and(|b| {
                    b.axes
                        .iter()
                        .any(|a| a.id == pair.id || a.samples.iter().any(|s| s.id == pair.id))
                })
                || !ids.insert(pair.id.as_str())
                || (self.contains_id(&pair.id)
                    && self.glue_pair_owner(&pair.id) != Some(glue.id.as_str()))
            {
                return Status::error("DUPLICATE_PAIR_ID", &pair.id);
            }
            if pair
                .vertex_a
                .is_some_and(|v| slots_a.is_some_and(|slots| !slots.contains_key(&v)))
            {
                return Status::error(
                    "MISSING_VERTEX",
                    format!(
                        "{}: vertex {} not in mesh_a",
                        glue.id,
                        pair.vertex_a.unwrap()
                    ),
                );
            }
            if pair
                .vertex_b
                .is_some_and(|v| slots_b.is_some_and(|slots| !slots.contains_key(&v)))
            {
                return Status::error(
                    "MISSING_VERTEX",
                    format!(
                        "{}: vertex {} not in mesh_b",
                        glue.id,
                        pair.vertex_b.unwrap()
                    ),
                );
            }
            if !pair.weight_a.is_finite()
                || pair.weight_a < 0.0
                || !pair.weight_b.is_finite()
                || pair.weight_b < 0.0
            {
                return Status::error(
                    "INVALID_WEIGHT",
                    format!("{}: weights must be finite and non-negative", glue.id),
                );
            }
        }
        if !glue.intensity.is_finite() {
            return Status::error(
                "INVALID_INTENSITY",
                format!("{}: intensity must be finite", glue.id),
            );
        }
        if let Some(binding) = &glue.binding {
            if binding.axes.len() > 16 {
                return Status::error("INVALID_BINDING", "Glue requires 1..16 axes");
            }
            let mut total = if binding.axes.iter().all(|a| a.parameter_id.is_empty()) {
                0usize
            } else {
                1usize
            };
            let mut seen = HashSet::new();
            for axis in binding.axes.iter().filter(|a| !a.parameter_id.is_empty()) {
                let Some(p) = self.get_parameter(&axis.parameter_id) else {
                    return Status::error("MISSING_PARAMETER", &axis.parameter_id);
                };
                if p.kind != ParameterKind::Normal {
                    return Status::error("INVALID_PARAMETER_KIND", &p.id);
                }
                if !seen.insert(&axis.parameter_id) {
                    return Status::error("DUPLICATE_AXIS", &p.id);
                }
                if axis.keys.is_empty() || axis.keys.len() > i32::MAX as usize / total {
                    return Status::error("INVALID_KEYS", &glue.id);
                }
                total *= axis.keys.len();
                for (i, &key) in axis.keys.iter().enumerate() {
                    if !key.is_finite()
                        || key < p.minimum
                        || key > p.maximum
                        || (i > 0 && key <= axis.keys[i - 1])
                    {
                        return Status::error("INVALID_KEYS", &p.id);
                    }
                }
            }
            if binding.keyforms.len() != total {
                return Status::error("INCOMPLETE_KEYFORMS", &glue.id);
            }
            let active: Vec<_> = binding
                .axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty())
                .collect();
            for (mut slot, f) in binding.keyforms.iter().enumerate() {
                let expected: Vec<_> = active
                    .iter()
                    .map(|a| {
                        let i = slot % a.samples.len();
                        slot /= a.samples.len();
                        &a.samples[i].id
                    })
                    .collect();
                if f.samples.iter().ne(expected.into_iter()) {
                    return Status::error("INVALID_SAMPLE_REFERENCE", &glue.id);
                }
            }
            if binding.keyforms.iter().any(|k| !k.intensity.is_finite()) {
                return Status::error("INVALID_GLUE", "Glue keyform intensity must be finite");
            }
        }
        Status::ok()
    }

    pub fn create_glue(&mut self, mut glue: Glue) -> EditResult {
        if let Some(b) = &mut glue.binding {
            if let Err(s) = self.normalize_owner_axes(&glue.id, &mut b.axes) {
                return self.failed(s);
            }
            if let Err(s) = self.check_binding_ids(
                &glue.id,
                b.axes.iter().flat_map(|a| {
                    std::iter::once(a.id.clone()).chain(a.samples.iter().map(|s| s.id.clone()))
                }),
            ) {
                return self.failed(s);
            }
        }
        if let Err(s) = self.normalize_glue_forms(&mut glue) {
            return self.failed(s);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &glue.id));
        }
        if self.contains_id(&glue.id) {
            return self.failed(Status::error("DUPLICATE_ID", &glue.id));
        }
        for (other_id, other) in &self.content.glues {
            if other.runtime_id == glue.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", glue.id, other_id),
                ));
            }
        }
        if glue.runtime_id.is_empty() {
            glue.runtime_id = glue.id.clone();
        }
        let s = self.validate_glue(&glue);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = glue.id.clone();
        let mesh_a = glue.mesh_a_id.as_str().to_owned();
        let mesh_b = glue.mesh_b_id.as_str().to_owned();
        self.content.glues.insert(id.clone(), glue);
        self.content.glue_order.push(id.clone());
        let mut affected = vec![mesh_a.clone()];
        if mesh_a != mesh_b {
            affected.push(mesh_b.clone());
        }
        affected.retain(|id| !id.is_empty());
        self.changed(ChangeKind::Structure, affected, vec![id])
    }

    pub fn replace_glue(&mut self, mut glue: Glue) -> EditResult {
        if let Some(old) = self.get_glue(&glue.id) {
            for pair in &old.pairs {
                if glue.pairs.iter().find(|p| p.id == pair.id) != Some(pair)
                    || old.pairs.iter().position(|p| p.id == pair.id)
                        != glue.pairs.iter().position(|p| p.id == pair.id)
                    || old.mesh_a_id != glue.mesh_a_id
                    || old.mesh_b_id != glue.mesh_b_id
                {
                    if let Err(status) = self.authoring_unlocked(&pair.id) {
                        return self.failed(status);
                    }
                }
            }
        }
        if let Some(b) = &mut glue.binding {
            if let Err(s) = self.normalize_owner_axes(&glue.id, &mut b.axes) {
                return self.failed(s);
            }
            if let Err(s) = self.check_binding_ids(
                &glue.id,
                b.axes.iter().flat_map(|a| {
                    std::iter::once(a.id.clone()).chain(a.samples.iter().map(|s| s.id.clone()))
                }),
            ) {
                return self.failed(s);
            }
        }
        if let Err(status) = self.authoring_unlocked(&glue.id) {
            return self.failed(status);
        }
        if let Err(s) = self.normalize_glue_forms(&mut glue) {
            return self.failed(s);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &glue.id));
        }
        let prev = match self.content.glues.get(&glue.id) {
            Some(g) => (
                g.mesh_a_id.as_str().to_owned(),
                g.mesh_b_id.as_str().to_owned(),
            ),
            None => return self.failed(Status::error("MISSING_GLUE", &glue.id)),
        };
        for (other_id, other) in &self.content.glues {
            if other_id != &glue.id && other.runtime_id == glue.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", glue.id, other_id),
                ));
            }
        }
        if glue.runtime_id.is_empty() {
            glue.runtime_id = glue.id.clone();
        }
        let s = self.validate_glue(&glue);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = glue.id.clone();
        let mesh_a = glue.mesh_a_id.as_str().to_owned();
        let mesh_b = glue.mesh_b_id.as_str().to_owned();
        let keep: HashSet<_> = glue.pairs.iter().map(|p| p.id.as_str()).collect();
        let retired: Vec<_> = self
            .get_glue(&id)
            .into_iter()
            .flat_map(|g| &g.pairs)
            .filter(|p| !keep.contains(p.id.as_str()))
            .map(|p| p.id.clone())
            .collect();
        for pair in retired {
            self.content.editor_state.objects.remove(&pair);
        }
        self.content.glues.insert(id.clone(), glue);
        let mut affected = vec![mesh_a.clone(), mesh_b.clone(), prev.0, prev.1];
        affected.sort();
        affected.dedup();
        affected.retain(|id| !id.is_empty());
        self.changed(ChangeKind::Structure, affected, vec![id])
    }
}
