//! Adapter dispatch for bulk modeling. Domain rules stay in the SDK.
pub use zipzi_contract::runtime::ModelingCommand;

pub trait ModelingCommandApply {
    fn apply(self, edit: &mut zipzi_sdk::EditSession<'_>) -> Result<(), zipzi_sdk::SdkError>;
}
impl ModelingCommandApply for ModelingCommand {
    fn apply(self, edit: &mut zipzi_sdk::EditSession<'_>) -> Result<(), zipzi_sdk::SdkError> {
        match self {
            Self::CreateAsset { asset } => edit.create_asset(asset),
            Self::ReplaceAsset { asset } => edit.replace_asset(asset),
            Self::CreateMesh { mesh } => edit.create_mesh(mesh),
            Self::ReplaceMesh { mesh } => edit.replace_mesh(mesh),
            Self::CreateParameter { parameter } => edit.create_parameter(parameter),
            Self::ReplaceParameter { parameter } => edit.replace_parameter(parameter),
            Self::CreatePart { part } => edit.create_part(part),
            Self::ReplacePart { part } => edit.replace_part(part),
            Self::CreateTransform { transform } => edit.create_transform(transform),
            Self::ReplaceTransform { transform } => edit.replace_transform(transform),
            Self::CreateBinding { binding } => edit.create_binding(binding),
            Self::ReplaceBinding { binding } => edit.replace_binding(binding),
            Self::CreateSceneBinding { binding } => edit.create_scene_binding(binding),
            Self::ReplaceSceneBinding { binding } => edit.replace_scene_binding(binding),
            Self::CreateBlendKeyTable { table } => edit.create_blend_key_table(table),
            Self::ReplaceBlendKeyTable { table } => edit.replace_blend_key_table(table),
            Self::CreateBlendConstraint { constraint } => edit.create_blend_constraint(constraint),
            Self::ReplaceBlendConstraint { constraint } => {
                edit.replace_blend_constraint(constraint)
            }
            Self::CreateBlendBinding { binding } => edit.create_blend_binding(binding),
            Self::ReplaceBlendBinding { binding } => edit.replace_blend_binding(binding),
            Self::CreateGlue { glue } => edit.create_glue(glue),
            Self::ReplaceGlue { glue } => edit.replace_glue(glue),
            Self::CreateOffscreen { offscreen } => edit.create_offscreen(offscreen),
            Self::ReplaceOffscreen { offscreen } => edit.replace_offscreen(offscreen),
            Self::Deletion { .. } => Err(zipzi_sdk::SdkError {
                code: "DELETION_PLAN_REQUIRED".into(),
                message: "Prepare and review a deletion plan before applying".into(),
                operation: "deletion",
                object_ids: vec![],
                field_path: None,
                expected_version: None,
                actual_version: None,
                referrers: Box::new([]),
            }),
            Self::Scene { intent } => edit.edit_scene(intent),
            Self::Organization { intent } => edit.edit_organization(intent),
            Self::Binding { intent } => edit.binding_authoring(intent),
            Self::Content { intent } => edit.edit_scene_content(intent),
            Self::References { intent } => edit.edit_references(intent),
            Self::Resource { intent } => edit.edit_resource(intent),
            Self::Document { intent } => edit.edit_document(intent),
            Self::Attachment { intent } => edit.edit_attachment(intent),
            Self::Motion { intent } => edit.edit_motion(intent),
            Self::Expression { intent } => edit.edit_expression(intent),
            Self::Physics { intent } => edit.edit_physics(intent),
            Self::Pose { intent } => edit.edit_pose(intent),
            Self::CreateMotion { clip } => edit.create_motion(clip),
            Self::ReplaceMotion { clip } => edit.replace_motion(clip),
            Self::CreateExpression { expression } => edit.create_expression(expression),
            Self::ReplaceExpression { expression } => edit.replace_expression(expression),
            Self::SetPhysics { physics } => edit.set_physics(physics),
            Self::SetPose { pose } => edit.set_pose(pose),
            Self::ReplaceModelSetup { setup } => edit.replace_model_setup(setup),
            Self::ReplaceHierarchyOrder { order } => edit.replace_hierarchy_order(order),
            Self::ReplaceEditorState { state } => edit.replace_editor_state(state),
            Self::ReplaceDrawOrderGroups { groups } => edit.replace_draw_order_groups(groups),
            Self::EraseObject { id } => edit.erase_object(&id),
            Self::RenameMesh { id, name } => edit.rename_mesh(&id, name),
            Self::UpdatePositions {
                id,
                vertex_ids,
                positions,
            } => edit.update_positions(&id, &vertex_ids, &positions),
            Self::SetMeshKeyform { id, keyform } => edit.set_mesh_keyform(&id, keyform),
            Self::SetSceneKeyform { id, keyform } => edit.set_scene_keyform(&id, keyform),
            Self::RemeshRectangleGrid { id, columns, rows } => {
                edit.remesh_rectangle_grid(&id, columns, rows)
            }
            Self::ReplaceTopology {
                source,
                replacement,
            } => edit.replace_topology(&source, replacement),
            Self::ManualMesh {
                mesh_id, operation, ..
            } => edit.edit_manual_mesh(&mesh_id, operation).map(|_| ()),
        }
    }
}
