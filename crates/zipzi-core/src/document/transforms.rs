use super::*;
use crate::types::{ChangeKind, EditResult, Status, Transform};
use std::collections::HashSet;
use zipzi_core::SceneTrackEvaluation;

impl Document {
    pub fn transform_order(&self) -> &[String] {
        &self.content.transform_order
    }

    pub fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.content.transforms.get(id)
    }

    pub(super) fn validate_transform(&self, t: &Transform) -> Status {
        self.validation_model().validate_transform(t)
    }

    pub fn create_transform(&mut self, t: Transform) -> EditResult {
        self.create_transform_with_original(t, true)
    }

    /// Insert imported data without inventing a creation or first-key baseline.
    pub fn create_imported_transform(&mut self, t: Transform) -> EditResult {
        self.create_transform_with_original(t, false)
    }

    fn create_transform_with_original(&mut self, mut t: Transform, capture: bool) -> EditResult {
        if let Err(status) = self.authoring_unlocked(t.part()) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &t.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &t.id));
        }
        if self.contains_id(&t.id) {
            return self.failed(Status::error("DUPLICATE_ID", &t.id));
        }
        if t.runtime_id.is_empty() {
            t.runtime_id = t.id.clone();
        }
        let s = self.validate_transform(&t);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = t.id.clone();
        self.content.transforms.insert(id.clone(), t);
        self.content.transform_order.push(id.clone());
        self.lookup.take();
        self.prepared.take();
        if capture {
            self.initialize_deformer_original(&id);
            self.initialize_rotation_bone_display(std::slice::from_ref(&id));
        }
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    pub fn replace_transform(&mut self, t: Transform) -> EditResult {
        if let Err(status) = self
            .authoring_unlocked(&t.id)
            .and_then(|_| self.authoring_unlocked(t.part()))
        {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &t.id));
        }
        let Some(old) = self.content.transforms.get(&t.id) else {
            return self.failed(Status::error("MISSING_TRANSFORM", &t.id));
        };
        if (self.binding_for_scene(&t.id).is_some()
            || !self.blend_bindings_for_target(&t.id).is_empty())
            && (old.kind() != t.kind()
                || old.warp().map(|w| (w.rows, w.columns)) != t.warp().map(|w| (w.rows, w.columns)))
        {
            return self.failed(Status::error(
                "KEYFORMS_REQUIRED",
                "Remove ordinary and blend shape bindings before changing transform grid/type",
            ));
        }
        let s = self.validate_transform(&t);
        if !s.is_ok() {
            return self.failed(s);
        }
        let kind = if pose_only_change(old, &t) {
            ChangeKind::Positions
        } else {
            ChangeKind::Structure
        };
        let id = t.id.clone();
        self.content.transforms.insert(id.clone(), t);
        let meshes = self.content.mesh_order.clone();
        self.changed(kind, meshes, vec![id])
    }

    pub fn sorted_transforms(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        fn visit(
            id: &str,
            transforms: &RecordMap<Transform>,
            seen: &mut HashSet<String>,
            result: &mut Vec<String>,
        ) {
            if id.is_empty() || !seen.insert(id.to_string()) {
                return;
            }
            if let Some(t) = transforms.get(id) {
                visit(t.parent(), transforms, seen, result);
                result.push(id.to_string());
            }
        }

        for id in &self.content.transform_order {
            visit(id, &self.content.transforms, &mut seen, &mut result);
        }
        result
    }
}

impl Document {
    /// Atomically replace a Warp lattice and its existing ordinary keyforms.
    /// Controller edits preserve explicit conversion density; resize plans change it separately.
    pub fn replace_warp_authoring(
        &mut self,
        t: Transform,
        binding: Option<crate::SceneBinding>,
    ) -> EditResult {
        if let Err(status) = self
            .authoring_unlocked(&t.id)
            .and_then(|_| self.authoring_unlocked(t.part()))
        {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &t.id));
        }
        let Some(old) = self.get_transform(&t.id).cloned() else {
            return self.failed(Status::error("MISSING_TRANSFORM", &t.id));
        };
        if old.warp().is_none()
            || t.warp().is_none()
            || (!self.blend_bindings_for_target(&t.id).is_empty()
                && old.warp().map(|w| (w.rows, w.columns, w.quad))
                    != t.warp().map(|w| (w.rows, w.columns, w.quad)))
        {
            return self.failed(Status::error("INVALID_WARP_AUTHORING", &t.id));
        }
        let existing = self.binding_for_scene(&t.id);
        if let Some(b) = existing {
            if let Err(status) = self.authoring_unlocked(&b.id) {
                return self.failed(status);
            }
        }
        if existing.map(|b| b.id.as_str()) != binding.as_ref().map(|b| b.id.as_str())
            || binding.as_ref().is_some_and(|b| {
                b.target_id() != t.id || !matches!(b.track, crate::SceneTrack::Warp { .. })
            })
        {
            return self.failed(Status::error("INVALID_BINDING_TARGET", &t.id));
        }
        let status = self.validate_transform(&t);
        if !status.is_ok() {
            return self.failed(status);
        }
        let mut kind = if pose_only_change(&old, &t) {
            ChangeKind::Positions
        } else {
            ChangeKind::Structure
        };
        let id = t.id.clone();
        self.content.transforms.insert(id.clone(), t);
        let mut objects = vec![id.clone()];
        if let Some(mut b) = binding {
            let status = self.canonicalize_scene_binding(&mut b);
            if !status.is_ok() {
                self.content.transforms.insert(id.clone(), old);
                return self.failed(status);
            }
            let previous = self.get_scene_binding(&b.id).unwrap();
            if previous.axes != b.axes
                || !previous
                    .track
                    .samples()
                    .map(|f| f.keys)
                    .eq(b.track.samples().map(|f| f.keys))
            {
                kind = ChangeKind::Structure;
            }
            objects.push(b.id.clone());
            self.content.scene_bindings.insert(b.id.clone(), b);
        }
        self.changed(kind, self.content.mesh_order.clone(), objects)
    }
}

/// All pose values are read afresh by the evaluator. References and lattice
/// structure still invalidate the prepared graph and dependent geometry.
fn pose_only_change(old: &Transform, new: &Transform) -> bool {
    old.id == new.id
        && old.name == new.name
        && old.runtime_id == new.runtime_id
        && old.parent_id == new.parent_id
        && old.part_id == new.part_id
        && old.enabled == new.enabled
        && old.appearance == new.appearance
        && old.kind() == new.kind()
        && old.warp().map(|w| (w.rows, w.columns, w.quad))
            == new.warp().map(|w| (w.rows, w.columns, w.quad))
}
