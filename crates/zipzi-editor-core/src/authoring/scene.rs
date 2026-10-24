//! Editor session coordination; scene rules and copy closures live in the SDK.
use super::*;
use anyhow::Result;
use zipzi_sdk::{
    DrawingGroupAddress, SceneCapabilities, SceneEdit, SceneEditPlan, SceneField,
    SceneFieldAddress, SceneFieldValue,
};
#[derive(Debug, Clone)]
pub struct SceneAuthoringCapabilities {
    pub domain: SceneCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}

impl Document {
    pub fn scene_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<SceneAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Mesh => zipzi_sdk::ObjectKind::Mesh,
            ObjectKind::Deformer => zipzi_sdk::ObjectKind::Transform,
            ObjectKind::Offscreen => zipzi_sdk::ObjectKind::Offscreen,
            ObjectKind::Glue => zipzi_sdk::ObjectKind::Glue,
            ObjectKind::DrawOrderGroup => zipzi_sdk::ObjectKind::DrawOrderGroup,
            _ => anyhow::bail!("Expected a scene object or drawing group"),
        };
        Ok(SceneAuthoringCapabilities {
            domain: self
                .session
                .scene_capabilities(&self.session.handle(kind, &object.id)?)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn scene_field_address(&self, id: &str, field: SceneField) -> Result<SceneFieldAddress> {
        Ok(self.session.scene_field_address(id, field)?)
    }
    pub fn drawing_group_address(&self, owner: &str) -> Result<DrawingGroupAddress> {
        Ok(self.session.drawing_group_address(owner)?)
    }
    pub fn plan_scene_field_patch(
        &self,
        address: &SceneFieldAddress,
        value: SceneFieldValue,
        clear_pairs: bool,
    ) -> Result<SceneEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self
            .session
            .plan_scene_field_patch(address, value, clear_pairs)?)
    }
    pub fn plan_drawing_range_patch(
        &self,
        address: &DrawingGroupAddress,
        range: [i32; 2],
    ) -> Result<SceneEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_drawing_range_patch(address, range)?)
    }
    pub fn plan_scene_authoring(&self, intent: SceneEdit) -> Result<SceneEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_scene_edit(intent)?)
    }
    pub fn execute_scene_authoring(&mut self, plan: &SceneEditPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_scene_plan(plan)?;
        if receipt.changed {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, true);
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn plan_scene_authoring(
        &self,
        id: crate::DocumentId,
        intent: SceneEdit,
    ) -> Result<SceneEditPlan> {
        self.ensure_authoring_target(id, "Scene")?;
        self.document().unwrap().plan_scene_authoring(intent)
    }
    pub fn execute_scene_authoring(
        &mut self,
        id: crate::DocumentId,
        plan: &SceneEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Scene")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_scene_authoring(plan))
    }
}
