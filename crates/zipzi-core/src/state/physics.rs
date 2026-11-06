use super::*;
use crate::model_validation::physics_authoring::PhysicsChange;
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
        self.write(|this| {
            let prepared = this.content.model().prepare_physics_patch(patch)?;
            if !prepared.changed() {
                return Ok(());
            }
            let mut asset = this.physics().as_ref().unwrap().as_ref().clone();
            prepared.apply(&mut asset, patch);
            this.set_physics(Some(Arc::new(asset)))
        })
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
