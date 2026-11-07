use super::*;
impl Model<'_> {
    pub(crate) fn validate_mesh_properties(&self, m: &Mesh) -> Status {
        if !m.part_id.is_empty() && !self.content.parts.contains_key(&m.part_id) {
            return Status::error("MISSING_PART", format!("{}.part_id", m.id));
        }
        if !m.deformer_id.is_empty() && !self.content.transforms.contains_key(&m.deformer_id) {
            return Status::error("MISSING_TRANSFORM", format!("{}.deformer_id", m.id));
        }
        let s = validate_appearance(&m.appearance, &m.id);
        if !s.is_ok() {
            return s;
        }
        if let Some(order) = m.draw_order {
            let s = validate_draw_order(order, &m.id);
            if !s.is_ok() {
                return s;
            }
        }
        let mut seen = HashSet::new();
        for mask in &m.masks {
            if !seen.insert(mask.clone()) {
                return Status::error("INVALID_MASK", format!("{}.masks", m.id));
            }
            if !self.content.meshes.contains_key(mask) {
                return Status::error("MISSING_MESH", format!("{}.masks: {}", m.id, mask));
            }
        }
        // Masks render their source meshes directly. A source mesh's own masks
        // do not participate, so self references and cycles are well defined.
        Status::ok()
    }
}
