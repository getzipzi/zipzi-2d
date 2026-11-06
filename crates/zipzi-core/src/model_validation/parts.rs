use super::*;
impl Model<'_> {
    pub(crate) fn validate_part(&self, p: &Part) -> Status {
        if !valid_uuid(&p.id) || p.runtime_id.is_empty() {
            return Status::error("INVALID_ID", &p.id);
        }
        for (id, other) in &self.content.parts {
            if id != &p.id && other.runtime_id == p.runtime_id {
                return Status::error("DUPLICATE_RUNTIME_ID", &p.id);
            }
        }
        if self.content.display_info.parts.as_ref().is_some_and(|entries| entries.iter().any(|entry| {
            matches!(entry, CdiPartEntry::Unresolved { runtime_id, .. } if runtime_id == &p.runtime_id)
        })) {
            return Status::error("DUPLICATE_RUNTIME_ID", format!("{}.runtime_id collides with unresolved CDI part", p.id));
        }
        let mut seen = HashSet::new();
        seen.insert(p.id.clone());
        let mut parent_id = p.parent_id.clone();
        while !parent_id.is_empty() {
            if !seen.insert(parent_id.clone()) {
                return Status::error("RELATION_CYCLE", format!("{}.parent_id", p.id));
            }
            match self.get_part(&parent_id) {
                Some(parent) => parent_id = parent.parent_id.clone(),
                None => return Status::error("MISSING_PART", parent_id),
            }
        }
        validate_draw_order(p.draw_order, &p.id)
    }
}
