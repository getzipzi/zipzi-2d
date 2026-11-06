use super::*;
use crate::types::{
    ChangeKind, EditResult, MeshBinding, MeshKeyform, Offscreen, SceneBinding, SceneKeyform,
    SceneTrack, Status,
};
use zipzi_core::SceneTrackEvaluation;

impl Document {
    pub fn binding_order(&self) -> &[String] {
        &self.content.binding_order
    }

    pub fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.content.bindings.get(id)
    }

    pub fn binding_for_mesh(&self, mesh_id: &str) -> Option<&MeshBinding> {
        self.lookup()
            .mesh_bindings
            .get(mesh_id)
            .and_then(|id| self.content.bindings.get(id))
    }

    pub(super) fn canonicalize_binding(&self, b: &mut MeshBinding) -> Status {
        self.validation_model().canonicalize_binding(b)
    }

    pub fn create_binding(&mut self, mut b: MeshBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&b.mesh_id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let mesh = b.mesh_id.clone();
        self.content.bindings.insert(id.clone(), b);
        self.content.binding_order.push(id.clone());
        self.changed(ChangeKind::Structure, vec![mesh], vec![id])
    }

    pub fn replace_binding(&mut self, mut b: MeshBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&b.mesh_id) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&b.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let previous_mesh = match self.content.bindings.get(&b.id) {
            Some(old) => old.mesh_id.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", &b.id)),
        };
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let mesh = b.mesh_id.clone();
        let mut affected = vec![mesh.clone()];
        if previous_mesh != mesh {
            affected.push(previous_mesh.clone());
        }
        let tracked =
            super::tracking::binding_payload_is_tracked(self.get_binding(&id).unwrap(), &b)
                .then(|| self.track_binding_payload_write(&id));
        self.content.bindings.insert(id.clone(), b);
        if let Some(write) = tracked {
            self.changed_tracked(
                write,
                ChangeKind::Structure,
                affected,
                vec![id, mesh, previous_mesh],
            )
        } else {
            self.changed(
                ChangeKind::Structure,
                affected,
                vec![id, mesh, previous_mesh],
            )
        }
    }

    pub fn set_mesh_keyform(&mut self, id: &str, mut form: MeshKeyform) -> EditResult {
        if let Err(status) = self.authoring_unlocked(id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let mut b = match self.get_binding(id) {
            Some(old) => old.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", id)),
        };
        if let Err(s) = super::binding_authoring::identity::coordinates(
            &b.axes,
            &mut form.keys,
            &mut form.samples,
        ) {
            return self.failed(s);
        }
        let it = b.keyforms.iter_mut().find(|f| f.samples == form.samples);
        let changes_order = match it {
            Some(f) => {
                let changes_order = f.draw_order != form.draw_order;
                *f = form;
                changes_order
            }
            None => return self.failed(Status::error("INVALID_KEY_COMBINATION", id)),
        };
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let mesh = b.mesh_id.clone();
        let tracked =
            super::tracking::binding_payload_is_tracked(self.get_binding(id).unwrap(), &b)
                .then(|| self.track_binding_payload_write(id));
        self.content.bindings.insert(id.to_string(), b);
        let kind = if changes_order {
            ChangeKind::Structure
        } else {
            ChangeKind::Positions
        };
        if let Some(write) = tracked {
            self.changed_tracked(write, kind, vec![mesh.clone()], vec![id.to_string(), mesh])
        } else {
            self.changed(kind, vec![mesh.clone()], vec![id.to_string(), mesh])
        }
    }

    pub fn scene_binding_order(&self) -> &[String] {
        &self.content.scene_binding_order
    }

    pub fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.content.scene_bindings.get(id)
    }

    pub fn binding_for_scene(&self, target_id: &str) -> Option<&SceneBinding> {
        self.lookup()
            .scene_bindings
            .get(target_id)
            .and_then(|id| self.content.scene_bindings.get(id))
    }

    pub(super) fn canonicalize_scene_binding(&self, b: &mut SceneBinding) -> Status {
        self.validation_model().canonicalize_scene_binding(b)
    }

    pub fn create_scene_binding(&mut self, mut b: SceneBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(b.target_id()) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.canonicalize_scene_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id().to_owned();
        self.content.scene_bindings.insert(id.clone(), b);
        self.content.scene_binding_order.push(id.clone());
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target])
    }

    pub fn replace_scene_binding(&mut self, mut b: SceneBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(b.target_id()) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&b.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        let previous = match self.content.scene_bindings.get(&b.id) {
            Some(old) => old.target_id().to_owned(),
            None => return self.failed(Status::error("MISSING_BINDING", &b.id)),
        };
        let s = self.canonicalize_scene_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id().to_owned();
        self.content.scene_bindings.insert(id.clone(), b);
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target, previous])
    }

    /// Atomically replace an existing Part binding and its Offscreen appearance.
    /// The caller supplies every new keyform and mapping explicitly; no implicit
    /// interpolation or slot copying takes place. Mapping slots use canonical axis order.
    pub fn replace_part_binding_with_offscreen(
        &mut self,
        binding: SceneBinding,
        offscreen: Offscreen,
    ) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&binding.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &binding.id));
        }
        let Some(old_binding) = self.get_scene_binding(&binding.id) else {
            return self.failed(Status::error("MISSING_BINDING", &binding.id));
        };
        let Some(old_offscreen) = self.get_offscreen(&offscreen.id) else {
            return self.failed(Status::error("MISSING_OBJECT", &offscreen.id));
        };
        if binding.target_id() != old_binding.target_id()
            || offscreen.part_id != old_offscreen.part_id
            || binding.target_id() != offscreen.part_id
        {
            return self.failed(Status::error(
                "INVALID_BINDING",
                "Preserve the Part and Offscreen owner",
            ));
        }
        let objects = vec![
            binding.id.clone(),
            binding.target_id().to_owned(),
            offscreen.id.clone(),
        ];
        let mut candidate = self.clone();
        candidate
            .content
            .offscreens
            .insert(offscreen.id.clone(), offscreen.clone());
        let edit = candidate.replace_scene_binding(binding);
        if !edit.status.is_ok() {
            return self.failed(edit.status);
        }
        let edit = candidate.replace_offscreen(offscreen);
        if !edit.status.is_ok() {
            return self.failed(edit.status);
        }
        self.content.scene_bindings = candidate.content.scene_bindings;
        self.content.offscreens = candidate.content.offscreens;
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            objects,
        )
    }

    pub fn set_scene_keyform(&mut self, id: &str, mut f: SceneKeyform) -> EditResult {
        if let Err(status) = self.authoring_unlocked(id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", id));
        }
        let mut b = match self.get_scene_binding(id) {
            Some(old) => old.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", id)),
        };
        let (keys, samples) = match &mut f {
            SceneKeyform::Warp(f) => (&mut f.keys, &mut f.samples),
            SceneKeyform::Rotation(f) => (&mut f.keys, &mut f.samples),
            SceneKeyform::Part(f) => (&mut f.keys, &mut f.samples),
        };
        if let Err(s) = super::binding_authoring::identity::coordinates(&b.axes, keys, samples) {
            return self.failed(s);
        }
        let status = b.track.replace(f);
        if !status.is_ok() {
            return self.failed(status);
        }
        let status = self.canonicalize_scene_binding(&mut b);
        if !status.is_ok() {
            return self.failed(status);
        }
        // Replacing an existing pose preserves target, axes and key coordinates.
        // Part keyforms can change render order and still require preparation.
        let kind = if matches!(b.track, SceneTrack::Part { .. }) {
            ChangeKind::Structure
        } else {
            ChangeKind::Positions
        };
        let target = b.target_id().to_owned();
        self.content.scene_bindings.insert(id.to_owned(), b);
        self.changed(
            kind,
            self.content.mesh_order.clone(),
            vec![id.to_owned(), target],
        )
    }
}
