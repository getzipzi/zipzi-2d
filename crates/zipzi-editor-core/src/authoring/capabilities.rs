//! One typed capability vocabulary for hosts. Domain states are copied from the
//! SDK; session state is kept separate so active field gestures can stay alive.
use super::*;
use anyhow::{Result, ensure};
use zipzi_sdk::{CapabilityState as C, ObjectKind as K};

#[derive(Default)]
pub(in crate::document) struct AuthoringCapabilityCache {
    version: Option<zipzi_sdk::Version>,
    objects: std::collections::HashMap<ObjectRef, ObjectAuthoringCapabilities>,
    #[cfg(test)]
    builds: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoringAction {
    Inspect,
    Patch,
    CreateChild,
    Move,
    Reorder,
    Duplicate,
    Delete,
    Dissolve,
    Repair,
    Preview,
    EditContent,
    Topology,
    LoadContent,
    Reparent,
}
#[derive(Clone, Debug)]
pub struct ObjectAuthoringCapabilities {
    states: Vec<(AuthoringAction, C)>,
    pub session_reason: Option<AuthoringSessionReason>,
}
impl ObjectAuthoringCapabilities {
    pub fn state(&self, action: AuthoringAction) -> C {
        self.states
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, s)| s.clone())
            .unwrap_or(C::Unsupported {
                code: "USE_SPECIALIZED_EDIT",
            })
    }
}
impl ObjectKind {
    /// Projection aliases (notably GlueBinding) retain their editor identity.
    pub fn sdk_kind(self) -> Option<K> {
        Some(match self {
            Self::Texture => K::Asset,
            Self::Deformer => K::Transform,
            Self::ParameterGroup => K::CdiParameterGroup,
            Self::GlueBinding => K::Glue,
            Self::Document | Self::Unresolved => return None,
            Self::Part => K::Part,
            Self::Mesh => K::Mesh,
            Self::Offscreen => K::Offscreen,
            Self::Glue => K::Glue,
            Self::GlueVertexPair => K::GlueVertexPair,
            Self::DrawOrderGroup => K::DrawOrderGroup,
            Self::Parameter => K::Parameter,
            Self::MeshBinding => K::MeshBinding,
            Self::SceneBinding => K::SceneBinding,
            Self::BindingAxis => K::BindingAxis,
            Self::BindingSample => K::BindingSample,
            Self::BlendBinding => K::BlendBinding,
            Self::BlendKeyTable => K::BlendKeyTable,
            Self::BlendConstraint => K::BlendConstraint,
            Self::MotionGroup => K::MotionGroup,
            Self::MotionRegistration => K::MotionRegistration,
            Self::Motion => K::Motion,
            Self::MotionTrack => K::MotionTrack,
            Self::MotionKey => K::MotionKey,
            Self::MotionEvent => K::MotionEvent,
            Self::Expression => K::Expression,
            Self::ExpressionEntry => K::ExpressionEntry,
            Self::Physics => K::Physics,
            Self::PhysicsGroup => K::PhysicsGroup,
            Self::PhysicsInput => K::PhysicsInput,
            Self::PhysicsOutput => K::PhysicsOutput,
            Self::PhysicsNode => K::PhysicsNode,
            Self::Pose => K::Pose,
            Self::PoseGroup => K::PoseGroup,
            Self::PoseEntry => K::PoseEntry,
            Self::PoseLink => K::PoseLink,
            Self::Attachment => K::Attachment,
            Self::ModelParameterGroup => K::ModelParameterGroup,
            Self::ModelParameterMember => K::ModelParameterMember,
            Self::ModelHitArea => K::ModelHitArea,
            Self::CdiCombinedSet => K::CdiCombinedSet,
            Self::CdiCombinedMember => K::CdiCombinedMember,
        })
    }
    pub fn from_sdk(kind: K) -> Self {
        match kind {
            K::Asset => Self::Texture,
            K::Transform => Self::Deformer,
            K::CdiParameterGroup => Self::ParameterGroup,
            K::Part => Self::Part,
            K::Mesh => Self::Mesh,
            K::Offscreen => Self::Offscreen,
            K::Glue => Self::Glue,
            K::GlueVertexPair => Self::GlueVertexPair,
            K::DrawOrderGroup => Self::DrawOrderGroup,
            K::Parameter => Self::Parameter,
            K::MeshBinding => Self::MeshBinding,
            K::SceneBinding => Self::SceneBinding,
            K::BindingAxis => Self::BindingAxis,
            K::BindingSample => Self::BindingSample,
            K::BlendBinding => Self::BlendBinding,
            K::BlendKeyTable => Self::BlendKeyTable,
            K::BlendConstraint => Self::BlendConstraint,
            K::MotionGroup => Self::MotionGroup,
            K::MotionRegistration => Self::MotionRegistration,
            K::Motion => Self::Motion,
            K::MotionTrack => Self::MotionTrack,
            K::MotionKey => Self::MotionKey,
            K::MotionEvent => Self::MotionEvent,
            K::Expression => Self::Expression,
            K::ExpressionEntry => Self::ExpressionEntry,
            K::Physics => Self::Physics,
            K::PhysicsGroup => Self::PhysicsGroup,
            K::PhysicsInput => Self::PhysicsInput,
            K::PhysicsOutput => Self::PhysicsOutput,
            K::PhysicsNode => Self::PhysicsNode,
            K::Pose => Self::Pose,
            K::PoseGroup => Self::PoseGroup,
            K::PoseEntry => Self::PoseEntry,
            K::PoseLink => Self::PoseLink,
            K::Attachment => Self::Attachment,
            K::ModelParameterGroup => Self::ModelParameterGroup,
            K::ModelParameterMember => Self::ModelParameterMember,
            K::ModelHitArea => Self::ModelHitArea,
            K::CdiCombinedSet => Self::CdiCombinedSet,
            K::CdiCombinedMember => Self::CdiCombinedMember,
        }
    }
}
impl Document {
    pub fn authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<ObjectAuthoringCapabilities> {
        // SDK capabilities describe committed state. Preview-only updates keep
        // that state; candidate availability is overlaid on every read below.
        // Full capability queries include dependency checks for deletion, so do
        // not repeat them for each property control, hover or input event.
        let mut cache = self.authoring_capability_cache.lock().unwrap();
        let version = self.session.version();
        if cache.version != Some(version) {
            cache.version = Some(version);
            cache.objects.clear();
        }
        if !cache.objects.contains_key(object) {
            let capabilities = self.query_authoring_capabilities(object)?;
            cache.objects.insert(object.clone(), capabilities);
            #[cfg(test)]
            {
                cache.builds += 1;
            }
        }
        let mut result = cache.objects[object].clone();
        result.session_reason = self
            .pending
            .as_ref()
            .map(|_| AuthoringSessionReason::CandidateActive);
        Ok(result)
    }

