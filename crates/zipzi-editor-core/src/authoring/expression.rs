//! Editor session adapter for the SDK's expression domain operations.
use super::*;
use anyhow::{Result, ensure};
use zipzi_sdk::{
    ExpressionEdit, ExpressionEditPlan, ExpressionEntryAddress, ExpressionEntryFields,
};

impl Document {
    pub fn expression_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<ExpressionAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Expression => zipzi_sdk::ObjectKind::Expression,
            ObjectKind::ExpressionEntry => zipzi_sdk::ObjectKind::ExpressionEntry,
            _ => anyhow::bail!("Expected an expression or entry"),
        };
        let handle = self.session.handle(kind, &object.id)?;
        Ok(ExpressionAuthoringCapabilities {
            domain: self.session.expression_capabilities(&handle)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn plan_expression_authoring(&self, intent: ExpressionEdit) -> Result<ExpressionEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_expression_edit(intent)?)
    }
    pub fn execute_expression_authoring(
        &mut self,
        plan: &ExpressionEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_expression_plan(plan)?;
        self.expression_operation_result(receipt)
    }
    pub fn expression_entry_address(&self, id: &str) -> Result<ExpressionEntryAddress> {
        Ok(self.session.expression_entry_address(id)?)
    }
    pub fn begin_expression_entry_edit(
        &mut self,
        address: &ExpressionEntryAddress,
    ) -> Result<ExpressionEntryEditToken> {
        self.ensure_authoring_idle()?;
        self.session
            .plan_expression_entry_patch(address, address.expected().clone())?;
        let token = ExpressionEntryEditToken(Uuid::new_v4());
        self.pending = Some(PendingEdit {
            kind: Gesture::ExpressionEntry(token.0),
            original: PendingOriginal::ExpressionEntry(address.clone()),
            edit: self
                .session
                .begin_owned_edit("Edit expression entry", None)?
                .interactive()?,
            preview: PreviewState::default(),
            changed: OnceLock::new(),
            topology_changed: false,
            draw_groups_changed: false,
            property_write: None,
        });
        Ok(token)
    }

    pub(in crate::document) fn expression_edit_token(&self) -> Option<ExpressionEntryEditToken> {
        self.pending
            .as_ref()
            .and_then(|pending| match pending.kind {
                Gesture::ExpressionEntry(id) => Some(ExpressionEntryEditToken(id)),
                _ => None,
            })
    }

    pub(crate) fn has_inspector_edit(&self) -> bool {
        self.property_edit_serial().is_some() || self.expression_edit_token().is_some()
    }

    pub(in crate::document) fn finish_context_edit(&mut self) -> Result<()> {
        if self.expression_edit_token().is_some() {
            self.cancel_edit();
            Ok(())
        } else {
            self.finish_edit()
        }
    }

    pub fn preview_expression_entry_edit(
        &mut self,
        token: ExpressionEntryEditToken,
        fields: ExpressionEntryFields,
    ) -> Result<()> {
        ensure!(
            self.expression_edit_token() == Some(token),
            "Expression edit expired"
        );
        let pending = self.pending.as_mut().unwrap();
        let PendingOriginal::ExpressionEntry(address) = &pending.original else {
            unreachable!()
        };
        let owner = address.owner_id().to_owned();
        let previous_target = pending
            .edit
            .candidate_document()
            .get_expression_entry(address.entry_id())
            .unwrap()
            .1
            .target
            .clone();
        let references_changed = previous_target != fields.target;
        pending
            .edit
            .preview_expression_entry(address.entry_id(), fields)?;
        pending.changed.take();
        let previous_revision = self.projection_revision;
        self.invalidate_expression_projection();
        if !references_changed {
            self.refresh_expression_index(previous_revision, &[owner]);
        }
        Ok(())
    }

    pub fn plan_expression_entry_patch(
        &self,
        address: &ExpressionEntryAddress,
        fields: ExpressionEntryFields,
    ) -> Result<ExpressionEditPlan> {
        Ok(self.session.plan_expression_entry_patch(address, fields)?)
    }

    pub fn commit_expression_entry_edit(
        &mut self,
        token: ExpressionEntryEditToken,
    ) -> Result<OperationResult> {
        ensure!(
            self.expression_edit_token() == Some(token),
            "Expression edit expired"
        );
        let pending = self.pending.take().unwrap();
        let PendingOriginal::ExpressionEntry(address) = &pending.original else {
            unreachable!()
        };
        let owner = address.owner_id().to_owned();
        let references_changed = pending
            .edit
            .candidate_document()
            .get_expression_entry(address.entry_id())
            .unwrap()
            .1
            .target
            != address.expected().target;
        let result = pending.edit.commit_to(&mut self.session);
        if !result.as_ref().is_ok_and(|receipt| receipt.changed) {
            let previous_revision = self.projection_revision;
            self.invalidate_expression_projection();
            if !references_changed {
                self.refresh_expression_index(previous_revision, &[owner]);
            }
        }
        self.expression_operation_result(result?)
    }

    fn expression_operation_result(
        &mut self,
        receipt: zipzi_sdk::EditReceipt,
    ) -> Result<OperationResult> {
        if receipt.changed {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, false);
        }
        Ok(self.authoring_result(receipt))
    }
}
