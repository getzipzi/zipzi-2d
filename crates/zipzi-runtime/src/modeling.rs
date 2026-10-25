//! Adapter dispatch for bulk modeling. Domain rules stay in the SDK.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelingCommand {
    CreateAsset {
        asset: zipzi_core::ImageAsset,
    },
    ReplaceAsset {
        asset: zipzi_core::ImageAsset,
    },
    CreateMesh {
        mesh: zipzi_core::Mesh,
    },
    ReplaceMesh {
        mesh: zipzi_core::Mesh,
    },
    CreateParameter {
        parameter: zipzi_core::Parameter,
    },
    ReplaceParameter {
        parameter: zipzi_core::Parameter,
    },
    CreatePart {
        part: zipzi_core::Part,
    },
    ReplacePart {
        part: zipzi_core::Part,
    },
    CreateTransform {
        transform: zipzi_core::Transform,
    },
    ReplaceTransform {
        transform: zipzi_core::Transform,
    },
    CreateBinding {
        binding: zipzi_core::MeshBinding,
    },
    ReplaceBinding {
        binding: zipzi_core::MeshBinding,
    },
    CreateSceneBinding {
        binding: zipzi_core::SceneBinding,
    },
    ReplaceSceneBinding {
        binding: zipzi_core::SceneBinding,
    },
    CreateBlendKeyTable {
        table: zipzi_core::BlendShapeKeyTable,
    },
    ReplaceBlendKeyTable {
        table: zipzi_core::BlendShapeKeyTable,
    },
    CreateBlendConstraint {
        constraint: zipzi_core::BlendShapeConstraint,
    },
    ReplaceBlendConstraint {
        constraint: zipzi_core::BlendShapeConstraint,
    },
    CreateBlendBinding {
        binding: zipzi_core::BlendShapeBinding,
    },
    ReplaceBlendBinding {
        binding: zipzi_core::BlendShapeBinding,
    },
    CreateGlue {
        glue: zipzi_core::Glue,
    },
    ReplaceGlue {
        glue: zipzi_core::Glue,
    },
    CreateOffscreen {
        offscreen: zipzi_core::Offscreen,
    },
    ReplaceOffscreen {
        offscreen: zipzi_core::Offscreen,
    },
    Scene {
        intent: zipzi_sdk::SceneEdit,
    },
    Organization {
        intent: zipzi_sdk::OrganizationEdit,
    },
    Binding {
        intent: zipzi_sdk::BindingAuthoringEdit,
    },
    Content {
        intent: zipzi_sdk::SceneContentEdit,
    },
    References {
        intent: zipzi_sdk::ReferenceEdit,
    },
    Resource {
        intent: zipzi_sdk::ResourceEdit,
    },
    Document {
        intent: zipzi_sdk::DocumentAuthoringEdit,
    },
    Attachment {
        intent: zipzi_core::document::AttachmentEdit,
    },
    ReplaceModelSetup {
        setup: zipzi_core::ModelSetup,
    },
    ReplaceHierarchyOrder {
        order: zipzi_core::document::HierarchyOrder,
    },
    ReplaceEditorState {
        state: zipzi_core::document::EditorState,
    },
    ReplaceDrawOrderGroups {
        groups: Vec<zipzi_core::draw_order::DrawOrderGroup>,
    },
    EraseObject {
        id: String,
    },
    RenameMesh {
        id: String,
        name: String,
    },
    UpdatePositions {
        id: String,
        vertex_ids: Vec<u32>,
        positions: Vec<zipzi_core::Vec2>,
    },
    SetMeshKeyform {
        id: String,
        keyform: zipzi_core::MeshKeyform,
    },
    SetSceneKeyform {
        id: String,
        keyform: zipzi_core::SceneKeyform,
    },
    ReplaceTopology {
        source: zipzi_sdk::GeometrySnapshot,
        replacement: zipzi_sdk::TopologyReplacement,
    },
}
impl ModelingCommand {
    pub fn apply(self, edit: &mut zipzi_sdk::EditSession<'_>) -> Result<(), zipzi_sdk::SdkError> {
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
            Self::Scene { intent } => edit.edit_scene(intent),
            Self::Organization { intent } => edit.edit_organization(intent),
            Self::Binding { intent } => edit.binding_authoring(intent),
            Self::Content { intent } => edit.edit_scene_content(intent),
            Self::References { intent } => edit.edit_references(intent),
            Self::Resource { intent } => edit.edit_resource(intent),
            Self::Document { intent } => edit.edit_document(intent),
            Self::Attachment { intent } => edit.edit_attachment(intent),
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
            Self::ReplaceTopology {
                source,
                replacement,
            } => edit.replace_topology(&source, replacement),
        }
    }
}
