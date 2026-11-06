//! Editor adapters attach selection hints without activating playback.
use super::*;
use anyhow::Result;
use zipzi_core::document::{AttachmentEdit, MotionEdit};
impl Document {
    pub fn plan_motion_authoring(&self, intent: MotionEdit) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(EditPreview::prepare(
            &self.session,
            "Edit motion",
            None,
            |c| c.edit_motion(intent),
        )?)
    }
    pub fn execute_motion_authoring(
        &mut self,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        Ok(self
            .motion_operation_result(receipt, plan.baseline().document())
            .with_preview(plan))
    }
    pub fn plan_attachment_authoring(
        &self,
        intent: AttachmentEdit,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(EditPreview::prepare(
            &self.session,
            "Edit attachment",
            None,
            |c| c.edit_attachment(intent),
        )?)
    }
    pub fn execute_attachment_authoring(
        &mut self,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        Ok(self
            .motion_operation_result(receipt, plan.baseline().document())
            .with_preview(plan))
    }
    fn motion_operation_result(
        &mut self,
        receipt: zipzi_sdk::Publication,
        before: &zipzi_core::state::DocumentState,
    ) -> OperationResult {
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
        }
        self.consume_authoring_receipt(&receipt, Some(before));
        self.authoring_result(receipt)
    }
}

impl crate::Editor {
    pub fn plan_motion_authoring(
        &self,
        document: crate::DocumentId,
        intent: MotionEdit,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_motion_authoring(intent)
    }
    pub fn execute_motion_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_motion_authoring(plan))
    }
}
