use super::*;
use crate::types::{ChangeKind, EditResult, Part, Status};
use std::collections::{HashMap, HashSet};

impl Document {
    pub fn part_order(&self) -> &[String] {
        &self.content.part_order
    }

    pub fn get_part(&self, id: &str) -> Option<&Part> {
        self.content.parts.get(id)
    }

    pub(super) fn validate_part(&self, p: &Part) -> Status {
        self.validation_model().validate_part(p)
    }

    pub fn create_part(&mut self, mut p: Part) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&p.parent_id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &p.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &p.id));
        }
        if self.contains_id(&p.id) {
            return self.failed(Status::error("DUPLICATE_ID", &p.id));
        }
        if p.runtime_id.is_empty() {
            p.runtime_id = p.id.clone();
        }
        let s = self.validate_part(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = p.id.clone();
        self.content.parts.insert(id.clone(), p);
        self.content.part_order.push(id.clone());
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    pub fn replace_part(&mut self, p: Part) -> EditResult {
        if let Err(status) = self
            .authoring_unlocked(&p.id)
            .and_then(|_| self.authoring_unlocked(&p.parent_id))
        {
            return self.failed(status);
        }
        if self
            .get_part(&p.id)
            .is_some_and(|old| old.parent_id != p.parent_id)
        {
            if let Err(status) = self.organization_subtree_unlocked(&[p.id.clone()]) {
                return self.failed(status);
            }
        }
        if self.get_part(&p.id) == Some(&p) {
            return self.failed(Status::ok());
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &p.id));
        }
        if !self.content.parts.contains_key(&p.id) {
            return self.failed(Status::error("MISSING_PART", &p.id));
        }
        let s = self.validate_part(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = p.id.clone();
        self.content.parts.insert(id.clone(), p);
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    pub fn sorted_parts(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        fn visit(
            id: &str,
            parts: &HashMap<String, Part>,
            seen: &mut HashSet<String>,
            result: &mut Vec<String>,
        ) {
            if id.is_empty() || !seen.insert(id.to_string()) {
                return;
            }
            if let Some(p) = parts.get(id) {
                visit(&p.parent_id, parts, seen, result);
                result.push(id.to_string());
            }
        }

        for id in &self.content.part_order {
            visit(id, &self.content.parts, &mut seen, &mut result);
        }
        result
    }
}
