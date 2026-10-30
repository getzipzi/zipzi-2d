//! Data in publication summaries.
use crate::authoring::ObjectKind;
use crate::document::{
    AttachmentRef, ExpressionTarget, MotionTrackTarget, PosePartRef, ReferenceFields as F,
    ReferenceSelectionField as S, SceneContentDomain as D, SceneField,
};
use crate::physics_document::PhysicsTarget;
use crate::scene_reference::SceneReference;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneFieldChange {
    pub entity: EntityRef,
    pub field: SceneField,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneReferenceChange {
    pub entity: EntityRef,
    pub field: SceneField,
    pub before: Vec<SceneReference>,
    pub after: Vec<SceneReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrganizationReferenceChange {
    pub entity: EntityRef,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceFieldChange {
    pub entity: EntityRef,
    pub before_owner: Option<String>,
    pub after_owner: Option<String>,
    pub before: Option<F>,
    pub after: Option<F>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceSelectionChange {
    pub entity: EntityRef,
    pub field: S,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindingReferenceChange {
    pub entity: EntityRef,
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoseReferenceChange {
    pub entity: EntityRef,
    pub before: Option<PosePartRef>,
    pub after: Option<PosePartRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PhysicsReferenceChange {
    pub entity: EntityRef,
    pub field: String,
    pub before: Option<PhysicsReferenceValue>,
    pub after: Option<PhysicsReferenceValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhysicsReferenceValue {
    Parameter { target: PhysicsTarget },
    Node { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneContentChange {
    pub entity: EntityRef,
    pub domain: D,
    pub topology: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneContentReferenceChange {
    pub entity: EntityRef,
    pub before: SceneContentReferenceValue,
    pub after: SceneContentReferenceValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SceneContentReferenceValue {
    Absent,
    Pair {
        glue_id: String,
        mesh_a: SceneReference,
        mesh_b: SceneReference,
        vertex_a: Option<crate::model::VertexId>,
        vertex_b: Option<crate::model::VertexId>,
    },
    OffscreenMapping {
        part_id: String,
        indices: Vec<i32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotionReferenceChange {
    pub entity: EntityRef,
    pub field: String,
    pub before: Option<MotionReferenceValue>,
    pub after: Option<MotionReferenceValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MotionReferenceValue {
    Clip { id: String },
    Track { target: MotionTrackTarget },
    Attachment { reference: AttachmentRef },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeCategory {
    Labels,
    Order,
    References,
    Content,
    Topology,
    EvaluationInputs,
    EditorState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityRef {
    pub document_id: String,
    pub kind: ObjectKind,
    pub id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionRef {
    Resources {
        document_id: String,
    },
    References {
        document_id: String,
        collection: crate::document::ReferenceCollection,
    },
    SceneContent {
        document_id: String,
        owner_id: String,
        domain: crate::document::SceneContentDomain,
    },
    Scene {
        document_id: String,
        collection: crate::document::SceneCollection,
    },
    Organization {
        document_id: String,
        collection: crate::document::OrganizationCollection,
    },
    DeformationChildren {
        document_id: String,
        parent_id: String,
    },
    BindingGrid {
        document_id: String,
        binding_id: String,
    },
    Pose {
        document_id: String,
        owner: Option<String>,
        role: String,
    },
    Physics {
        document_id: String,
        owner: Option<String>,
        role: String,
    },
    Motion {
        document_id: String,
        owner: Option<String>,
        role: String,
    },
    Expressions {
        document_id: String,
    },
    ExpressionEntries {
        document_id: String,
        expression_id: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceChange {
    pub entry: EntityRef,
    pub before: Option<ExpressionTarget>,
    pub after: Option<ExpressionTarget>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ChangeSet {
    pub document_fields: Vec<crate::document::DocumentField>,
    pub created: Vec<EntityRef>,
    pub deleted: Vec<EntityRef>,
    pub modified: Vec<EntityRef>,
    pub collections: Vec<CollectionRef>,
    pub references: Vec<ReferenceChange>,
    pub motion_references: Vec<MotionReferenceChange>,
    pub reference_fields: Vec<ReferenceFieldChange>,
    pub reference_selections: Vec<ReferenceSelectionChange>,
    pub content_references: Vec<SceneContentReferenceChange>,
    pub content_changes: Vec<SceneContentChange>,
    pub scene_fields: Vec<SceneFieldChange>,
    pub scene_references: Vec<SceneReferenceChange>,
    pub organization_references: Vec<OrganizationReferenceChange>,
    pub pose_references: Vec<PoseReferenceChange>,
    // Root `modified` also describes child payload changes; only actual Pose
    // fields retire field addresses. Keep this publication metadata off wire.
    #[serde(skip)]
    pose_field_changes: Vec<String>,
    pub physics_references: Vec<PhysicsReferenceChange>,
    pub binding_references: Vec<BindingReferenceChange>,
    pub resource_content: Vec<EntityRef>,
    pub categories: BTreeSet<ChangeCategory>,
    pub full_invalidation: bool,
}

impl ChangeSet {
    pub fn pose_field_changes(&self) -> &Vec<String> {
        &self.pose_field_changes
    }
    pub fn record_pose_field_change(&mut self, id: String) {
        self.pose_field_changes.push(id);
    }
}
