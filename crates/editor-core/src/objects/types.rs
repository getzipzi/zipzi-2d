//! Object identities, panel payloads, and typed graph relationships.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ObjectKind {
    Document,
    Part,
    Mesh,
    Deformer,
    Offscreen,
    Parameter,
    ParameterGroup,
    MeshBinding,
    BindingAxis,
    BindingSample,
    SceneBinding,
    GlueBinding,
    BlendBinding,
    BlendKeyTable,
    BlendConstraint,
    Glue,
    MotionGroup,
    MotionRegistration,
    MotionKey,
    MotionEvent,
    Motion,
    MotionTrack,
    Expression,
    ExpressionEntry,
    Physics,
    PhysicsGroup,
    PhysicsInput,
    PhysicsOutput,
    PhysicsNode,
    Pose,
    PoseGroup,
    PoseEntry,
    Texture,
    Attachment,
    Unresolved,
}
impl ObjectKind {
    pub fn is_edit_target(self) -> bool {
        matches!(
            self,
            Self::Mesh | Self::Deformer | Self::Part | Self::Glue | Self::Offscreen
        )
    }
    pub fn is_binding(self) -> bool {
        matches!(
            self,
            Self::MeshBinding | Self::SceneBinding | Self::GlueBinding | Self::BlendBinding
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectRef {
    pub kind: ObjectKind,
    pub id: String,
}
impl ObjectRef {
    pub fn new(kind: ObjectKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ObjectInfo {
    pub reference: ObjectRef,
    pub name: String,
    pub properties: Value,
    pub diagnostic: Option<String>,
    pub enabled: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationKind {
    PartOwner,
    DeformParent,
    ParameterAxis,
    BindingTarget,
    BlendTarget,
    KeyTable,
    Constraint,
    Mask,
    GlueA,
    GlueB,
    Resource,
    Contains,
    ParameterGroup,
    MotionWrites,
    ExpressionWrites,
    EntryOf,
    PhysicsInput,
    PhysicsOutput,
    PhysicsSample,
    PoseMember,
    PoseLink,
    HitArea,
}
#[derive(Clone, Debug)]
pub struct Relation {
    pub source: ObjectRef,
    pub target: ObjectRef,
    pub kind: RelationKind,
    pub detail: String,
}
impl RelationKind {
    pub fn label(self, incoming: bool) -> &'static str {
        use RelationKind::*;
        match (self, incoming) {
            (PartOwner, false) => "belongs to Part",
            (PartOwner, true) => "owns",
            (DeformParent, false) => "deformed by",
            (DeformParent, true) => "deforms",
            (ParameterAxis, false) => "drives binding axis",
            (ParameterAxis, true) => "parameter axis",
            (BindingTarget, false) => "drives target",
            (BindingTarget, true) => "driven by binding",
            (BlendTarget, false) => "adds BlendShape to",
            (BlendTarget, true) => "BlendShape source",
            (KeyTable, false) => "uses shared key table",
            (KeyTable, true) => "key table used by",
            (Constraint, false) => "uses constraint",
            (Constraint, true) => "constraint used by",
            (Mask, false) => "uses mask",
            (Mask, true) => "provides mask for",
            (GlueA, false) => "connects Mesh A",
            (GlueA, true) => "Glue endpoint A",
            (GlueB, false) => "connects Mesh B",
            (GlueB, true) => "Glue endpoint B",
            (Resource, false) => "uses resource",
            (Resource, true) => "resource used by",
            (Contains, false) => "contains",
            (Contains, true) => "contained in",
            (ParameterGroup, false) => "in parameter group",
            (ParameterGroup, true) => "group member",
            (MotionWrites, false) => "writes motion target",
            (MotionWrites, true) => "written by motion track",
            (EntryOf, false) => "entry of",
            (EntryOf, true) => "contains entry",
            (ExpressionWrites, false) => "writes expression parameter",
            (ExpressionWrites, true) => "written by expression",
            (PhysicsInput, false) => "reads physics input",
            (PhysicsInput, true) => "physics input to",
            (PhysicsSample, false) => "samples physics node",
            (PhysicsSample, true) => "sampled by physics output",
            (PhysicsOutput, false) => "writes physics output",
            (PhysicsOutput, true) => "physics output from",
            (PoseMember, false) => "switches Part",
            (PoseMember, true) => "switched by Pose",
            (PoseLink, false) => "links Part visibility",
            (PoseLink, true) => "visibility linked by Pose",
            (HitArea, false) => "hit area Mesh",
            (HitArea, true) => "used by hit area",
        }
    }
}
