//! Whole-asset candidates share the inspector lifecycle and SDK undo history.
use super::*;
use zipzi_core::document::PhysicsAsset;

impl Document {
    pub(crate) fn validate_physics_edit(&self, asset: &PhysicsAsset, revision: u64) -> Result<()> {
        ensure!(
            self.model_generation() == revision,
            "Physics document changed"
        );
        if let Some(current) = self.session.document().physics() {
            ensure!(current.id == asset.id, "Physics asset changed");
            self.ensure_editable(&[crate::ObjectRef::new(
                crate::ObjectKind::Physics,
                &current.id,
            )])?;
        }
        let mut edit = self.session.begin_owned_edit("Validate physics", None)?;
        edit.set_physics(asset.clone())?;
        Ok(())
    }

    pub(crate) fn replace_physics(&mut self, asset: PhysicsAsset, revision: u64) -> Result<()> {
        self.validate_physics_edit(&asset, revision)?;
        self.finish_edit()?;
        let mut edit = self.session.begin_owned_edit("Edit physics", None)?;
        edit.set_physics(asset)?;
        let receipt = edit.commit_to(&mut self.session)?;
        if receipt.changed {
            self.pristine_blank = None;
        }
        self.consume_authoring_receipt(&receipt, false);
        Ok(())
    }
}