    fn query_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<ObjectAuthoringCapabilities> {
        use AuthoringAction as A;
        ensure!(
            self.object_index().get(object).is_some(),
            AuthoringPlanningError::new("MISSING_OBJECT")
        );
        let mut result = ObjectAuthoringCapabilities {
            states: vec![],
            session_reason: None,
        };
        macro_rules! states { ($cap:expr; $($action:ident => $field:ident),+ $(,)?) => {{
            let cap = $cap?; $(result.states.push((A::$action,cap.$field.clone()));)+
        }}; }
        let Some(kind) = object.kind.sdk_kind() else {
            result.states.push((A::Inspect, C::Available));
            if object.kind == ObjectKind::Document {
                result.states.push((
                    A::Patch,
                    self.session
                        .document_field_capability(zipzi_sdk::DocumentField::ExportSettings),
                ));
            }
            return Ok(result);
        };
        let handle = self.session.handle(kind, &object.id)?;
        match object.kind {
            ObjectKind::Part | ObjectKind::Parameter | ObjectKind::ParameterGroup => {
                states!(self.session.organization_capabilities(&handle); Inspect=>inspect, Patch=>patch, CreateChild=>create_child,
                    Move=>move_object, Reorder=>reorder, Duplicate=>duplicate, Delete=>delete, Dissolve=>dissolve);
            }
            ObjectKind::Mesh
            | ObjectKind::Deformer
            | ObjectKind::Offscreen
            | ObjectKind::Glue
            | ObjectKind::DrawOrderGroup => {
                states!(self.session.scene_capabilities(&handle); Inspect=>inspect, Patch=>patch, Move=>move_object, Reparent=>reparent,
                    Reorder=>reorder, Duplicate=>duplicate, Delete=>delete, Dissolve=>dissolve);
                if !matches!(object.kind, ObjectKind::DrawOrderGroup) {
                    states!(self.session.scene_content_capabilities(&handle); EditContent=>edit, Topology=>topology);
                }
            }
            ObjectKind::GlueVertexPair => {
                states!(self.session.scene_content_capabilities(&handle); Inspect=>inspect, EditContent=>edit, Topology=>topology);
            }
            ObjectKind::MeshBinding
            | ObjectKind::SceneBinding
            | ObjectKind::GlueBinding
            | ObjectKind::BindingAxis
            | ObjectKind::BindingSample
            | ObjectKind::BlendBinding
            | ObjectKind::BlendKeyTable
            | ObjectKind::BlendConstraint => {
                states!(self.session.binding_capabilities(&handle); Inspect=>inspect, Patch=>patch, CreateChild=>create_child, Reorder=>reorder, Delete=>delete, Repair=>repair, Preview=>preview);
            }
            ObjectKind::Motion
            | ObjectKind::MotionGroup
            | ObjectKind::MotionRegistration
            | ObjectKind::MotionTrack
            | ObjectKind::MotionKey
            | ObjectKind::MotionEvent => {
                states!(self.session.motion_capabilities(&handle); Inspect=>inspect, Patch=>patch, CreateChild=>create_child, Move=>move_object,
                    Reorder=>reorder, Duplicate=>duplicate, Delete=>delete, Repair=>repair, Preview=>preview, EditContent=>edit_content);
                // Both native group removals check the group and its registrations.
                if object.kind == ObjectKind::MotionGroup {
                    result.states.push((A::Dissolve, result.state(A::Delete)));
                }
            }
            ObjectKind::Expression | ObjectKind::ExpressionEntry => {
                states!(self.session.expression_capabilities(&handle); Inspect=>inspect, Patch=>patch, CreateChild=>create_entry, Move=>move_entry, Reorder=>patch,
                    Duplicate=>duplicate, Delete=>delete, Repair=>repair, Preview=>preview);
            }
            ObjectKind::Physics
            | ObjectKind::PhysicsGroup
            | ObjectKind::PhysicsInput
            | ObjectKind::PhysicsOutput
            | ObjectKind::PhysicsNode => {
                states!(self.session.physics_capabilities(&handle); Inspect=>inspect, Patch=>patch, CreateChild=>create_child, Move=>move_object, Reorder=>reorder,
                    Duplicate=>duplicate, Delete=>delete, Repair=>repair, Preview=>preview);
            }
            ObjectKind::Pose
            | ObjectKind::PoseGroup
            | ObjectKind::PoseEntry
            | ObjectKind::PoseLink => {
                states!(self.session.pose_capabilities(&handle); Inspect=>inspect, Patch=>patch, CreateChild=>create_child, Move=>move_object, Reorder=>reorder,
                    Duplicate=>duplicate, Delete=>delete, Repair=>repair, Preview=>preview, Dissolve=>dissolve);
            }
            ObjectKind::Texture => {
                states!(self.session.resource_capabilities(&handle); Inspect=>inspect, Patch=>patch, LoadContent=>load_content, Duplicate=>duplicate, Delete=>delete, Reorder=>reorder);
            }
            ObjectKind::Attachment => {
                states!(self.session.attachment_capabilities(&handle); Inspect=>inspect, Patch=>patch, LoadContent=>load_content, Duplicate=>duplicate, Delete=>delete, Reorder=>patch);
            }
            ObjectKind::ModelParameterGroup
            | ObjectKind::ModelParameterMember
            | ObjectKind::ModelHitArea
            | ObjectKind::CdiCombinedSet
            | ObjectKind::CdiCombinedMember => {
                states!(self.session.reference_capabilities(&handle); Inspect=>inspect, Patch=>patch, CreateChild=>create_child, Move=>move_object, Reorder=>move_object,
                    Duplicate=>duplicate, Delete=>delete, Dissolve=>dissolve);
            }
            ObjectKind::Document | ObjectKind::Unresolved => unreachable!(),
        }
        Ok(result)
    }
}

