use super::*;
use zipzi_core::document::ExpressionAsset;

impl Commands<'_> {
    pub fn create_expression(&mut self, asset: ExpressionAsset) -> Result<(), Status> {
        self.run(|this| this.builder.create_expression(asset))
    }
    pub fn replace_expression(&mut self, asset: ExpressionAsset) -> Result<(), Status> {
        self.run(|this| this.builder.replace_expression(asset))
    }
    pub fn edit_expression(&mut self, edit: ExpressionEdit) -> Result<(), Status> {
        self.run(|this| this.builder.edit_expression(edit))
    }
}
