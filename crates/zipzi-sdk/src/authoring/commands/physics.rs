use super::*;
use zipzi_core::document::PhysicsAsset;
impl Commands<'_> {
    pub fn replace_physics(&mut self, asset: Option<PhysicsAsset>) -> Result<(), Status> {
        self.run(|this| this.builder.replace_physics(asset))
    }
    pub fn edit_physics(&mut self, edit: PhysicsEdit) -> Result<(), Status> {
        self.run(|this| this.builder.edit_physics(edit))
    }
    pub fn patch_physics(&mut self, patch: &PhysicsPatch) -> Result<(), Status> {
        self.run(|this| this.builder.patch_physics(patch))
    }
}