pub use zipzi_sdk::AuthoringCollection;
#[derive(Clone, Debug)]
pub struct AuthoringCollectionCapabilities {
    pub domain: zipzi_sdk::CollectionCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
impl Document {
    pub fn authoring_collection_capabilities(
        &self,
        collection: &AuthoringCollection,
    ) -> Result<AuthoringCollectionCapabilities> {
        Ok(AuthoringCollectionCapabilities {
            domain: self.session.authoring_collection_capabilities(collection)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
}

#[derive(Clone, Debug)]
pub struct AuthoringPlanningError {
    code: String,
    message: String,
}
impl AuthoringPlanningError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code: code.into(),
            message: String::new(),
        }
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn from_status(status: zipzi_core::Status) -> Self {
        Self {
            code: status.code,
            message: status.message,
        }
    }
}
impl std::fmt::Display for AuthoringPlanningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            f.write_str(&self.code)
        } else {
            write!(f, "{}: {}", self.code, self.message)
        }
    }
}
impl std::error::Error for AuthoringPlanningError {}

impl crate::Editor {
    /// Domain state shared by native menus and configured authoring commands.
    pub fn authoring_command_capability(&self, id: crate::CommandId) -> Result<C> {
        use crate::CommandId as I;
        if self.is_busy() {
            return Err(zipzi_runtime::RuntimeError::Busy.into());
        }
        let doc = self
            .document()
            .ok_or_else(|| anyhow::anyhow!("Document no longer exists"))?;
        if let Some(kind) = id.business_creation_kind() {
            return doc.business_capability(&crate::BusinessAction::Create(
                kind,
                self.active_inspection().and_then(|i| i.object()),
            ));
        }
        if let Some(kind) = id.scene_creation_type() {
            doc.validate_scene_object_creation_in(
                id.scene_creation_projection().unwrap(),
                kind,
                &doc.context().selections,
            )?;
            return Ok(C::Available);
        }
        match id {
            I::CreateMotion
            | I::CreateMotionGroup
            | I::CreateExpression
            | I::CreateExpressionEntry => {
                let kind = match id {
                    I::CreateMotion => ObjectKind::Motion,
                    I::CreateMotionGroup => ObjectKind::MotionGroup,
                    I::CreateExpression => ObjectKind::Expression,
                    _ => ObjectKind::ExpressionEntry,
                };
                doc.animation_resource_creation_capability(
                    kind,
                    self.animation_creation_selection(matches!(
                        kind,
                        ObjectKind::Motion | ObjectKind::MotionGroup
                    ))
                    .as_ref(),
                )
            }
            I::CreateParameter | I::CreateParameterGroup => doc
                .parameter_object_creation_capability(
                    if id == I::CreateParameter {
                        ObjectKind::Parameter
                    } else {
                        ObjectKind::ParameterGroup
                    },
                    &doc.context().inspected.iter().cloned().collect::<Vec<_>>(),
                ),
            I::EditParameter
            | I::DeleteParameter
            | I::EditParameterGroup
            | I::DeleteParameterGroup => {
                let expected = if matches!(id, I::EditParameter | I::DeleteParameter) {
                    ObjectKind::Parameter
                } else {
                    ObjectKind::ParameterGroup
                };
                let Some(object) = doc
                    .context()
                    .inspected
                    .as_ref()
                    .filter(|r| r.kind == expected)
                else {
                    return Ok(C::Unavailable {
                        code: "MISSING_OBJECT",
                    });
                };
                Ok(doc.authoring_capabilities(object)?.state(match id {
                    I::DeleteParameter => AuthoringAction::Delete,
                    I::DeleteParameterGroup => AuthoringAction::Dissolve,
                    _ => AuthoringAction::Inspect,
                }))
            }
            _ => Ok(C::Unsupported {
                code: "USE_SPECIALIZED_COMMAND",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_queries_reuse_committed_state_but_refresh_locks_history_and_candidates()
    -> Result<()> {
        let mut doc = Document::new_blank()?;
        let creation = doc.plan_scene_object_creation(SceneObjectType::Mesh, "Mesh".into(), &[])?;
        let mesh = doc
            .execute_scene_object_creation(&creation)?
            .suggested_selection[0]
            .clone();
        doc.select_object(mesh.clone())?;
        assert_eq!(
            doc.authoring_capabilities(&mesh)?
                .state(AuthoringAction::Patch),
            C::Available
        );
        let builds = doc.authoring_capability_cache.lock().unwrap().builds;
        for n in 1..=12 {
            doc.preview_opacity(n as f32 / 16.)?;
            let capabilities = doc.authoring_capabilities(&mesh)?;
            assert_eq!(capabilities.state(AuthoringAction::Patch), C::Available);
            assert_eq!(
                capabilities.session_reason,
                Some(AuthoringSessionReason::CandidateActive)
            );
        }
        doc.cancel_edit();
        assert_eq!(doc.authoring_capabilities(&mesh)?.session_reason, None);
        assert_eq!(
            doc.authoring_capability_cache.lock().unwrap().builds,
            builds
        );

        doc.set_objects_locked(std::slice::from_ref(&mesh), true)?;
        assert_eq!(
            doc.authoring_capabilities(&mesh)?
                .state(AuthoringAction::Patch),
            C::Unavailable {
                code: "OBJECT_LOCKED"
            }
        );
        assert_eq!(
            doc.authoring_capability_cache.lock().unwrap().builds,
            builds + 1
        );
        doc.undo()?;
        assert_eq!(
            doc.authoring_capabilities(&mesh)?
                .state(AuthoringAction::Patch),
            C::Available
        );
        doc.undo()?;
        assert!(doc.authoring_capabilities(&mesh).is_err());
        doc.redo()?;
        assert_eq!(
            doc.authoring_capabilities(&mesh)?
                .state(AuthoringAction::Patch),
            C::Available
        );
        assert!(
            doc.authoring_capabilities(&ObjectRef::new(ObjectKind::Parameter, &mesh.id))
                .is_err()
        );
        assert_eq!(doc.history_lengths(), (1, 1));
        Ok(())
    }
}
