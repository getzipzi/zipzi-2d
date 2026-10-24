//! Editor adapters attach selection hints without activating playback.
use super::*;
use anyhow::Result;
use zipzi_core::document::{AttachmentEdit, MotionEdit};
impl Document {
    pub fn plan_motion_authoring(&self, intent: MotionEdit) -> Result<zipzi_sdk::MotionEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_motion_edit(intent)?)
    }
    pub fn execute_motion_authoring(
        &mut self,
        plan: &zipzi_sdk::MotionEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_motion_plan(plan)?;
        Ok(self.motion_operation_result(receipt))
    }
    pub fn plan_attachment_authoring(
        &self,
        intent: AttachmentEdit,
    ) -> Result<zipzi_sdk::AttachmentEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_attachment_edit(intent)?)
    }
    pub fn execute_attachment_authoring(
        &mut self,
        plan: &zipzi_sdk::AttachmentEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_attachment_plan(plan)?;
        Ok(self.motion_operation_result(receipt))
    }
    fn motion_operation_result(&mut self, receipt: zipzi_sdk::EditReceipt) -> OperationResult {
        if receipt.changed {
            self.pristine_blank = None;
        }
        self.consume_authoring_receipt(&receipt, false);
        self.authoring_result(receipt)
    }
}

impl crate::Editor {
    pub fn plan_motion_authoring(
        &self,
        document: crate::DocumentId,
        intent: MotionEdit,
    ) -> Result<zipzi_sdk::MotionEditPlan> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_motion_authoring(intent)
    }
    pub fn execute_motion_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &zipzi_sdk::MotionEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_motion_authoring(plan))
    }
}
