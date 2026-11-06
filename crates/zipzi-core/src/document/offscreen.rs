use super::*;
use crate::types::{ChangeKind, EditResult, Offscreen, Status};

impl Document {
    pub fn offscreen_order(&self) -> &[String] {
        &self.content.offscreen_order
    }

    pub fn offscreen_count(&self) -> usize {
        self.content.offscreens.len()
    }

    pub fn get_offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.content.offscreens.get(id)
    }

    pub fn offscreen_for_part(&self, part_id: &str) -> Option<&Offscreen> {
        self.content
            .offscreens
            .values()
            .find(|os| os.part_id == part_id)
    }

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

    pub fn validate_offscreen(&self, os: &Offscreen) -> Status {
        self.validation_model().validate_offscreen(os)
    }
    pub(in crate::document) fn validate_offscreen_key_count(
        &self,
        os: &Offscreen,
        klen: usize,
    ) -> Status {
        self.validation_model()
            .validate_offscreen_key_count(os, klen)
    }

    pub fn create_offscreen(&mut self, offscreen: Offscreen) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&offscreen.part_id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &offscreen.id));
        }
        if self.contains_id(&offscreen.id) {
            return self.failed(Status::error("DUPLICATE_ID", &offscreen.id));
        }
        for (other_id, other) in &self.content.offscreens {
            if other.runtime_id == offscreen.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", offscreen.id, other_id),
                ));
            }
        }
        let s = self.validate_offscreen(&offscreen);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = offscreen.id.clone();
        let part_id = offscreen.part_id.clone();
        self.content.offscreens.insert(id.clone(), offscreen);
        self.content.offscreen_order.push(id.clone());
        self.changed(ChangeKind::Structure, Vec::new(), vec![id, part_id])
    }

    pub fn replace_offscreen(&mut self, offscreen: Offscreen) -> EditResult {
        for id in [&offscreen.id, &offscreen.part_id] {
            if let Err(status) = self.authoring_unlocked(id) {
                return self.failed(status);
            }
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &offscreen.id));
        }
        if !self.content.offscreens.contains_key(&offscreen.id) {
            return self.failed(Status::error("MISSING_OBJECT", &offscreen.id));
        }
        for (other_id, other) in &self.content.offscreens {
            if other_id != &offscreen.id && other.runtime_id == offscreen.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", offscreen.id, other_id),
                ));
            }
        }
        let s = self.validate_offscreen(&offscreen);
        if !s.is_ok() {
            return self.failed(s);
        }
        if self.get_offscreen(&offscreen.id).is_some_and(|old| {
            old.part_id != offscreen.part_id
                && (self.binding_for_scene(&old.part_id).is_some()
                    || self.binding_for_scene(&offscreen.part_id).is_some())
        }) {
            return self.failed(Status::error(
                "OFFSCREEN_REBIND_REQUIRES_CONVERSION",
                &offscreen.id,
            ));
        }
        let id = offscreen.id.clone();
        let part_id = offscreen.part_id.clone();
        self.content.offscreens.insert(id.clone(), offscreen);
        self.changed(ChangeKind::Structure, Vec::new(), vec![id, part_id])
    }
}
