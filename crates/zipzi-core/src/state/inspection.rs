//! Immutable native inspection projections; identities remain stable domain IDs.
use super::*;
impl DocumentState {
    pub fn parameter_group_for(&self, id: &str) -> Option<&str> {
        self.display_info().parameters.iter().flatten()
            .find(|entry| matches!(entry, CdiParameterEntry::Resolved { parameter_id, .. } if parameter_id == id))
            .and_then(|entry| entry.group_id())
    }
    pub fn motion_group(&self, id: &str) -> Option<&MotionGroup> {
        self.motion_groups().iter().find(|g| g.id == id)
    }
    pub fn motion_track(&self, id: &str) -> Option<(&MotionClip, &MotionTrack)> {
        self.motion_order()
            .iter()
            .filter_map(|id| self.motion(id))
            .find_map(|clip| {
                clip.tracks
                    .iter()
                    .find(|track| track.id == id)
                    .map(|track| (clip, track))
            })
    }
    pub fn motion_key(&self, id: &str) -> Option<(&MotionClip, &MotionTrack, &MotionKey)> {
        self.motion_order()
            .iter()
            .filter_map(|id| self.motion(id))
            .find_map(|clip| {
                clip.tracks.iter().find_map(|track| {
                    track
                        .keys()
                        .find(|key| key.id == id)
                        .map(|key| (clip, track, key))
                })
            })
    }
    pub fn parameter_group(&self, id: &str) -> Option<&CdiParameterGroup> {
        self.display_info()
            .parameter_groups
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn pose_group(&self, id: &str) -> Option<&PoseGroup> {
        self.pose().as_deref()?.groups.iter().find(|g| g.id == id)
    }
    pub fn pose_entry(&self, id: &str) -> Option<(&PoseGroup, &PoseEntry)> {
        self.pose()
            .as_deref()?
            .groups
            .iter()
            .find_map(|g| g.entries.iter().find(|e| e.id == id).map(|e| (g, e)))
    }
    pub fn pose_link(&self, id: &str) -> Option<(&PoseEntry, &PoseLink)> {
        self.pose()
            .as_deref()?
            .groups
            .iter()
            .flat_map(|g| &g.entries)
            .find_map(|e| {
                e.links
                    .iter()
                    .flatten()
                    .find(|l| l.id == id)
                    .map(|l| (e, l))
            })
    }
    pub fn model_parameter_group(&self, id: &str) -> Option<&ModelParameterGroup> {
        self.model3_settings()
            .groups
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn model_hit_area(&self, id: &str) -> Option<&ModelHitArea> {
        self.model3_settings()
            .hit_areas
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn physics_group(&self, id: &str) -> Option<&PhysicsSetting> {
        self.physics()
            .as_deref()?
            .data
            .settings
            .iter()
            .find(|group| group.id == id)
            .map(AsRef::as_ref)
    }
    pub fn physics_input(&self, id: &str) -> Option<(&PhysicsSetting, &PhysicsInput)> {
        self.physics()
            .as_deref()?
            .data
            .settings
            .iter()
            .find_map(|group| {
                group
                    .inputs
                    .iter()
                    .find(|input| input.id == id)
                    .map(|input| (group.as_ref(), input))
            })
    }
    pub fn physics_output(&self, id: &str) -> Option<(&PhysicsSetting, &PhysicsOutput)> {
        self.physics()
            .as_deref()?
            .data
            .settings
            .iter()
            .find_map(|group| {
                group
                    .outputs
                    .iter()
                    .find(|output| output.id == id)
                    .map(|output| (group.as_ref(), output))
            })
    }
    pub fn organization_owner(&self, id: &str) -> Option<&str> {
        let model = self.content.model();
        let owner = model.organization_owner(id)?;
        self.part(owner)
            .map(|part| part.id.as_str())
            .or_else(|| self.parameter_group(owner).map(|group| group.id.as_str()))
    }
    pub fn scene_deformer_parent(&self, id: &str) -> Option<&str> {
        self.mesh(id)
            .map(|mesh| mesh.deformer_id.as_str())
            .or_else(|| self.transform(id).map(Transform::parent))
    }
    pub fn validate_offscreen(&self, offscreen: &Offscreen) -> Status {
        self.content.model().validate_offscreen(offscreen)
    }
    pub fn validate_physics_patch(&self, patch: &PhysicsPatch) -> Status {
        self.content.model().validate_physics_patch(patch)
    }
    pub fn organization_subtree_unlocked(&self, ids: &[String]) -> Result<(), Status> {
        self.content.model().organization_subtree_unlocked(ids)
    }
    pub fn binding_identities(&self) -> Vec<crate::document::BindingIdentity> {
        self.content.model().binding_identities().to_vec()
    }
    pub fn reference_identity(&self, id: &str) -> Option<crate::document::ReferenceIdentity> {
        self.content.model().reference_identity(id).cloned()
    }
    pub fn blend_binding_active(&self, binding: &BlendShapeBinding) -> bool {
        self.content.model().blend_binding_active(binding)
    }
    pub fn blend_bindings_for_target<'a>(
        &'a self,
        id: &str,
    ) -> impl Iterator<Item = &'a BlendShapeBinding> {
        let id = id.to_owned();
        self.blend_binding_order()
            .iter()
            .filter_map(|key| self.blend_binding(key))
            .filter(move |binding| binding.target_id == id)
    }
    pub fn glues_for_mesh<'a>(&'a self, id: &str) -> Vec<&'a Glue> {
        self.glue_order()
            .iter()
            .filter_map(|key| self.glue(key))
            .filter(|glue| glue.mesh_a_id == id || glue.mesh_b_id == id)
            .collect()
    }
}

impl DocumentState {
    pub fn glue_pair(&self, id: &str) -> Option<&crate::GlueVertexPair> {
        self.glue_order()
            .iter()
            .filter_map(|owner| self.glue(owner))
            .flat_map(|g| &g.pairs)
            .find(|pair| pair.id == id)
    }
    pub fn glue_pair_owner(&self, id: &str) -> Option<&str> {
        self.glue_order()
            .iter()
            .filter_map(|owner| self.glue(owner))
            .find(|g| g.pairs.iter().any(|pair| pair.id == id))
            .map(|g| g.id.as_str())
    }
}
