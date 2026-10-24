//! Editor session checks compose with the SDK's conversion rules and history.
use super::*;
use anyhow::{Result, ensure};
use zipzi_sdk::{BindingAuthoringEdit, BindingEditPlan};
impl Document {
    pub fn binding_field_address(&self, id: &str) -> Result<zipzi_sdk::BindingFieldAddress> {
        Ok(self.session.binding_field_address(id)?)
    }
    pub fn plan_binding_field_patch(
        &self,
        address: &zipzi_sdk::BindingFieldAddress,
        patch: zipzi_sdk::BindingFieldPatch,
    ) -> Result<BindingEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_binding_field_patch(address, patch)?)
    }
    pub fn reparent_support(
        &self,
        edit: &zipzi_sdk::ReparentEdit,
    ) -> Result<zipzi_sdk::ReparentSupport> {
        self.ensure_authoring_idle()?;
        Ok(self.session.reparent_support(edit)?)
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        intent: BindingAuthoringEdit,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<BindingEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self
            .session
            .plan_binding_authoring_with_control(intent, control)?)
    }
    pub fn plan_binding_authoring(&self, intent: BindingAuthoringEdit) -> Result<BindingEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_binding_authoring(intent)?)
    }
    pub fn execute_binding_authoring(&mut self, plan: &BindingEditPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_binding_plan(plan)?;
        if receipt.changed {
            self.pristine_blank = None;
        }
        self.consume_authoring_receipt(&receipt, true);
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    fn ensure_binding_authoring_target(&self, document: crate::DocumentId) -> Result<()> {
        ensure!(
            self.editing_context() == Some(crate::EditingContext::Modeling)
                && self.active_id() == Some(document)
                && !self.is_busy()
                && !self.exiting(),
            "Modeling context changed"
        );
        Ok(())
    }
    pub fn plan_binding_authoring(
        &self,
        document: crate::DocumentId,
        intent: BindingAuthoringEdit,
    ) -> Result<BindingEditPlan> {
        self.ensure_binding_authoring_target(document)?;
        self.document().unwrap().plan_binding_authoring(intent)
    }
    pub fn reparent_support(
        &self,
        document: crate::DocumentId,
        edit: &zipzi_sdk::ReparentEdit,
    ) -> Result<zipzi_sdk::ReparentSupport> {
        self.ensure_binding_authoring_target(document)?;
        self.document().unwrap().reparent_support(edit)
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        document: crate::DocumentId,
        intent: BindingAuthoringEdit,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<BindingEditPlan> {
        self.ensure_binding_authoring_target(document)?;
        self.document()
            .unwrap()
            .plan_binding_authoring_with_control(intent, control)
    }
    pub fn execute_binding_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &BindingEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_binding_authoring_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_binding_authoring(plan))
    }
}
