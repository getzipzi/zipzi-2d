use super::*;
impl Model<'_> {
    pub(crate) fn validate_offscreen(&self, os: &Offscreen) -> Status {
        let klen = self
            .binding_for_scene(&os.part_id)
            .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
            .map_or(1, |binding| binding.track.len());
        self.validate_offscreen_key_count(os, klen)
    }
    pub(crate) fn validate_offscreen_key_count(&self, os: &Offscreen, klen: usize) -> Status {
        if !valid_uuid(&os.id) {
            return Status::error("INVALID_ID", format!("{}: invalid UUID", os.id));
        }
        if os.part_id.is_empty() || !self.content.parts.contains_key(&os.part_id) {
            return Status::error(
                "MISSING_PART",
                format!("{}: owner part {} not found", os.id, os.part_id),
            );
        }
        for (other_id, other) in &self.content.offscreens {
            if other_id != &os.id && other.part_id == os.part_id {
                return Status::error(
                    "DUPLICATE_OWNER",
                    format!(
                        "{}: part {} already owned by offscreen {}",
                        os.id, os.part_id, other_id
                    ),
                );
            }
        }
        for mask_id in &os.masks {
            if !self.content.meshes.contains_key(mask_id) {
                return Status::error(
                    "MISSING_MESH",
                    format!("{}: mask mesh {} not found", os.id, mask_id),
                );
            }
        }
        for kf in &os.keyforms {
            if !kf.opacity.is_finite() {
                return Status::error("INVALID_OPACITY", format!("{}: non-finite opacity", os.id));
            }
            if let Some(m) = kf.multiply {
                for c in m {
                    if !c.is_finite() {
                        return Status::error(
                            "INVALID_COLOR",
                            format!("{}: non-finite color", os.id),
                        );
                    }
                }
            }
            if let Some(s) = kf.screen {
                for c in s {
                    if !c.is_finite() {
                        return Status::error(
                            "INVALID_COLOR",
                            format!("{}: non-finite color", os.id),
                        );
                    }
                }
            }
        }
        if !os.part_keyform_indices.is_empty() && os.part_keyform_indices.len() != klen {
            return Status::error(
                "INVALID_LENGTH",
                format!(
                    "{}: part_keyform_indices len ({}) != part keyforms len ({})",
                    os.id,
                    os.part_keyform_indices.len(),
                    klen
                ),
            );
        }
        for &idx in &os.part_keyform_indices {
            if idx >= 0 && (idx as usize) >= os.keyforms.len() {
                return Status::error(
                    "INDEX_OUT_OF_BOUNDS",
                    format!(
                        "{}: keyform index {} exceeds offscreen keyforms len {}",
                        os.id,
                        idx,
                        os.keyforms.len()
                    ),
                );
            }
        }
        Status::ok()
    }
}

impl Model<'_> {
    pub fn is_part_ancestor(&self, ancestor_id: &str, part_id: &str) -> bool {
        if ancestor_id == part_id {
            return true;
        }
        let mut cur = part_id;
        while let Some(part) = self.get_part(cur) {
            if part.parent_id.is_empty() {
                break;
            }
            if part.parent_id == ancestor_id {
                return true;
            }
            cur = &part.parent_id;
        }
        false
    }

    pub fn parent_offscreen_for_part(&self, part_id: &str) -> Option<&Offscreen> {
        let mut cur = part_id;
        while let Some(part) = self.get_part(cur) {
            if part.parent_id.is_empty() {
                break;
            }
            if let Some(os) = self.offscreen_for_part(&part.parent_id) {
                return Some(os);
            }
            cur = &part.parent_id;
        }
        None
    }
}
