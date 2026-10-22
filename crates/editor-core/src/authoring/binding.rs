//! Editor session checks compose with the SDK's conversion rules and history.
use super::*;
use anyhow::{Result, ensure};
use kasane_sdk::{BindingAuthoringEdit, BindingEditPlan, ObjectKind as SdkKind};
impl Document {
    pub fn binding_field_address(&self, id: &str) -> Result<kasane_sdk::BindingFieldAddress> {
        Ok(self.session.binding_field_address(id)?)
    }
    pub fn plan_binding_field_patch(
        &self,
        address: &kasane_sdk::BindingFieldAddress,
        patch: kasane_sdk::BindingFieldPatch,
    ) -> Result<BindingEditPlan> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self.session.plan_binding_field_patch(address, patch)?)
    }
    pub fn reparent_support(
        &self,
        edit: &kasane_sdk::ReparentEdit,
    ) -> Result<kasane_sdk::ReparentSupport> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self.session.reparent_support(edit)?)
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        intent: BindingAuthoringEdit,
        control: &kasane_sdk::ConversionControl,
    ) -> Result<BindingEditPlan> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self
            .session
            .plan_binding_authoring_with_control(intent, control)?)
    }
    pub fn plan_binding_authoring(&self, intent: BindingAuthoringEdit) -> Result<BindingEditPlan> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(self.session.plan_binding_authoring(intent)?)
    }
    pub fn execute_binding_authoring(&mut self, plan: &BindingEditPlan) -> Result<OperationResult> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        let receipt = self.session.execute_binding_plan(plan)?;
        let selection: Vec<_> = receipt
            .changes
            .created
            .iter()
            .filter_map(|entity| {
                Some(ObjectRef::new(
                    match entity.kind {
                        SdkKind::MeshBinding => ObjectKind::MeshBinding,
                        SdkKind::SceneBinding => ObjectKind::SceneBinding,
                        SdkKind::BindingAxis => ObjectKind::BindingAxis,
                        SdkKind::BindingSample => ObjectKind::BindingSample,
                        SdkKind::BlendKeyTable => ObjectKind::BlendKeyTable,
                        SdkKind::BlendConstraint => ObjectKind::BlendConstraint,
                        SdkKind::BlendBinding => ObjectKind::BlendBinding,
                        _ => return None,
                    },
                    &entity.id,
                ))
            })
            .collect();
        if receipt.changed {
            self.pristine_blank = false;
        }
        self.consume_authoring_receipt(&receipt, true);
        Ok(OperationResult {
            receipt,
            suggested_selection: selection.clone(),
            locations: selection,
        })
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
        edit: &kasane_sdk::ReparentEdit,
    ) -> Result<kasane_sdk::ReparentSupport> {
        self.ensure_binding_authoring_target(document)?;
        self.document().unwrap().reparent_support(edit)
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        document: crate::DocumentId,
        intent: BindingAuthoringEdit,
        control: &kasane_sdk::ConversionControl,
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
        let session = self.active_session_mut().unwrap();
        ensure!(
            session.animation.timeline.edit.is_none(),
            "Finish the active Timeline gesture first"
        );
        let result = session.document.execute_binding_authoring(plan)?;
        session.reconcile_animation();
        Ok(result)
    }
}
