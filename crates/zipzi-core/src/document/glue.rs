use super::*;
use crate::types::{ChangeKind, EditResult, Glue, Status};
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
        self.validation_model().validate_glue(glue)
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
