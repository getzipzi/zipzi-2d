use std::collections::HashMap;
use zipzi_core::{document::*, draw_order::DrawOrderGroup, *};
pub(super) trait EncodeModel {
    fn get_asset(&self, id: &str) -> Option<&ImageAsset>;
    fn asset_order(&self) -> &[String];
    fn get_mesh(&self, id: &str) -> Option<&Mesh>;
    fn mesh_order(&self) -> &[String];
    fn get_part(&self, id: &str) -> Option<&Part>;
    fn part_order(&self) -> &[String];
    fn get_transform(&self, id: &str) -> Option<&Transform>;
    fn transform_order(&self) -> &[String];
    fn get_parameter(&self, id: &str) -> Option<&Parameter>;
    fn parameter_order(&self) -> &[String];
    fn get_binding(&self, id: &str) -> Option<&MeshBinding>;
    fn binding_order(&self) -> &[String];
    fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding>;
    fn scene_binding_order(&self) -> &[String];
    fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable>;
    fn blend_key_table_order(&self) -> &[String];
    fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint>;
    fn blend_constraint_order(&self) -> &[String];
    fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding>;
    fn blend_binding_order(&self) -> &[String];
    fn get_glue(&self, id: &str) -> Option<&Glue>;
    fn glue_order(&self) -> &[String];
    fn get_offscreen(&self, id: &str) -> Option<&Offscreen>;
    fn offscreen_order(&self) -> &[String];
    fn get_expression(&self, id: &str) -> Option<&ExpressionAsset>;
    fn expression_order(&self) -> &[String];
    fn get_motion(&self, id: &str) -> Option<&MotionClip>;
    fn motion_order(&self) -> &[String];
    fn id(&self) -> &str;
    fn export_settings(&self) -> ExportSettings;
    fn source_canvas(&self) -> Option<Canvas>;
    fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]>;
    fn display_info(&self) -> &DisplayInfo;
    fn hierarchy_order(&self) -> &HierarchyOrder;
    fn editor_state(&self) -> &EditorState;
    fn motion_groups(&self) -> &[MotionGroup];
    fn motion_roots(&self) -> &[MotionRegistration];
    fn pose(&self) -> Option<&PoseAsset>;
    fn physics(&self) -> Option<&PhysicsAsset>;
    fn missing_attachments(&self) -> &[String];
    fn model3_settings(&self) -> &Model3Settings;
    fn package_attachments(&self) -> &[PackageAttachment];
}
pub(super) struct StateProjection<'a> {
    pub state: &'a state::DocumentState,
    pub images: HashMap<String, ImageAsset>,
}
impl EncodeModel for StateProjection<'_> {
    fn get_asset(&self, id: &str) -> Option<&ImageAsset> {
        self.images.get(id)
    }
    fn asset_order(&self) -> &[String] {
        self.state.asset_order()
    }
    fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.state.mesh(id)
    }
    fn mesh_order(&self) -> &[String] {
        self.state.mesh_order()
    }
    fn get_part(&self, id: &str) -> Option<&Part> {
        self.state.part(id)
    }
    fn part_order(&self) -> &[String] {
        self.state.part_order()
    }
    fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.state.transform(id)
    }
    fn transform_order(&self) -> &[String] {
        self.state.transform_order()
    }
    fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.state.parameter(id)
    }
    fn parameter_order(&self) -> &[String] {
        self.state.parameter_order()
    }
    fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.state.binding(id)
    }
    fn binding_order(&self) -> &[String] {
        self.state.binding_order()
    }
    fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.state.scene_binding(id)
    }
    fn scene_binding_order(&self) -> &[String] {
        self.state.scene_binding_order()
    }
    fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.state.blend_key_table(id)
    }
    fn blend_key_table_order(&self) -> &[String] {
        self.state.blend_key_table_order()
    }
    fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.state.blend_constraint(id)
    }
    fn blend_constraint_order(&self) -> &[String] {
        self.state.blend_constraint_order()
    }
    fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.state.blend_binding(id)
    }
    fn blend_binding_order(&self) -> &[String] {
        self.state.blend_binding_order()
    }
    fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.state.glue(id)
    }
    fn glue_order(&self) -> &[String] {
        self.state.glue_order()
    }
    fn get_offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.state.offscreen(id)
    }
    fn offscreen_order(&self) -> &[String] {
        self.state.offscreen_order()
    }
    fn get_expression(&self, id: &str) -> Option<&ExpressionAsset> {
        self.state.expression(id)
    }
    fn expression_order(&self) -> &[String] {
        self.state.expression_order()
    }
    fn get_motion(&self, id: &str) -> Option<&MotionClip> {
        self.state.motion(id)
    }
    fn motion_order(&self) -> &[String] {
        self.state.motion_order()
    }
    fn id(&self) -> &str {
        self.state.id()
    }
    fn export_settings(&self) -> ExportSettings {
        self.state.setup().export
    }
    fn source_canvas(&self) -> Option<Canvas> {
        self.state.setup().source_canvas
    }
    fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.state.draw_order_groups().as_deref()
    }
    fn display_info(&self) -> &DisplayInfo {
        self.state.display_info()
    }
    fn hierarchy_order(&self) -> &HierarchyOrder {
        self.state.hierarchy_order()
    }
    fn editor_state(&self) -> &EditorState {
        self.state.editor_state()
    }
    fn motion_groups(&self) -> &[MotionGroup] {
        self.state.motion_groups()
    }
    fn motion_roots(&self) -> &[MotionRegistration] {
        self.state.motion_roots()
    }
    fn pose(&self) -> Option<&PoseAsset> {
        self.state.pose().as_deref()
    }
    fn physics(&self) -> Option<&PhysicsAsset> {
        self.state.physics().as_deref()
    }
    fn missing_attachments(&self) -> &[String] {
        self.state.missing_attachments()
    }
    fn model3_settings(&self) -> &Model3Settings {
        self.state.model3_settings()
    }
    fn package_attachments(&self) -> &[PackageAttachment] {
        self.state.package_attachments()
    }
}
