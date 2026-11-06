//! Editor session adapter for the SDK's expression domain operations.
use super::*;
use anyhow::{Result, ensure};
use zipzi_contract::document::ExpressionEdit;
use zipzi_contract::document::ExpressionEntryFields;

impl Document {
    pub fn expression_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<ExpressionAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Expression => zipzi_contract::authoring::ObjectKind::Expression,
            ObjectKind::ExpressionEntry => zipzi_contract::authoring::ObjectKind::ExpressionEntry,
            _ => anyhow::bail!("Expected an expression or entry"),
        };

        Ok(ExpressionAuthoringCapabilities {
            domain: self.model().expression_capabilities(&object.id)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn plan_expression_authoring(
        &self,
        intent: ExpressionEdit,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(EditPreview::prepare(
            &self.session,
            "Edit expression",
            None,
            |c| c.edit_expression(intent),
        )?)
    }
    pub fn execute_expression_authoring(
        &mut self,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        self.expression_operation_result(receipt)
            .map(|result| result.with_preview(plan))
    }
    pub fn expression_entry_address(
        &self,
        id: &str,
    ) -> Result<crate::FieldCapture<ExpressionEntryFields>> {
        let source = self.session.sourced_snapshot();
        let (owner, entry) = source
            .snapshot
            .document()
            .expression_entry(id)
            .ok_or_else(|| zipzi_sdk::Status::error("MISSING_EXPRESSION_ENTRY", id))?;
        Ok(FieldCapture {
            source: match source.source {
                zipzi_sdk::QuerySource::Published(v) => v,
                _ => unreachable!(),
            },
            id: id.into(),
            owner: Some(owner.id.clone()),
            value: entry.into(),
        })
    }
    pub fn begin_expression_entry_edit(
        &mut self,
        address: &crate::FieldCapture<ExpressionEntryFields>,
    ) -> Result<ExpressionEntryEditToken> {
        self.ensure_authoring_idle()?;
        ensure!(
            self.session.version() == address.source,
            "Expression source changed"
        );
        let token = ExpressionEntryEditToken(Uuid::new_v4());
        self.pending = Some(PendingEdit {
            kind: Gesture::ExpressionEntry(token.0),
            original: PendingOriginal::ExpressionEntry(address.clone()),
            edit: self
                .session
                .begin_draft("Edit expression entry", Some(address.source))?
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
        let owner = address
            .owner
            .as_deref()
            .expect("expression owner")
            .to_owned();
        let previous_target = pending
            .edit
            .candidate_document()
            .expression_entry(&address.id)
            .unwrap()
            .1
            .target
            .clone();
        let references_changed = previous_target != fields.target;
        let expected = pending
            .edit
            .candidate_document()
            .expression_entry(&address.id)
            .map(|(_, entry)| ExpressionEntryFields::from(entry))
            .ok_or_else(|| zipzi_sdk::Status::error("MISSING_EXPRESSION_ENTRY", &address.id))?;
        pending.edit.preview(|c| {
            c.edit_expression(ExpressionEdit::PatchEntry {
                id: address.id.clone(),
                expected,
                fields,
            })
        })?;
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
        address: &crate::FieldCapture<ExpressionEntryFields>,
        fields: ExpressionEntryFields,
    ) -> Result<crate::EditPreview<()>> {
        Ok(EditPreview::prepare(
            &self.session,
            "Edit expression entry",
            Some(address.source),
            |c| {
                c.edit_expression(ExpressionEdit::PatchEntry {
                    id: address.id.clone(),
                    expected: address.value.clone(),
                    fields,
                })
            },
        )?)
    }

    pub fn commit_expression_entry_edit(
        &mut self,
        token: ExpressionEntryEditToken,
    ) -> Result<OperationResult> {
        ensure!(
            self.expression_edit_token() == Some(token),
            "Expression edit expired"
        );
        let mut pending = self.pending.take().unwrap();
        let PendingOriginal::ExpressionEntry(address) = &pending.original else {
            unreachable!()
        };
        let owner = address
            .owner
            .as_deref()
            .expect("expression owner")
            .to_owned();
        let references_changed = pending
            .edit
            .candidate_document()
            .expression_entry(&address.id)
            .unwrap()
            .1
            .target
            != address.value.target;
        let result = pending.edit.commit_to(&mut self.session);
        if result.is_err() && pending.edit.is_active() {
            self.pending = Some(pending);
            return Err(result.unwrap_err().into());
        }
        if !result
            .as_ref()
            .is_ok_and(|receipt| (receipt.kind != zipzi_sdk::PublicationKind::Unchanged))
        {
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
        receipt: zipzi_sdk::Publication,
    ) -> Result<OperationResult> {
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, false);
        }
        Ok(self.authoring_result(receipt))
    }
}
