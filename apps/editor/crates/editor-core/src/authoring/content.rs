//! Session coordination around the SDK's dedicated content rules.
use super::*;
use anyhow::Result;
use kasane_sdk::{
    SceneContentAddress, SceneContentCapabilities, SceneContentEdit, SceneContentEditPlan,
    SceneContentTarget, SceneContentValue,
};
#[derive(Debug, Clone)]
pub struct SceneContentAuthoringCapabilities {
    pub domain: SceneContentCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
impl Document {
    pub fn scene_content_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<SceneContentAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Mesh => kasane_sdk::ObjectKind::Mesh,
            ObjectKind::Deformer => kasane_sdk::ObjectKind::Transform,
            ObjectKind::Glue => kasane_sdk::ObjectKind::Glue,
            ObjectKind::GlueVertexPair => kasane_sdk::ObjectKind::GlueVertexPair,
            ObjectKind::Offscreen => kasane_sdk::ObjectKind::Offscreen,
            _ => anyhow::bail!("Expected a content owner"),
        };
        Ok(SceneContentAuthoringCapabilities {
            domain: self
                .session
                .scene_content_capabilities(&self.session.handle(kind, &object.id)?)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn scene_content_address(
        &self,
        owner: &str,
        target: SceneContentTarget,
    ) -> Result<SceneContentAddress> {
        Ok(self.session.scene_content_address(owner, target)?)
    }
    pub fn plan_scene_content_patch(
        &self,
        address: &SceneContentAddress,
        value: SceneContentValue,
    ) -> Result<SceneContentEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_scene_content_patch(address, value)?)
    }
    pub fn plan_scene_content(&self, intent: SceneContentEdit) -> Result<SceneContentEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_scene_content(intent)?)
    }
    pub fn execute_scene_content(
        &mut self,
        plan: &SceneContentEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_scene_content_plan(plan)?;
        if receipt.changed {
            self.pristine_blank = false;
            self.consume_authoring_receipt(&receipt, true);
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn plan_scene_content(
        &self,
        id: crate::DocumentId,
        intent: SceneContentEdit,
    ) -> Result<SceneContentEditPlan> {
        self.ensure_authoring_target(id, "Content")?;
        self.document().unwrap().plan_scene_content(intent)
    }
    pub fn execute_scene_content(
        &mut self,
        id: crate::DocumentId,
        plan: &SceneContentEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Content")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_scene_content(plan))
    }
}
