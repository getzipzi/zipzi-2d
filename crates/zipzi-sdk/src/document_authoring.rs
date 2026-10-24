use crate::{AuthoringSession, CapabilityState, EditReceipt, EditSession, SdkError, Version};
pub use zipzi_core::document::{DocumentAuthoringEdit, DocumentField, DocumentValue};
#[derive(Debug, Clone)]
pub struct DocumentEditPlan {
    version: Version,
    domain: zipzi_core::document::DocumentPlan,
}
impl DocumentEditPlan {
    pub fn losses(&self) -> &[String] {
        self.domain.losses()
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn is_noop(&self) -> bool {
        self.domain.is_noop()
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
}
#[derive(Debug, Clone)]
pub struct DocumentFieldAddress {
    version: Version,
    expected: DocumentValue,
    epoch: u64,
}
impl AuthoringSession {
    pub fn document_field_capability(&self, field: DocumentField) -> CapabilityState {
        match field {
            DocumentField::Layout | DocumentField::ExportSettings => CapabilityState::Available,
            _ => CapabilityState::Unsupported {
                code: "SOURCE_METADATA_READ_ONLY",
            },
        }
    }
    pub fn document_field_address(
        &self,
        field: DocumentField,
    ) -> Result<DocumentFieldAddress, SdkError> {
        Ok(DocumentFieldAddress {
            version: self.version(),
            expected: self
                .document()
                .document_value(field)
                .map_err(|s| SdkError::from_status(s, "document_field_address", vec![]))?,
            epoch: *self.document_epochs.get(&field).unwrap_or(&0),
        })
    }
    pub fn plan_document_field_patch(
        &self,
        a: &DocumentFieldAddress,
        value: DocumentValue,
    ) -> Result<DocumentEditPlan, SdkError> {
        if self.version().session_id != a.version.session_id
            || self.version().generation != a.version.generation
            || *self.document_epochs.get(&a.expected.field()).unwrap_or(&0) != a.epoch
        {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Document field changed",
                "plan_document_field_patch",
            ));
        }
        self.plan_document_edit(DocumentAuthoringEdit::Patch {
            expected: a.expected.clone(),
            value,
        })
    }
    pub fn plan_document_edit(
        &self,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentEditPlan, SdkError> {
        Ok(DocumentEditPlan {
            version: self.version(),
            domain: self
                .document()
                .plan_document_edit(intent)
                .map_err(|s| SdkError::from_status(s, "plan_document_edit", vec![]))?,
        })
    }
    pub fn execute_document_plan(&mut self, p: &DocumentEditPlan) -> Result<EditReceipt, SdkError> {
        self.edit("Edit document", Some(p.version), |e| {
            e.apply_document_plan(p)
        })
        .map(|(_, r)| r)
    }
}
impl EditSession<'_> {
    pub fn apply_document_plan(&mut self, p: &DocumentEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(
            p.version,
            "apply_document_plan",
            "Replan document properties",
        )?;
        self.apply_authoring_change("apply_document_plan", "", true, |document| {
            document.apply_document_plan(&p.domain)
        })
    }
    pub fn edit_document(&mut self, intent: DocumentAuthoringEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_document")?;
        let p = self
            .candidate_document()
            .plan_document_edit(intent)
            .map_err(|s| self.abort_with(SdkError::from_status(s, "edit_document", vec![])))?;
        self.apply_authoring_change("edit_document", "", true, |document| {
            document.apply_document_plan(&p)
        })
    }
}
