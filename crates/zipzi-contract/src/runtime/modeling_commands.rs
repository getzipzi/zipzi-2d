#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ModelingCommand {
    CreateAsset {
        /// Complete asset record to create with native identity validation; this DTO alone does
        /// not publish.
        asset: crate::model::ImageAsset,
    },
    ReplaceAsset {
        /// Complete asset record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        asset: crate::model::ImageAsset,
    },
    CreateMesh {
        /// Complete mesh record to create with native identity validation; this DTO alone does
        /// not publish.
        mesh: crate::model::Mesh,
    },
    ReplaceMesh {
        /// Complete mesh record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        mesh: crate::model::Mesh,
    },
    CreateParameter {
        /// Complete parameter record to create with native identity validation; this DTO alone
        /// does not publish.
        parameter: crate::model::Parameter,
    },
    ReplaceParameter {
        /// Complete parameter record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        parameter: crate::model::Parameter,
    },
    CreatePart {
        /// Complete part record to create with native identity validation; this DTO alone does
        /// not publish.
        part: crate::model::Part,
    },
    ReplacePart {
        /// Complete part record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        part: crate::model::Part,
    },
    CreateTransform {
        /// Complete transform record to create with native identity validation; this DTO alone
        /// does not publish.
        transform: crate::model::Transform,
    },
    ReplaceTransform {
        /// Complete transform record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        transform: crate::model::Transform,
    },
    CreateBinding {
        /// Complete binding record to create with native identity validation; this DTO alone
        /// does not publish.
        binding: crate::model::MeshBinding,
    },
    ReplaceBinding {
        /// Complete binding record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        binding: crate::model::MeshBinding,
    },
    CreateSceneBinding {
        /// Complete scene binding record to create with native identity validation; this DTO
        /// alone does not publish.
        binding: crate::model::SceneBinding,
    },
    ReplaceSceneBinding {
        /// Complete scene binding record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        binding: crate::model::SceneBinding,
    },
    CreateBlendKeyTable {
        /// Complete blend key table record to create with native identity validation; this DTO
        /// alone does not publish.
        table: crate::model::BlendShapeKeyTable,
    },
    ReplaceBlendKeyTable {
        /// Complete blend key table record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        table: crate::model::BlendShapeKeyTable,
    },
    CreateBlendConstraint {
        /// Complete blend constraint record to create with native identity validation; this DTO
        /// alone does not publish.
        constraint: crate::model::BlendShapeConstraint,
    },
    ReplaceBlendConstraint {
        /// Complete blend constraint record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        constraint: crate::model::BlendShapeConstraint,
    },
    CreateBlendBinding {
        /// Complete blend binding record to create with native identity validation; this DTO
        /// alone does not publish.
        binding: crate::model::BlendShapeBinding,
    },
    ReplaceBlendBinding {
        /// Complete blend binding record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        binding: crate::model::BlendShapeBinding,
    },
    CreateGlue {
        /// Complete glue record to create with native identity validation; this DTO alone does
        /// not publish.
        glue: crate::model::Glue,
    },
    ReplaceGlue {
        /// Complete glue record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        glue: crate::model::Glue,
    },
    CreateOffscreen {
        /// Complete offscreen record to create with native identity validation; this DTO alone
        /// does not publish.
        offscreen: crate::model::Offscreen,
    },
    ReplaceOffscreen {
        /// Complete offscreen record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        offscreen: crate::model::Offscreen,
    },
    Deletion {
        /// Dependency-cleaning deletion request; plan first to inspect choices and bounded report pages.
        intent: crate::authoring::DeletionIntent,
    },
    Scene {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::SceneEdit,
    },
    Organization {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::OrganizationEdit,
    },
    Binding {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::authoring::BindingAuthoringEdit,
    },
    Content {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::SceneContentEdit,
    },
    References {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::ReferenceEdit,
    },
    Resource {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::ResourceEdit,
    },
    Document {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::DocumentAuthoringEdit,
    },
    Attachment {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::AttachmentEdit,
    },
    Motion {
        /// Motion library/track/key/event intent; use a Plan or apply to an Edit.
        intent: crate::document::MotionEdit,
    },
    Expression {
        /// Expression asset/entry intent, validated by the shared SDK planner.
        intent: crate::document::ExpressionEdit,
    },
    Physics {
        /// Physics rig/row/node intent with native ownership and reference checks.
        intent: crate::physics_document::PhysicsEdit,
    },
    Pose {
        /// Pose singleton/group/entry/link intent, independent of playback state.
        intent: crate::document::PoseEdit,
    },
    CreateMotion {
        /// Complete owned clip with stable track/key/event identities.
        clip: crate::document::MotionClip,
    },
    ReplaceMotion {
        /// Complete replacement clip preserving its stable object ID.
        clip: crate::document::MotionClip,
    },
    CreateExpression {
        /// Complete expression asset with ordered owned entries.
        expression: crate::document::ExpressionAsset,
    },
    ReplaceExpression {
        /// Complete replacement expression preserving its object ID.
        expression: crate::document::ExpressionAsset,
    },
    SetPhysics {
        /// Complete Physics singleton and ordered rig data.
        physics: crate::physics_document::PhysicsAsset,
    },
    SetPose {
        /// Complete Pose singleton and owned part/link references.
        pose: crate::document::PoseAsset,
    },
    ReplaceModelSetup {
        /// Model source-canvas and export-frame configuration.
        setup: crate::model::ModelSetup,
    },
    ReplaceHierarchyOrder {
        /// Replacement organization/deformation palette order; not runtime draw order.
        order: crate::document::HierarchyOrder,
    },
    ReplaceEditorState {
        /// Complete Editor visibility/lock metadata to replace.
        state: crate::document::EditorState,
    },
    ReplaceDrawOrderGroups {
        /// Complete drawing-order group list; native validation checks root/owner membership
        /// and ranges. This is a required list, not an optional grouping override.
        groups: Vec<crate::document::DrawOrderGroup>,
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
        positions: Vec<crate::model::Vec2>,
    },
    SetMeshKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
        keyform: crate::model::MeshKeyform,
    },
    SetSceneKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
        keyform: crate::model::SceneKeyform,
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
        source: crate::authoring::GeometrySnapshot,
        /// Coordinated replacement geometry, bindings, Glues and vertex mapping from the
        /// remeshing plan.
        replacement: crate::authoring::TopologyReplacement,
    },
    ManualMesh {
        /// Stable mesh UUID whose editable UV triangulation is changed.
        mesh_id: String,
        /// Exact candidate source before this operation; submit it alone in a batch.
        expected: SourceRef,
        /// Atomic UV topology intent; the SDK preserves dependent geometry and identities.
        operation: crate::authoring::ManualMeshOperation,
    },
}
