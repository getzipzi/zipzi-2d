use super::*;
use crate::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, ChangeKind, EditResult, Status,
};

impl Document {
    pub fn blend_key_table_order(&self) -> &[String] {
        &self.content.blend_key_table_order
    }

    pub fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.content.blend_key_tables.get(id)
    }

    pub(super) fn validate_blend_key_table(&self, table: &BlendShapeKeyTable) -> Status {
        self.validation_model().validate_blend_key_table(table)
    }

    pub fn create_blend_key_table(&mut self, mut table: BlendShapeKeyTable) -> EditResult {
        if let Err(status) = self.normalize_table(&mut table) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &table.id));
        }
        if self.contains_id(&table.id) {
            return self.failed(Status::error("DUPLICATE_ID", &table.id));
        }
        let s = self.validate_blend_key_table(&table);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = table.id.clone();
        let param_id = table.parameter_id.clone();
        self.content.blend_key_tables.insert(id.clone(), table);
        self.content.blend_key_table_order.push(id.clone());
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn replace_blend_key_table(&mut self, mut table: BlendShapeKeyTable) -> EditResult {
        if let Err(status) = self.normalize_table(&mut table) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&table.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &table.id));
        }
        if !self.content.blend_key_tables.contains_key(&table.id) {
            return self.failed(Status::error("MISSING_KEY_TABLE", &table.id));
        }
        let s = self.validate_blend_key_table(&table);
        if !s.is_ok() {
            return self.failed(s);
        }
        let old = self.get_blend_key_table(&table.id).unwrap();
        if (old.base_sample_id != table.base_sample_id
            || old
                .samples
                .iter()
                .map(|s| &s.id)
                .ne(table.samples.iter().map(|s| &s.id)))
            && self
                .content
                .blend_bindings
                .values()
                .any(|b| b.key_table_id == table.id)
        {
            return self.failed(Status::error("SHARED_BLEND_CONVERSION_REQUIRED", &table.id));
        }
        for b in self.content.blend_bindings.values() {
            if b.key_table_id == table.id && b.keyforms.len() != table.keys.len() {
                return self.failed(Status::error(
                    "KEYFORMS_REQUIRED",
                    format!(
                        "{}: binding keyform count does not match new key table",
                        b.id
                    ),
                ));
            }
        }
        let id = table.id.clone();
        let param_id = table.parameter_id.clone();
        self.content.blend_key_tables.insert(id.clone(), table);
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn blend_constraint_order(&self) -> &[String] {
        &self.content.blend_constraint_order
    }

    pub fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.content.blend_constraints.get(id)
    }

    pub(super) fn validate_blend_constraint(&self, constraint: &BlendShapeConstraint) -> Status {
        self.validation_model()
            .validate_blend_constraint(constraint)
    }

    pub fn create_blend_constraint(&mut self, mut constraint: BlendShapeConstraint) -> EditResult {
        if let Err(status) = self.normalize_constraint(&mut constraint) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &constraint.id));
        }
        if self.contains_id(&constraint.id) {
            return self.failed(Status::error("DUPLICATE_ID", &constraint.id));
        }
        let s = self.validate_blend_constraint(&constraint);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = constraint.id.clone();
        let param_id = constraint.parameter_id.clone();
        self.content
            .blend_constraints
            .insert(id.clone(), constraint);
        self.content.blend_constraint_order.push(id.clone());
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn replace_blend_constraint(&mut self, mut constraint: BlendShapeConstraint) -> EditResult {
        if let Err(status) = self.normalize_constraint(&mut constraint) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&constraint.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &constraint.id));
        }
        if !self.content.blend_constraints.contains_key(&constraint.id) {
            return self.failed(Status::error("MISSING_CONSTRAINT", &constraint.id));
        }
        let s = self.validate_blend_constraint(&constraint);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = constraint.id.clone();
        let param_id = constraint.parameter_id.clone();
        self.content
            .blend_constraints
            .insert(id.clone(), constraint);
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn blend_binding_order(&self) -> &[String] {
        &self.content.blend_binding_order
    }

    pub fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.content.blend_bindings.get(id)
    }

    pub fn blend_bindings_for_target(&self, target_id: &str) -> Vec<&BlendShapeBinding> {
        self.iter_blend_bindings_for_target(target_id).collect()
    }

    pub fn blend_binding_active(&self, b: &BlendShapeBinding) -> bool {
        self.get_blend_key_table(&b.key_table_id)
            .is_some_and(|t| !t.parameter_id.is_empty() && !t.keys.is_empty())
            && b.constraint_ids.iter().all(|id| {
                self.get_blend_constraint(id)
                    .is_some_and(|c| !c.parameter_id.is_empty() && !c.keys.is_empty())
            })
    }
    pub(crate) fn iter_active_blend_bindings_for_target<'a>(
        &'a self,
        id: &str,
    ) -> impl Iterator<Item = &'a BlendShapeBinding> {
        self.iter_blend_bindings_for_target(id)
            .filter(|b| self.blend_binding_active(b))
    }
    pub fn iter_blend_bindings_for_target(
        &self,
        target_id: &str,
    ) -> impl Iterator<Item = &BlendShapeBinding> {
        self.lookup()
            .blend_bindings
            .get(target_id)
            .into_iter()
            .flatten()
            .map(|id| &self.content.blend_bindings[id])
    }

    pub(super) fn validate_blend_binding(&self, b: &BlendShapeBinding) -> Status {
        self.validation_model().validate_blend_binding(b)
    }

    pub fn create_blend_binding(&mut self, mut b: BlendShapeBinding) -> EditResult {
        if let Err(status) = self.normalize_blend_forms(&mut b) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&b.target_id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.validate_blend_binding(&b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        let key_table = b.key_table_id.clone();
        self.content.blend_bindings.insert(id.clone(), b);
        self.content.blend_binding_order.push(id.clone());
        self.changed_blend_binding(vec![id, target, key_table])
    }

    pub fn replace_blend_binding(&mut self, mut b: BlendShapeBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&b.target_id) {
            return self.failed(status);
        }
        if let Err(status) = self.normalize_blend_forms(&mut b) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&b.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if !self.content.blend_bindings.contains_key(&b.id) {
            return self.failed(Status::error("MISSING_BINDING", &b.id));
        }
        let s = self.validate_blend_binding(&b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        let key_table = b.key_table_id.clone();
        self.content.blend_bindings.insert(id.clone(), b);
        self.changed_blend_binding(vec![id, target, key_table])
    }

    fn changed_blend_binding(&mut self, object_ids: Vec<String>) -> EditResult {
        // BlendShape forms reference samples owned by key tables; they do not
        // define axes or sample identities. Keep that immutable index while
        // invalidating all ordinary structural/evaluation state as usual.
        let identities = self.binding_lookup.clone();
        let meshes = self.content.mesh_order.clone();
        let result = self.changed(ChangeKind::Structure, meshes, object_ids);
        self.binding_lookup = identities;
        result
    }
}
