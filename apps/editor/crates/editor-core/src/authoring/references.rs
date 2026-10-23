//! Editor session checks around the native reference authoring rules.
use super::*;
use anyhow::Result;
use kasane_sdk::{
    ReferenceCapabilities, ReferenceCollection, ReferenceEdit, ReferenceEditPlan,
    ReferenceFieldAddress, ReferenceFields, ReferenceSelectionAddress, ReferenceSelectionField,
};
#[derive(Debug, Clone)]
pub struct ReferenceAuthoringCapabilities {
    pub domain: ReferenceCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
fn editor_kind(k: kasane_sdk::ObjectKind) -> Option<ObjectKind> {
    Some(match k {
        kasane_sdk::ObjectKind::ModelParameterGroup => ObjectKind::ModelParameterGroup,
        kasane_sdk::ObjectKind::ModelParameterMember => ObjectKind::ModelParameterMember,
        kasane_sdk::ObjectKind::ModelHitArea => ObjectKind::ModelHitArea,
        kasane_sdk::ObjectKind::CdiCombinedSet => ObjectKind::CdiCombinedSet,
        kasane_sdk::ObjectKind::CdiCombinedMember => ObjectKind::CdiCombinedMember,
        _ => return None,
    })
}
impl Document {
    pub fn reference_collection(&self, c: &ReferenceCollection) -> Result<Vec<ObjectRef>> {
        Ok(self
            .session
            .reference_collection(c)?
            .iter()
            .filter_map(|h| editor_kind(h.kind()).map(|k| ObjectRef::new(k, h.id())))
            .collect())
    }
    pub fn reference_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<ReferenceAuthoringCapabilities> {
        let k = match object.kind {
            ObjectKind::ModelParameterGroup => kasane_sdk::ObjectKind::ModelParameterGroup,
            ObjectKind::ModelParameterMember => kasane_sdk::ObjectKind::ModelParameterMember,
            ObjectKind::ModelHitArea => kasane_sdk::ObjectKind::ModelHitArea,
            ObjectKind::CdiCombinedSet => kasane_sdk::ObjectKind::CdiCombinedSet,
            ObjectKind::CdiCombinedMember => kasane_sdk::ObjectKind::CdiCombinedMember,
            _ => anyhow::bail!("Expected a reference object"),
        };
        Ok(ReferenceAuthoringCapabilities {
            domain: self
                .session
                .reference_capabilities(&self.session.handle(k, &object.id)?)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn reference_field_address(&self, id: &str) -> Result<ReferenceFieldAddress> {
        Ok(self.session.reference_field_address(id)?)
    }
    pub fn reference_selection_address(
        &self,
        owner: &str,
        field: ReferenceSelectionField,
    ) -> Result<ReferenceSelectionAddress> {
        Ok(self.session.reference_selection_address(owner, field)?)
    }
    pub fn plan_reference_field_patch(
        &self,
        a: &ReferenceFieldAddress,
        f: ReferenceFields,
    ) -> Result<ReferenceEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_reference_field_patch(a, f)?)
    }
    pub fn plan_reference_selection_patch(
        &self,
        a: &ReferenceSelectionAddress,
        targets: Vec<String>,
    ) -> Result<ReferenceEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_reference_selection_patch(a, targets)?)
    }
    pub fn plan_reference_authoring(&self, e: ReferenceEdit) -> Result<ReferenceEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_reference_edit(e)?)
    }
    pub fn execute_reference_authoring(
        &mut self,
        p: &ReferenceEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_reference_plan(p)?;
        if receipt.changed {
            self.pristine_blank = false;
            self.consume_authoring_receipt(&receipt, false);
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn plan_reference_authoring(
        &self,
        id: crate::DocumentId,
        e: ReferenceEdit,
    ) -> Result<ReferenceEditPlan> {
        self.ensure_authoring_target(id, "Reference")?;
        self.document().unwrap().plan_reference_authoring(e)
    }
    pub fn execute_reference_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &ReferenceEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Reference")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_reference_authoring(p))
    }
}
