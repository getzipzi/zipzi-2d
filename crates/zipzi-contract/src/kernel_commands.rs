//! Typed commands for one isolated Draft batch. These values carry no handles,
//! model mutation, Session ownership or transport recovery behavior.
use crate::physics_document::PhysicsAsset;
use crate::{authoring::*, document::*, model::*};
use serde::{Deserialize, Serialize};
use zipzi_api_derive::ApiShape;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PropertyPatch {
    Mesh {
        id: String,
        keyform: Option<(String, Vec<String>)>,
        appearance: Appearance,
        draw_order: Option<f32>,
    },
    Parameter {
        id: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        decimal_places: i32,
    },
    Part {
        id: String,
        draw_order: f32,
    },
    Glue {
        id: String,
        intensity: f32,
    },
    Motion {
        id: String,
        duration: f32,
        fps: f32,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    },
    Expression {
        id: String,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    },
    Export {
        padding_pixels: f32,
        minimum_extent: f32,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
)]
pub struct MotionKeyRef {
    pub track_id: String,
    pub key_id: String,
}

#[derive(
    Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape,
)]
pub struct MotionKeyMove {
    pub key: MotionKeyRef,
    /// Absolute seconds and raw channel value, calculated from the gesture origin.
    pub point: MotionPoint,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
)]
#[serde(rename_all = "snake_case")]
pub enum MotionInterpolation {
    Linear,
    Bezier,
    Stepped,
    InverseStepped,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
)]
#[serde(rename_all = "snake_case")]
pub enum MotionBezierHandle {
    Control1,
    Control2,
}

/// Key time order and Bezier roles use dedicated curve operations, never row sorting.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MotionCurveEdit {
    MoveKeys(Vec<MotionKeyMove>),
    InsertKey {
        track: String,
        id: String,
        time: f32,
        value: Option<f32>,
    },
    DeleteKeys(Vec<MotionKeyRef>),
    Interpolation {
        keys: Vec<MotionKeyRef>,
        kind: MotionInterpolation,
    },
    BezierHandle {
        end: MotionKeyRef,
        handle: MotionBezierHandle,
        point: MotionPoint,
    },
    ScaleTime {
        duration: f32,
    },
}

/// Stable address. Nested objects are identified by their stable child ID.
#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(deny_unknown_fields)]
pub struct ObjectAddress {
    pub kind: ObjectKind,
    pub id: String,
}

/// Captured image input; decoding/admission uses the host ResourceStore.
#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageInput {
    Rgba {
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    },
    File {
        path: std::path::PathBuf,
    },
}

/// Commands run atomically in one Draft batch. Source checking is on the batch,
/// so positional selectors cannot be replayed against a different query source.
#[derive(Debug, Clone, Serialize, Deserialize, ApiShape)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Rename {
        target: ObjectAddress,
        name: String,
    },
    CreateParameter {
        spec: ParameterCreateSpec,
    },
    PutParameter {
        value: Parameter,
    },
    PatchParameter {
        id: String,
        patch: ParameterPatch,
    },
    CreatePart {
        value: Part,
    },
    CreateMesh {
        spec: MeshCreateSpec,
    },
    PutMesh {
        value: Mesh,
    },
    CreateAsset {
        id: String,
        name: String,
    },
    ReplaceImage {
        id: String,
        input: ImageInput,
    },
    DuplicateAsset {
        source: String,
        id: String,
        name: String,
    },
    DeleteAsset {
        id: String,
        replacement: Option<crate::scene_reference::SceneReference>,
    },
    ClearImage {
        id: String,
    },
    ReorderAssets {
        order: Vec<String>,
    },
    VertexPositions {
        mesh: String,
        updates: Vec<(u32, Vec2)>,
    },
    CreateBinding {
        spec: MeshBindingCreateSpec,
    },
    PutBinding {
        value: MeshBinding,
    },
    MeshKeyform {
        binding: String,
        value: MeshKeyformSpec,
    },
    CreateSceneBinding {
        value: SceneBinding,
    },
    PutSceneBinding {
        value: SceneBinding,
    },
    SceneKeyform {
        binding: String,
        value: SceneKeyform,
    },
    BindingGrid {
        edit: BindingGridEdit,
    },
    BlendShape {
        edit: BlendAuthoringEdit,
    },
    PutBlendBinding {
        value: BlendShapeBinding,
    },
    ParameterRange {
        edit: ParameterRangeEdit,
    },
    ManualMesh {
        mesh: String,
        operation: ManualMeshOperation,
    },
    Remesh {
        mesh: String,
        geometry: MeshGeometry,
    },
    RemeshTextureUvs {
        mesh: String,
        geometry: MeshGeometry,
        texture_uvs: Vec<Vec2>,
    },
    RectangleGrid {
        mesh: String,
        columns: usize,
        rows: usize,
    },
    Organization {
        edit: OrganizationEdit,
    },
    DisplayInfo {
        value: DisplayInfo,
    },
    Scene {
        edit: SceneEdit,
    },
    GroupDeformer {
        create: SceneEdit,
        sources: Vec<String>,
    },
    SceneContent {
        edit: SceneContentEdit,
    },
    PatchSceneContent {
        owner: String,
        target: SceneContentTarget,
        expected: SceneContentValue,
        value: SceneContentValue,
    },
    Delete {
        request: DeletionRequest,
    },
    Reference {
        edit: ReferenceEdit,
    },
    ModelSettings {
        value: Model3Settings,
    },
    CreateMotion {
        value: MotionClip,
    },
    PutMotion {
        value: MotionClip,
    },
    Motion {
        edit: MotionEdit,
    },
    MotionCurve {
        clip: String,
        edit: MotionCurveEdit,
    },
    CreateExpression {
        value: ExpressionAsset,
    },
    PutExpression {
        value: ExpressionAsset,
    },
    Expression {
        edit: ExpressionEdit,
    },
    PutPose {
        value: Option<PoseAsset>,
    },
    Pose {
        edit: PoseEdit,
    },
    PutPhysics {
        value: Option<PhysicsAsset>,
    },
    Physics {
        edit: crate::physics_document::PhysicsEdit,
    },
    PatchPhysics {
        patch: crate::physics_document::PhysicsPatch,
    },
    Attachment {
        edit: AttachmentEdit,
    },
    EditorState {
        value: EditorState,
    },
    Document {
        edit: DocumentAuthoringEdit,
    },
    Property {
        patch: PropertyPatch,
    },
}

/// Allocated identity returned by creation; all writes remain in the same Draft.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ApiShape)]
#[serde(deny_unknown_fields)]
pub struct CommandOutcome {
    pub created: Option<String>,
}
