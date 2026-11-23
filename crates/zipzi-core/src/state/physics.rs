use super::*;
use crate::model_validation::physics_authoring::PhysicsChange;

impl DocumentState {
    /// A live preview can keep its existing evaluation state when only physics groups
    /// were enabled or disabled. Use shared playback inputs to reject other changes.
    pub fn can_retarget_physics_enablement(&self, previous: &Self) -> bool {
        let (Some(new), Some(old)) = (self.physics(), previous.physics()) else {
            return false;
        };
        let a = &self.content;
        let b = &previous.content;
        !Arc::ptr_eq(new, old)
            && self.frame_input_id == previous.frame_input_id
            && a.setup == b.setup
            && a.expressions.shares_values(&b.expressions)
            && a.expressions.shares_order(&b.expressions)
            && a.motions.shares_values(&b.motions)
            && a.motions.shares_order(&b.motions)
            && Arc::ptr_eq(&a.motion_groups, &b.motion_groups)
            && Arc::ptr_eq(&a.motion_roots, &b.motion_roots)
            && Arc::ptr_eq(&a.pose, &b.pose)
            && Arc::ptr_eq(&a.editor_state, &b.editor_state)
            && Arc::ptr_eq(&a.model3_settings, &b.model3_settings)
            && new.id == old.id
            && new.data.version == old.data.version
            && new.data.meta == old.data.meta
            && new.data.extensions == old.data.extensions
            && new.data.settings.len() == old.data.settings.len()
            && new
                .data
                .settings
                .iter()
                .zip(&old.data.settings)
                .all(|(a, b)| {
                    Arc::ptr_eq(a, b)
                        || (a.id == b.id
                            && a.external_id == b.external_id
                            && a.inputs == b.inputs
                            && a.outputs == b.outputs
                            && a.vertices == b.vertices
                            && a.normalization == b.normalization
                            && a.extensions == b.extensions)
                })
    }
}

impl DocumentBuilder {
    pub fn replace_physics(&mut self, mut asset: Option<PhysicsAsset>) -> Result<(), Status> {
        self.write(|this| {
            if let Some(asset) = &mut asset {
                asset.reconcile_counts();
                let model = this.content.model();
                model.validate_physics_replacement_unlocked(asset)?;
                let status = model.validate_physics(asset, false);
                if !status.is_ok() {
                    return Err(status);
                }
            } else if let Some(old) = this.physics().as_ref() {
                for id in old.entity_ids() {
                    if this.editor_state().get(id).locked {
                        return Err(Status::error("OBJECT_LOCKED", id));
                    }
                }
            }
            let retained: std::collections::HashSet<_> =
                asset.iter().flat_map(|a| a.entity_ids()).collect();
            let removed: Vec<_> = this
                .physics()
                .iter()
                .flat_map(|a| a.entity_ids())
                .filter(|id| !retained.contains(id))
                .map(str::to_owned)
                .collect();
            this.clear_owned_editor_objects(&removed);
            this.set_physics(asset.map(Arc::new))
        })
    }
    pub fn patch_physics(&mut self, patch: &PhysicsPatch) -> Result<(), Status> {
        let validation = self.validation;
        let result = self.write(|this| {
            let prepared = this.content.model().prepare_physics_patch(patch)?;
            if !prepared.changed() {
                return Ok(());
            }
            let mut asset = this.physics().as_ref().unwrap().as_ref().clone();
            prepared.apply(&mut asset, patch);
            this.set_physics(Some(Arc::new(asset)))
        });
        if result.is_ok() {
            // Typed patches preserve entity identities and validate the changed
            // rig and metadata. Other builder writes clear this proof as usual.
            self.validation = validation;
        }
        result
    }
    pub fn edit_physics(&mut self, edit: PhysicsEdit) -> Result<(), Status> {
        self.write(|this| {
            let changes = this.content.model().physics_changes(edit)?;
            match changes.change {
                PhysicsChange::Patch(patch) => this.patch_physics(&patch),
                PhysicsChange::Replace(asset) => this.replace_physics(asset),
            }
        })
    }
}
