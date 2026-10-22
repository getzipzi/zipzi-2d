//! Session coordination around the SDK's dedicated content rules.
use super::*;
use anyhow::{Result, ensure};
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
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self.session.plan_scene_content_patch(address, value)?)
    }
    pub fn plan_scene_content(&self, intent: SceneContentEdit) -> Result<SceneContentEditPlan> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self.session.plan_scene_content(intent)?)
    }
    pub fn execute_scene_content(
        &mut self,
        plan: &SceneContentEditPlan,
    ) -> Result<OperationResult> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        let receipt = self.session.execute_scene_content_plan(plan)?;
        if receipt.changed {
            self.pristine_blank = false;
            self.consume_authoring_receipt(&receipt, true);
            self.context.reconcile(&self.object_index());
        }
        let suggested_selection = receipt
            .changes
            .created
            .iter()
            .filter(|e| e.kind == kasane_sdk::ObjectKind::GlueVertexPair)
            .map(|e| ObjectRef::new(ObjectKind::GlueVertexPair, &e.id))
            .collect();
        Ok(OperationResult {
            receipt,
            suggested_selection,
            locations: vec![],
        })
    }
}
impl crate::Editor {
    fn ensure_content_target(&self, id: crate::DocumentId) -> Result<()> {
        ensure!(
            self.active_id() == Some(id) && !self.is_busy() && !self.exiting(),
            "Content context changed"
        );
        ensure!(
            self.active_session()
                .unwrap()
                .animation
                .timeline
                .edit
                .is_none(),
            "Finish the active Timeline gesture first"
        );
        Ok(())
    }
    pub fn plan_scene_content(
        &self,
        id: crate::DocumentId,
        intent: SceneContentEdit,
    ) -> Result<SceneContentEditPlan> {
        self.ensure_content_target(id)?;
        self.document().unwrap().plan_scene_content(intent)
    }
    pub fn execute_scene_content(
        &mut self,
        id: crate::DocumentId,
        plan: &SceneContentEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_content_target(id)?;
        let session = self.active_session_mut().unwrap();
        let result = session.document.execute_scene_content(plan)?;
        session.reconcile_animation();
        Ok(result)
    }
}
