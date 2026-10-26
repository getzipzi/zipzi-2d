//! Adapter dispatch for bulk modeling. Domain rules stay in the SDK.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ModelingCommand {
    CreateAsset {
        /// Complete asset record to create with native identity validation; this DTO alone does
        /// not publish.
        asset: zipzi_core::ImageAsset,
    },
    ReplaceAsset {
        /// Complete asset record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        asset: zipzi_core::ImageAsset,
    },
    CreateMesh {
        /// Complete mesh record to create with native identity validation; this DTO alone does
        /// not publish.
        mesh: zipzi_core::Mesh,
    },
    ReplaceMesh {
        /// Complete mesh record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        mesh: zipzi_core::Mesh,
    },
    CreateParameter {
        /// Complete parameter record to create with native identity validation; this DTO alone
        /// does not publish.
        parameter: zipzi_core::Parameter,
    },
    ReplaceParameter {
        /// Complete parameter record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        parameter: zipzi_core::Parameter,
    },
    CreatePart {
        /// Complete part record to create with native identity validation; this DTO alone does
        /// not publish.
        part: zipzi_core::Part,
    },
    ReplacePart {
        /// Complete part record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        part: zipzi_core::Part,
    },
    CreateTransform {
        /// Complete transform record to create with native identity validation; this DTO alone
        /// does not publish.
        transform: zipzi_core::Transform,
    },
    ReplaceTransform {
        /// Complete transform record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        transform: zipzi_core::Transform,
    },
    CreateBinding {
        /// Complete binding record to create with native identity validation; this DTO alone
        /// does not publish.
        binding: zipzi_core::MeshBinding,
    },
    ReplaceBinding {
        /// Complete binding record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        binding: zipzi_core::MeshBinding,
    },
    CreateSceneBinding {
        /// Complete scene binding record to create with native identity validation; this DTO
        /// alone does not publish.
        binding: zipzi_core::SceneBinding,
    },
    ReplaceSceneBinding {
        /// Complete scene binding record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        binding: zipzi_core::SceneBinding,
    },
    CreateBlendKeyTable {
        /// Complete blend key table record to create with native identity validation; this DTO
        /// alone does not publish.
        table: zipzi_core::BlendShapeKeyTable,
    },
    ReplaceBlendKeyTable {
        /// Complete blend key table record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        table: zipzi_core::BlendShapeKeyTable,
    },
    CreateBlendConstraint {
        /// Complete blend constraint record to create with native identity validation; this DTO
        /// alone does not publish.
        constraint: zipzi_core::BlendShapeConstraint,
    },
    ReplaceBlendConstraint {
        /// Complete blend constraint record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        constraint: zipzi_core::BlendShapeConstraint,
    },
    CreateBlendBinding {
        /// Complete blend binding record to create with native identity validation; this DTO
        /// alone does not publish.
        binding: zipzi_core::BlendShapeBinding,
    },
    ReplaceBlendBinding {
        /// Complete blend binding record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        binding: zipzi_core::BlendShapeBinding,
    },
    CreateGlue {
        /// Complete glue record to create with native identity validation; this DTO alone does
        /// not publish.
        glue: zipzi_core::Glue,
    },
    ReplaceGlue {
        /// Complete glue record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        glue: zipzi_core::Glue,
    },
    CreateOffscreen {
        /// Complete offscreen record to create with native identity validation; this DTO alone
        /// does not publish.
        offscreen: zipzi_core::Offscreen,
    },
    ReplaceOffscreen {
        /// Complete offscreen record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        offscreen: zipzi_core::Offscreen,
    },
    Scene {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_sdk::SceneEdit,
    },
    Organization {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_sdk::OrganizationEdit,
    },
    Binding {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_sdk::BindingAuthoringEdit,
    },
    Content {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_sdk::SceneContentEdit,
    },
    References {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_sdk::ReferenceEdit,
    },
    Resource {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_sdk::ResourceEdit,
    },
    Document {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_sdk::DocumentAuthoringEdit,
    },
    Attachment {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: zipzi_core::document::AttachmentEdit,
    },
    Motion {
        /// Motion library/track/key/event intent; use a Plan or apply to an Edit.
        intent: zipzi_core::document::MotionEdit,
    },
    Expression {
        /// Expression asset/entry intent, validated by the shared SDK planner.
        intent: zipzi_core::document::ExpressionEdit,
    },
    Physics {
        /// Physics rig/row/node intent with native ownership and reference checks.
        intent: zipzi_core::document::PhysicsEdit,
    },
    Pose {
        /// Pose singleton/group/entry/link intent, independent of playback state.
        intent: zipzi_core::document::PoseEdit,
    },
    CreateMotion {
        /// Complete owned clip with stable track/key/event identities.
        clip: zipzi_core::document::MotionClip,
    },
    ReplaceMotion {
        /// Complete replacement clip preserving its stable object ID.
        clip: zipzi_core::document::MotionClip,
    },
    CreateExpression {
        /// Complete expression asset with ordered owned entries.
        expression: zipzi_core::document::ExpressionAsset,
    },
    ReplaceExpression {
        /// Complete replacement expression preserving its object ID.
        expression: zipzi_core::document::ExpressionAsset,
    },
    SetPhysics {
        /// Complete Physics singleton and ordered rig data.
        physics: zipzi_core::document::PhysicsAsset,
    },
    SetPose {
        /// Complete Pose singleton and owned part/link references.
        pose: zipzi_core::document::PoseAsset,
    },
    ReplaceModelSetup {
        /// Model source-canvas and export-frame configuration.
        setup: zipzi_core::ModelSetup,
    },
    ReplaceHierarchyOrder {
        /// Replacement organization/deformation palette order; not runtime draw order.
        order: zipzi_core::document::HierarchyOrder,
    },
    ReplaceEditorState {
        /// Complete Editor visibility/lock metadata to replace.
        state: zipzi_core::document::EditorState,
    },
    ReplaceDrawOrderGroups {
        /// Complete drawing-order group list; native validation checks root/owner membership
        /// and ranges. This is a required list, not an optional grouping override.
        groups: Vec<zipzi_core::draw_order::DrawOrderGroup>,
    },
    EraseObject {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    RenameMesh {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    UpdatePositions {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Stable vertex IDs selecting which positions to replace; values are paired with
        /// positions in the supplied order.
        vertex_ids: Vec<u32>,
        /// Absolute XY shape positions in mesh vertex order; these are positions, not vertex IDs.
        positions: Vec<zipzi_core::Vec2>,
    },
    SetMeshKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
        keyform: zipzi_core::MeshKeyform,
    },
    SetSceneKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
        keyform: zipzi_core::SceneKeyform,
    },
    RemeshRectangleGrid {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Positive rectangle-grid subdivision count horizontally; resulting geometry must
        /// satisfy native vertex/working-set limits.
        columns: usize,
        /// Positive rectangle-grid subdivision count vertically; resulting geometry must
        /// satisfy native vertex/working-set limits.
        rows: usize,
    },
    ReplaceTopology {
        /// Exact geometry snapshot the topology replacement was planned against; stale source
        /// versions are rejected.
        source: zipzi_sdk::GeometrySnapshot,
        /// Coordinated replacement geometry, bindings, Glues and vertex mapping from the
        /// remeshing plan.
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
        }
    }
}
