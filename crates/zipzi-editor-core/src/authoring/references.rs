//! Editor session checks around the native reference authoring rules.
use super::*;
use anyhow::Result;
use zipzi_contract::authoring::ReferenceCapabilities;
use zipzi_contract::document::ReferenceCollection;
use zipzi_contract::document::ReferenceEdit;
use zipzi_contract::document::ReferenceFields;
use zipzi_contract::document::ReferenceSelectionField;

#[derive(Debug, Clone)]
pub struct ReferenceAuthoringCapabilities {
    pub domain: ReferenceCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
fn editor_kind(k: zipzi_contract::authoring::ObjectKind) -> Option<ObjectKind> {
    Some(match k {
        zipzi_contract::authoring::ObjectKind::ModelParameterGroup => {
            ObjectKind::ModelParameterGroup
        }
        zipzi_contract::authoring::ObjectKind::ModelParameterMember => {
            ObjectKind::ModelParameterMember
        }
        zipzi_contract::authoring::ObjectKind::ModelHitArea => ObjectKind::ModelHitArea,
        zipzi_contract::authoring::ObjectKind::CdiCombinedSet => ObjectKind::CdiCombinedSet,
        zipzi_contract::authoring::ObjectKind::CdiCombinedMember => ObjectKind::CdiCombinedMember,
        _ => return None,
    })
}
impl Document {
    pub fn reference_collection(&self, c: &ReferenceCollection) -> Result<Vec<ObjectRef>> {
        let model = self.model();
        Ok(model
            .reference_collection(c)?
            .into_iter()
            .filter_map(|id| {
                model
                    .reference_object_kind(&id)
                    .and_then(editor_kind)
                    .map(|kind| ObjectRef::new(kind, id))
            })
            .collect())
    }

    pub fn reference_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<ReferenceAuthoringCapabilities> {
        let k = match object.kind {
            ObjectKind::ModelParameterGroup => {
                zipzi_contract::authoring::ObjectKind::ModelParameterGroup
            }
            ObjectKind::ModelParameterMember => {
                zipzi_contract::authoring::ObjectKind::ModelParameterMember
            }
            ObjectKind::ModelHitArea => zipzi_contract::authoring::ObjectKind::ModelHitArea,
            ObjectKind::CdiCombinedSet => zipzi_contract::authoring::ObjectKind::CdiCombinedSet,
            ObjectKind::CdiCombinedMember => {
                zipzi_contract::authoring::ObjectKind::CdiCombinedMember
            }
            _ => anyhow::bail!("Expected a reference object"),
        };
        Ok(ReferenceAuthoringCapabilities {
            domain: self.model().reference_capabilities(k, &object.id)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn reference_field_address(
        &self,
        id: &str,
    ) -> Result<crate::FieldCapture<ReferenceFields>> {
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let value = captured.snapshot.document().reference_fields(id)?;
        Ok(crate::FieldCapture {
            source,
            id: id.into(),
            owner: None,
            value,
        })
    }
    pub fn reference_selection_address(
        &self,
        owner: &str,
        field: ReferenceSelectionField,
    ) -> Result<crate::FieldCapture<(ReferenceSelectionField, Vec<String>)>> {
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let value = captured
            .snapshot
            .document()
            .reference_selection(owner, field)?
            .to_vec();
        Ok(crate::FieldCapture {
            source,
            id: owner.into(),
            owner: None,
            value: (field, value),
        })
    }
    pub fn plan_reference_field_patch(
        &self,
        a: &crate::FieldCapture<ReferenceFields>,
        f: ReferenceFields,
    ) -> Result<crate::EditPreview<zipzi_core::state::ReferenceOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit reference",
            Some(a.source),
            |commands| {
                commands.edit_reference(ReferenceEdit::Patch {
                    id: a.id.clone(),
                    expected: a.value.clone(),
                    fields: f,
                })
            },
        )?)
    }
    pub fn plan_reference_selection_patch(
        &self,
        a: &crate::FieldCapture<(ReferenceSelectionField, Vec<String>)>,
        targets: Vec<String>,
    ) -> Result<crate::EditPreview<zipzi_core::state::ReferenceOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit reference selection",
            Some(a.source),
            |commands| {
                commands.edit_reference(ReferenceEdit::ReplaceSelection {
                    owner: a.id.clone(),
                    field: a.value.0,
                    expected: a.value.1.clone(),
                    targets,
                })
            },
        )?)
    }
    pub fn plan_reference_authoring(
        &self,
        e: ReferenceEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::ReferenceOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit reference",
            None,
            |commands| commands.edit_reference(e),
        )?)
    }
    pub fn execute_reference_authoring(
        &mut self,
        p: &crate::EditPreview<zipzi_core::state::ReferenceOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = p.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, false);
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt).with_preview(p))
    }
}
impl crate::Editor {
    pub fn plan_reference_authoring(
        &self,
        id: crate::DocumentId,
        e: ReferenceEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::ReferenceOutcome>> {
        self.ensure_authoring_target(id, "Reference")?;
        self.document().unwrap().plan_reference_authoring(e)
    }
    pub fn execute_reference_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &crate::EditPreview<zipzi_core::state::ReferenceOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Reference")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_reference_authoring(p))
    }
}
