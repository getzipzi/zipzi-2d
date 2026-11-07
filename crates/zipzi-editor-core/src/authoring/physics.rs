use super::*;
use anyhow::{Result, ensure};
use zipzi_core::document::PhysicsEdit;
impl Document {
    pub fn physics_group_creation(&self, name: String) -> PhysicsEdit {
        PhysicsEdit::CreateGroup {
            asset_id: self
                .model()
                .physics()
                .as_deref()
                .map_or_else(|| Uuid::new_v4().to_string(), |a| a.id.clone()),
            id: Uuid::new_v4().to_string(),
            name,
            before: None,
        }
    }
    pub fn physics_group_step(&self, id: &str, down: bool) -> Result<Option<PhysicsEdit>> {
        let groups = self
            .model()
            .physics_collection(zipzi_contract::authoring::PhysicsCollection::Groups)?;
        let i = groups
            .iter()
            .position(|g| g == id)
            .ok_or_else(|| anyhow::anyhow!("Physics group no longer exists"))?;
        if (!down && i == 0) || (down && i + 1 >= groups.len()) {
            return Ok(None);
        }
        let before = groups.get(if down { i + 2 } else { i - 1 }).cloned();
        Ok(Some(PhysicsEdit::ReorderGroups {
            ids: vec![id.into()],
            before,
        }))
    }
    pub fn plan_physics_authoring(&self, intent: PhysicsEdit) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(EditPreview::prepare(
            &self.session,
            "Edit physics",
            None,
            |c| c.edit_physics(intent),
        )?)
    }
    pub fn physics_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<PhysicsAuthoringCapabilities> {
        ensure!(
            matches!(
                object.kind,
                ObjectKind::Physics
                    | ObjectKind::PhysicsGroup
                    | ObjectKind::PhysicsInput
                    | ObjectKind::PhysicsOutput
                    | ObjectKind::PhysicsNode
            ),
            "Expected an authorable physics object"
        );

        Ok(PhysicsAuthoringCapabilities {
            domain: self.model().physics_capabilities(&object.id)?,
            session_reason: self
                .pending
                .is_some()
                .then_some(AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn execute_physics_authoring(
        &mut self,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
        }
        self.consume_authoring_receipt(&receipt, None);
        Ok(self.authoring_result(receipt).with_preview(plan))
    }
}
impl crate::Editor {
    pub fn plan_physics_authoring(
        &self,
        document: crate::DocumentId,
        intent: PhysicsEdit,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_physics_authoring(intent)
    }
    pub fn execute_physics_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_physics_authoring(plan))
    }
}

#[derive(Clone, Debug)]
pub enum PhysicsChannelAction {
    CreateInput,
    CreateOutput,
    Delete(ObjectRef),
    Step { object: ObjectRef, down: bool },
}
impl Document {
    pub fn physics_channel_capability(
        &self,
        group: &str,
        action: &PhysicsChannelAction,
    ) -> Result<zipzi_contract::authoring::CapabilityState> {
        use zipzi_contract::authoring::CapabilityState as C;
        let (object, kind) = match action {
            PhysicsChannelAction::CreateInput | PhysicsChannelAction::CreateOutput => (
                ObjectRef::new(ObjectKind::PhysicsGroup, group),
                AuthoringAction::CreateChild,
            ),
            PhysicsChannelAction::Delete(object) | PhysicsChannelAction::Step { object, .. } => {
                anyhow::ensure!(
                    matches!(
                        object.kind,
                        ObjectKind::PhysicsInput | ObjectKind::PhysicsOutput
                    ),
                    AuthoringPlanningError::new("WRONG_OBJECT_KIND")
                );
                (
                    object.clone(),
                    if matches!(action, PhysicsChannelAction::Delete(_)) {
                        AuthoringAction::Delete
                    } else {
                        AuthoringAction::Reorder
                    },
                )
            }
        };
        let state = self.authoring_capabilities(&object)?.state(kind);
        if state != C::Available {
            return Ok(state);
        }
        if matches!(action, PhysicsChannelAction::Step { .. })
            && self.physics_channel_edit(group, action)?.is_none()
        {
            return Ok(C::Unavailable { code: "NO_CHANGE" });
        }
        Ok(state)
    }
    pub fn physics_channel_edit(
        &self,
        group: &str,
        action: &PhysicsChannelAction,
    ) -> Result<Option<PhysicsEdit>> {
        let model_read_guard_50 = self.model();
        let rig = model_read_guard_50
            .physics_group(group)
            .ok_or_else(|| anyhow::anyhow!("Physics group no longer exists"))?;
        Ok(Some(match action {
            PhysicsChannelAction::CreateInput => PhysicsEdit::CreateInput {
                group_id: group.into(),
                id: Uuid::new_v4().to_string(),
                before: None,
            },
            PhysicsChannelAction::CreateOutput => PhysicsEdit::CreateOutput {
                group_id: group.into(),
                id: Uuid::new_v4().to_string(),
                node_id: rig.vertices.get(1).unwrap_or(&rig.vertices[0]).id.clone(),
                before: None,
            },
            PhysicsChannelAction::Delete(object) => {
                anyhow::ensure!(
                    matches!(
                        object.kind,
                        ObjectKind::PhysicsInput | ObjectKind::PhysicsOutput
                    ),
                    AuthoringPlanningError::new("WRONG_OBJECT_KIND")
                );
                self.model()
                    .require_object(object.kind.sdk_kind().unwrap(), &object.id)?;
                PhysicsEdit::Delete {
                    ids: vec![object.id.clone()],
                    node_replacements: Default::default(),
                }
            }
            PhysicsChannelAction::Step { object, down } => {
                let collection = match object.kind {
                    ObjectKind::PhysicsInput => {
                        zipzi_contract::authoring::PhysicsCollection::Inputs {
                            group_id: group.into(),
                        }
                    }
                    ObjectKind::PhysicsOutput => {
                        zipzi_contract::authoring::PhysicsCollection::Outputs {
                            group_id: group.into(),
                        }
                    }
                    _ => anyhow::bail!("Expected a Physics input or output"),
                };
                let ids = self.model().physics_collection(collection)?;
                let at = ids
                    .iter()
                    .position(|h| h == &object.id)
                    .ok_or_else(|| anyhow::anyhow!("Physics channel changed"))?;
                if (!down && at == 0) || (*down && at + 1 >= ids.len()) {
                    return Ok(None);
                }
                let before = ids.get(if *down { at + 2 } else { at - 1 }).cloned();
                if object.kind == ObjectKind::PhysicsInput {
                    PhysicsEdit::MoveInputs {
                        ids: vec![object.id.clone()],
                        group_id: group.into(),
                        before,
                    }
                } else {
                    PhysicsEdit::MoveOutputs {
                        ids: vec![object.id.clone()],
                        group_id: group.into(),
                        before,
                        node_replacements: Default::default(),
                    }
                }
            }
        }))
    }
    pub fn physics_append_node(&self, group: &str) -> Result<PhysicsEdit> {
        let model_read_guard_51 = self.model();
        let rig = model_read_guard_51
            .physics_group(group)
            .ok_or_else(|| anyhow::anyhow!("Physics group no longer exists"))?;
        let mut node = rig.vertices.last().unwrap().clone();
        node.id = Uuid::new_v4().to_string();
        node.radius = node.radius.max(1.);
        node.position.y += node.radius;
        Ok(PhysicsEdit::InsertNode {
            group_id: group.into(),
            node,
            before: None,
        })
    }
}
