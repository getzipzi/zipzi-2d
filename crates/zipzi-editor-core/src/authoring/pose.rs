//! Thin Editor adapter; Pose ownership and execution rules stay in SDK/core.
use super::*;
use anyhow::Result;
use zipzi_sdk::{PoseCapabilities, PoseEdit, PoseEditPlan, PoseFieldAddress, PoseFields};
#[derive(Debug, Clone)]
pub struct PoseAuthoringCapabilities {
    pub domain: PoseCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
impl Document {
    pub fn pose_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<PoseAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Pose => zipzi_sdk::ObjectKind::Pose,
            ObjectKind::PoseGroup => zipzi_sdk::ObjectKind::PoseGroup,
            ObjectKind::PoseEntry => zipzi_sdk::ObjectKind::PoseEntry,
            ObjectKind::PoseLink => zipzi_sdk::ObjectKind::PoseLink,
            _ => anyhow::bail!("Expected a Pose object"),
        };
        let handle = self.session.handle(kind, &object.id)?;
        Ok(PoseAuthoringCapabilities {
            domain: self.session.pose_capabilities(&handle)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn plan_pose_authoring(&self, intent: PoseEdit) -> Result<PoseEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_pose_edit(intent)?)
    }
    pub fn execute_pose_authoring(&mut self, plan: &PoseEditPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_pose_plan(plan)?;
        if receipt.changed {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, false);
        }
        Ok(self.authoring_result(receipt))
    }
    pub fn pose_field_address(&self, id: &str) -> Result<PoseFieldAddress> {
        Ok(self.session.pose_field_address(id)?)
    }
    pub fn plan_pose_field_patch(
        &self,
        address: &PoseFieldAddress,
        fields: PoseFields,
    ) -> Result<PoseEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_pose_field_patch(address, fields)?)
    }
}
impl crate::Editor {
    pub fn plan_pose_authoring(
        &self,
        document: crate::DocumentId,
        intent: PoseEdit,
    ) -> Result<PoseEditPlan> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_pose_authoring(intent)
    }
    pub fn execute_pose_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &PoseEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_pose_authoring(plan))
    }
}
