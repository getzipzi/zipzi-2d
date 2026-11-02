/// A typed candidate mutation; select the variant for the operation and submit it through an Edit.
/// Constructing a command neither changes the candidate nor publishes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ModelingCommand {
    /// Create an image asset from its complete record.
    CreateAsset {
        /// Complete asset record to create with native identity validation; this DTO alone does
        /// not publish.
        asset: crate::model::ImageAsset,
    },
    /// Replace an existing image asset with a complete record preserving its identity.
    ReplaceAsset {
        /// Complete asset record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        asset: crate::model::ImageAsset,
    },
    /// Create a mesh from its complete record.
    CreateMesh {
        /// Complete mesh record to create with native identity validation; this DTO alone does
        /// not publish.
        mesh: crate::model::Mesh,
    },
    /// Replace an existing mesh with a complete record preserving its identity.
    ReplaceMesh {
        /// Complete mesh record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        mesh: crate::model::Mesh,
    },
    /// Create a parameter from its complete record.
    CreateParameter {
        /// Complete parameter record to create with native identity validation; this DTO alone
        /// does not publish.
        parameter: crate::model::Parameter,
    },
    /// Replace an existing parameter with a complete record preserving its identity.
    ReplaceParameter {
        /// Complete parameter record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        parameter: crate::model::Parameter,
    },
    /// Create a Part from its complete record.
    CreatePart {
        /// Complete part record to create with native identity validation; this DTO alone does
        /// not publish.
        part: crate::model::Part,
    },
    /// Replace an existing Part with a complete record preserving its identity.
    ReplacePart {
        /// Complete part record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        part: crate::model::Part,
    },
    /// Create a deformation transform from its complete record.
    CreateTransform {
        /// Complete transform record to create with native identity validation; this DTO alone
        /// does not publish.
        transform: crate::model::Transform,
    },
    /// Replace an existing deformation transform with a complete record preserving its identity.
    ReplaceTransform {
        /// Complete transform record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        transform: crate::model::Transform,
    },
    /// Create a mesh binding from its complete record.
    CreateBinding {
        /// Complete binding record to create with native identity validation; this DTO alone
        /// does not publish.
        binding: crate::model::MeshBinding,
    },
    /// Replace an existing mesh binding with a complete record preserving its identity.
    ReplaceBinding {
        /// Complete binding record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        binding: crate::model::MeshBinding,
    },
    /// Create a scene binding from its complete record.
    CreateSceneBinding {
        /// Complete scene binding record to create with native identity validation; this DTO
        /// alone does not publish.
        binding: crate::model::SceneBinding,
    },
    /// Replace an existing scene binding with a complete record preserving its identity.
    ReplaceSceneBinding {
        /// Complete scene binding record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        binding: crate::model::SceneBinding,
    },
    /// Create a blend key table from its complete record.
    CreateBlendKeyTable {
        /// Complete blend key table record to create with native identity validation; this DTO
        /// alone does not publish.
        table: crate::model::BlendShapeKeyTable,
    },
    /// Replace an existing blend key table with a complete record preserving its identity.
    ReplaceBlendKeyTable {
        /// Complete blend key table record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        table: crate::model::BlendShapeKeyTable,
    },
    /// Create a blend constraint from its complete record.
    CreateBlendConstraint {
        /// Complete blend constraint record to create with native identity validation; this DTO
        /// alone does not publish.
        constraint: crate::model::BlendShapeConstraint,
    },
    /// Replace an existing blend constraint with a complete record preserving its identity.
    ReplaceBlendConstraint {
        /// Complete blend constraint record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        constraint: crate::model::BlendShapeConstraint,
    },
    /// Create a blend binding from its complete record.
    CreateBlendBinding {
        /// Complete blend binding record to create with native identity validation; this DTO
        /// alone does not publish.
        binding: crate::model::BlendShapeBinding,
    },
    /// Replace an existing blend binding with a complete record preserving its identity.
    ReplaceBlendBinding {
        /// Complete blend binding record to replace the existing object while preserving its
        /// stable identity; this DTO alone does not publish.
        binding: crate::model::BlendShapeBinding,
    },
    /// Create a Glue from its complete record.
    CreateGlue {
        /// Complete glue record to create with native identity validation; this DTO alone does
        /// not publish.
        glue: crate::model::Glue,
    },
    /// Replace an existing Glue with a complete record preserving its identity.
    ReplaceGlue {
        /// Complete glue record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        glue: crate::model::Glue,
    },
    /// Create an Offscreen from its complete record.
    CreateOffscreen {
        /// Complete offscreen record to create with native identity validation; this DTO alone
        /// does not publish.
        offscreen: crate::model::Offscreen,
    },
    /// Replace an existing Offscreen with a complete record preserving its identity.
    ReplaceOffscreen {
        /// Complete offscreen record to replace the existing object while preserving its stable
        /// identity; this DTO alone does not publish.
        offscreen: crate::model::Offscreen,
    },
    /// Request dependency-aware deletion; plan first to inspect the required decisions.
    Deletion {
        /// Dependency-cleaning deletion request; plan first to inspect choices and bounded report pages.
        intent: crate::authoring::DeletionIntent,
    },
    /// Apply a scene-domain edit to the candidate.
    Scene {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::SceneEdit,
    },
    /// Apply an organization and hierarchy edit to the candidate.
    Organization {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::OrganizationEdit,
    },
    /// Apply a binding authoring edit to the candidate.
    Binding {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::authoring::BindingAuthoringEdit,
    },
    /// Apply an edit to scene content.
    Content {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::SceneContentEdit,
    },
    /// Apply a document reference edit.
    References {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::ReferenceEdit,
    },
    /// Apply an edit to document resources.
    Resource {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::ResourceEdit,
    },
    /// Apply a document-level authoring edit.
    Document {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::DocumentAuthoringEdit,
    },
    /// Apply a package attachment edit.
    Attachment {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: crate::document::AttachmentEdit,
    },
    /// Edit motion library, track, key or event data.
    Motion {
        /// Motion library/track/key/event intent; use a Plan or apply to an Edit.
        intent: crate::document::MotionEdit,
    },
    /// Edit expression assets or entries.
    Expression {
        /// Expression asset/entry intent, validated by the shared SDK planner.
        intent: crate::document::ExpressionEdit,
    },
    /// Edit Physics rig, rows or nodes.
    Physics {
        /// Physics rig/row/node intent with native ownership and reference checks.
        intent: crate::physics_document::PhysicsEdit,
    },
    /// Edit Pose singleton, groups, entries or links.
    Pose {
        /// Pose singleton/group/entry/link intent, independent of playback state.
        intent: crate::document::PoseEdit,
    },
    /// Create a motion clip from its complete record.
    CreateMotion {
        /// Complete owned clip with stable track/key/event identities.
        clip: crate::document::MotionClip,
    },
    /// Replace an existing motion clip with a complete record preserving its identity.
    ReplaceMotion {
        /// Complete replacement clip preserving its stable object ID.
        clip: crate::document::MotionClip,
    },
    /// Create an expression asset from its complete record.
    CreateExpression {
        /// Complete expression asset with ordered owned entries.
        expression: crate::document::ExpressionAsset,
    },
    /// Replace an existing expression asset with a complete record preserving its identity.
    ReplaceExpression {
        /// Complete replacement expression preserving its object ID.
        expression: crate::document::ExpressionAsset,
    },
    /// Replace the Physics singleton with the supplied rig data.
    SetPhysics {
        /// Complete Physics singleton and ordered rig data.
        physics: crate::physics_document::PhysicsAsset,
    },
    /// Replace the Pose singleton with the supplied data.
    SetPose {
        /// Complete Pose singleton and owned part/link references.
        pose: crate::document::PoseAsset,
    },
    /// Replace model canvas and export-frame configuration.
    ReplaceModelSetup {
        /// Model source-canvas and export-frame configuration.
        setup: crate::model::ModelSetup,
    },
    /// Replace organization and deformation palette ordering.
    ReplaceHierarchyOrder {
        /// Replacement organization/deformation palette order; not runtime draw order.
        order: crate::document::HierarchyOrder,
    },
    /// Replace Editor visibility and lock metadata.
    ReplaceEditorState {
        /// Complete Editor visibility/lock metadata to replace.
        state: crate::document::EditorState,
    },
    /// Replace the complete draw-order group list.
    ReplaceDrawOrderGroups {
        /// Complete drawing-order group list; native validation checks root/owner membership
        /// and ranges. This is a required list, not an optional grouping override.
        groups: Vec<crate::document::DrawOrderGroup>,
    },
    /// Erase the object identified by its stable document ID.
    EraseObject {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    /// Change the display name of an existing mesh.
    RenameMesh {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    /// Replace positions for the selected mesh vertex IDs in paired order.
    UpdatePositions {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Stable vertex IDs selecting which positions to replace; values are paired with
        /// positions in the supplied order.
        vertex_ids: Vec<u32>,
        /// Absolute XY shape positions paired with vertex_ids in the supplied order; this can be
        /// a subset and need not follow mesh storage order.
        positions: Vec<crate::model::Vec2>,
    },
    /// Insert or replace one mesh shape sample.
    SetMeshKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
        keyform: crate::model::MeshKeyform,
    },
    /// Insert or replace one scene shape sample.
    SetSceneKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
        keyform: crate::model::SceneKeyform,
    },
    /// Regenerate a mesh using the requested rectangular grid.
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
    /// Apply a coordinated topology replacement against its captured geometry source.
    ReplaceTopology {
        /// Exact geometry snapshot the topology replacement was planned against; stale source
        /// versions are rejected.
        source: crate::authoring::GeometrySnapshot,
        /// Coordinated replacement geometry, bindings, Glues and vertex mapping from the
        /// remeshing plan.
        replacement: crate::authoring::TopologyReplacement,
    },
    /// Apply one source-checked manual UV topology operation.
    ManualMesh {
        /// Stable mesh UUID whose editable UV triangulation is changed.
        mesh_id: String,
        /// Exact candidate source before this operation; submit it alone in a batch.
        expected: SourceRef,
        /// Atomic UV topology intent; the SDK preserves dependent geometry and identities.
        operation: crate::authoring::ManualMeshOperation,
    },
}
