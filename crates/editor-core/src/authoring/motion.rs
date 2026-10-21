//! Editor adapters attach selection hints without activating playback.
use super::*;
use anyhow::{Result, ensure};
use kasane_core::document::{AttachmentEdit, MotionEdit};
impl Document {
    pub fn plan_motion_authoring(&self, intent: MotionEdit) -> Result<kasane_sdk::MotionEditPlan> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self.session.plan_motion_edit(intent)?)
    }
    pub fn execute_motion_authoring(
        &mut self,
        plan: &kasane_sdk::MotionEditPlan,
    ) -> Result<OperationResult> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        let receipt = self.session.execute_motion_plan(plan)?;
        Ok(self.motion_operation_result(receipt))
    }
    pub fn plan_attachment_authoring(
        &self,
        intent: AttachmentEdit,
    ) -> Result<kasane_sdk::AttachmentEditPlan> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self.session.plan_attachment_edit(intent)?)
    }
    pub fn execute_attachment_authoring(
        &mut self,
        plan: &kasane_sdk::AttachmentEditPlan,
    ) -> Result<OperationResult> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        let receipt = self.session.execute_attachment_plan(plan)?;
        Ok(self.motion_operation_result(receipt))
    }
    fn motion_operation_result(&mut self, receipt: kasane_sdk::EditReceipt) -> OperationResult {
        let selection = receipt
            .changes
            .created
            .iter()
            .filter_map(|e| {
                let kind = match e.kind {
                    kasane_sdk::ObjectKind::Motion => ObjectKind::Motion,
                    kasane_sdk::ObjectKind::MotionGroup => ObjectKind::MotionGroup,
                    kasane_sdk::ObjectKind::MotionRegistration => ObjectKind::MotionRegistration,
                    kasane_sdk::ObjectKind::MotionTrack => ObjectKind::MotionTrack,
                    kasane_sdk::ObjectKind::MotionKey => ObjectKind::MotionKey,
                    kasane_sdk::ObjectKind::MotionEvent => ObjectKind::MotionEvent,
                    kasane_sdk::ObjectKind::Attachment => ObjectKind::Attachment,
                    _ => return None,
                };
                Some(ObjectRef::new(kind, &e.id))
            })
            .collect::<Vec<_>>();
        if receipt.changed {
            self.pristine_blank = false;
        }
        self.consume_authoring_receipt(&receipt, false);
        OperationResult {
            receipt,
            suggested_selection: selection.clone(),
            locations: selection,
        }
    }
}

impl crate::Editor {
    pub fn plan_motion_authoring(
        &self,
        document: crate::DocumentId,
        intent: MotionEdit,
    ) -> Result<kasane_sdk::MotionEditPlan> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_motion_authoring(intent)
    }
    pub fn execute_motion_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &kasane_sdk::MotionEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        let session = self.active_session_mut().unwrap();
        ensure!(
            session.animation.timeline.edit.is_none(),
            "Finish the active Timeline gesture first"
        );
        let result = session.document.execute_motion_authoring(plan)?;
        session.reconcile_animation();
        Ok(result)
    }
}
