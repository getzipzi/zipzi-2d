use super::*;
use crate::model_validation::{AssetIds, Model, ModelContent, RecordView};
impl Document {
    pub(crate) fn validation_model(&self) -> Model<'_> {
        Model::new(ModelContent {
            id: &self.content.id,
            assets: AssetIds(&self.content.asset_order),
            parameters: RecordView::Owned(&self.content.parameters),
            parts: RecordView::Owned(&self.content.parts),
            transforms: RecordView::Shared(self.content.transforms.shared_map()),
            meshes: RecordView::Shared(self.content.meshes.shared_map()),
            mesh_order: &self.content.mesh_order,
            bindings: RecordView::Shared(self.content.bindings.shared_map()),
            binding_order: &self.content.binding_order,
            scene_bindings: RecordView::Shared(self.content.scene_bindings.shared_map()),
            scene_binding_order: &self.content.scene_binding_order,
            blend_key_tables: RecordView::Owned(&self.content.blend_key_tables),
            blend_key_table_order: &self.content.blend_key_table_order,
            blend_constraints: RecordView::Owned(&self.content.blend_constraints),
            blend_constraint_order: &self.content.blend_constraint_order,
            blend_bindings: RecordView::Shared(self.content.blend_bindings.shared_map()),
            glues: RecordView::Owned(&self.content.glues),
            glue_order: &self.content.glue_order,
            offscreens: RecordView::Owned(&self.content.offscreens),
            expressions: RecordView::Owned(&self.content.expressions),
            motions: RecordView::Shared(&self.content.motions),
            display_info: &self.content.display_info,
            motion_groups: &self.content.motion_groups,
            motion_roots: &self.content.motion_roots,
            pose: &self.content.pose,
            physics: &self.content.physics,
            model3_settings: &self.content.model3_settings,
            package_attachments: &self.content.package_attachments,
        })
    }
}
