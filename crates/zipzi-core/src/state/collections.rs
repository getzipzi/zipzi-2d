use super::*;
use zipzi_contract::authoring::{ObjectKind, PhysicsCollection};
impl DocumentState {
    pub fn pose_collection(&self, collection: PoseCollection) -> Result<Vec<String>, Status> {
        let d = self.content.model();
        let missing = |id: &str| Status::error("MISSING_POSE_OBJECT", id);
        let (_kind, ids): (ObjectKind, Vec<&str>) = match collection {
            PoseCollection::Groups => (
                ObjectKind::PoseGroup,
                d.pose()
                    .map(|p| p.groups.iter().map(|g| g.id.as_str()).collect())
                    .unwrap_or_default(),
            ),
            PoseCollection::Entries { group_id } => (
                ObjectKind::PoseEntry,
                d.get_pose_group(&group_id)
                    .ok_or_else(|| missing(&group_id))?
                    .entries
                    .iter()
                    .map(|e| e.id.as_str())
                    .collect(),
            ),
            PoseCollection::Links { entry_id } => (
                ObjectKind::PoseLink,
                d.get_pose_entry(&entry_id)
                    .ok_or_else(|| missing(&entry_id))?
                    .1
                    .links
                    .iter()
                    .flatten()
                    .map(|l| l.id.as_str())
                    .collect(),
            ),
        };
        Ok(ids.into_iter().map(str::to_owned).collect())
    }
    pub fn physics_collection(&self, collection: PhysicsCollection) -> Result<Vec<String>, Status> {
        let doc = self.content.model();
        let asset = doc
            .physics()
            .ok_or_else(|| Status::error("MISSING_PHYSICS", "create a group first"))?;
        let (_kind, ids) = match collection {
            PhysicsCollection::Groups => (
                ObjectKind::PhysicsGroup,
                asset
                    .data
                    .settings
                    .iter()
                    .map(|g| g.id.as_str())
                    .collect::<Vec<_>>(),
            ),
            PhysicsCollection::Inputs { ref group_id }
            | PhysicsCollection::Outputs { ref group_id }
            | PhysicsCollection::Nodes { ref group_id } => {
                let g = doc
                    .get_physics_group(group_id)
                    .ok_or_else(|| Status::error("MISSING_PHYSICS_GROUP", group_id))?;
                match collection {
                    PhysicsCollection::Inputs { .. } => (
                        ObjectKind::PhysicsInput,
                        g.inputs.iter().map(|v| v.id.as_str()).collect(),
                    ),
                    PhysicsCollection::Outputs { .. } => (
                        ObjectKind::PhysicsOutput,
                        g.outputs.iter().map(|v| v.id.as_str()).collect(),
                    ),
                    _ => (
                        ObjectKind::PhysicsNode,
                        g.vertices.iter().map(|v| v.id.as_str()).collect(),
                    ),
                }
            }
        };
        Ok(ids.into_iter().map(str::to_owned).collect())
    }
    pub fn reference_object_kind(&self, id: &str) -> Option<ObjectKind> {
        self.content
            .model()
            .reference_identity(id)
            .map(|i| match i.kind {
                ReferenceKind::ModelParameterGroup => ObjectKind::ModelParameterGroup,
                ReferenceKind::ModelParameterMember => ObjectKind::ModelParameterMember,
                ReferenceKind::ModelHitArea => ObjectKind::ModelHitArea,
                ReferenceKind::CdiCombinedSet => ObjectKind::CdiCombinedSet,
                ReferenceKind::CdiCombinedMember => ObjectKind::CdiCombinedMember,
            })
    }
}

impl DocumentState {
    pub fn deletion_target(&self, kind: ObjectKind, id: &str) -> Result<DeletionTarget, Status> {
        self.require_object(kind, id)?;
        Ok(match kind {
            ObjectKind::Mesh => DeletionTarget::Mesh(id.into()),
            ObjectKind::Glue => DeletionTarget::Glue(id.into()),
            ObjectKind::Offscreen => DeletionTarget::Offscreen(id.into()),
            ObjectKind::Part => DeletionTarget::Part(id.into()),
            ObjectKind::Transform => DeletionTarget::Deformer(id.into()),
            ObjectKind::Asset => DeletionTarget::Texture(id.into()),
            ObjectKind::Parameter => DeletionTarget::Parameter(id.into()),
            ObjectKind::CdiParameterGroup => DeletionTarget::ParameterFolder(id.into()),
            ObjectKind::Attachment => DeletionTarget::Attachment(id.into()),
            ObjectKind::MeshBinding => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::MeshBinding,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::BindingAxis => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::Axis,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::BindingSample => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::Sample,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::SceneBinding => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::SceneBinding,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::BlendKeyTable => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::BlendKeyTable,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::BlendConstraint => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::BlendConstraint,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::BlendBinding => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::BlendBinding,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::GlueVertexPair => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::GluePair,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::DrawOrderGroup => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::DrawingGroup,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::CdiCombinedSet => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::CdiCombinedSet,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::CdiCombinedMember => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::CdiCombinedMember,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::ModelParameterGroup => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::ModelParameterGroup,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::ModelParameterMember => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::ModelParameterMember,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::ModelHitArea => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::HitArea,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::Expression => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::Expression,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::ExpressionEntry => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::ExpressionEntry,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::Motion => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::Motion,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::MotionGroup => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::MotionGroup,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::MotionRegistration => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::MotionRegistration,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::MotionTrack => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::MotionTrack,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::MotionKey => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::MotionKey,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::MotionEvent => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::MotionEvent,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::Pose => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::Pose,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::PoseGroup => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::PoseGroup,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::PoseEntry => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::PoseEntry,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::PoseLink => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::PoseLink,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::Physics => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::Physics,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::PhysicsGroup => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::PhysicsGroup,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::PhysicsInput => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::PhysicsInput,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::PhysicsOutput => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::PhysicsOutput,
                id: id.into(),
                owner_id: None,
            }),
            ObjectKind::PhysicsNode => DeletionTarget::Record(DeletionObject {
                kind: DeletionKind::PhysicsNode,
                id: id.into(),
                owner_id: None,
            }),
        })
    }
}
