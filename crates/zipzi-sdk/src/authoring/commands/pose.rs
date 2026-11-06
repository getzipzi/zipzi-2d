use super::*;
use zipzi_core::document::PoseAsset;
impl Commands<'_> {
    pub fn replace_pose(&mut self, asset: Option<PoseAsset>) -> Result<(), Status> {
        self.run(|this| this.builder.replace_pose(asset))
    }
    pub fn edit_pose(&mut self, edit: PoseEdit) -> Result<(), Status> {
        self.run(|this| this.builder.edit_pose(edit))
    }
}
