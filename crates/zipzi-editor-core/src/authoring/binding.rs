//! Editor session checks compose with the SDK's conversion rules and history.
use super::*;
use anyhow::{Result, ensure};
use zipzi_contract::authoring::BindingAuthoringEdit;

pub(super) fn apply_binding(
    commands: &mut zipzi_sdk::Commands<'_>,
    intent: BindingAuthoringEdit,
    limits: zipzi_core::state::ConversionLimits,
    control: &zipzi_core::state::ConversionControl,
) -> Result<zipzi_core::state::ConversionOutcome, zipzi_core::Status> {
    match intent {
        BindingAuthoringEdit::Grid(edit) => commands.edit_binding_grid(edit, limits, control),
        BindingAuthoringEdit::Blend(edit) => commands.edit_blend_shape(edit, limits, control),
        BindingAuthoringEdit::ParameterRange(edit) => {
            commands.edit_parameter_range(edit, limits, control)
        }
        BindingAuthoringEdit::Reparent(edit) => commands
            .edit_scene(
                zipzi_contract::document::SceneEdit::Reparent { edit },
                limits,
                control,
            )?
            .conversion
            .ok_or_else(|| {
                zipzi_core::Status::error("CONVERSION_REPORT", "Reparent produced no report")
            }),
    }
}
impl Document {
    pub fn reparent_support(
        &self,
        edit: &zipzi_contract::document::ReparentEdit,
    ) -> Result<zipzi_core::document::ReparentSupport> {
        self.ensure_authoring_idle()?;
        Ok(self.session.reparent_support(edit)?)
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        intent: BindingAuthoringEdit,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<crate::EditPreview<zipzi_core::state::ConversionOutcome>> {
        self.ensure_authoring_idle()?;
        let source = self.session.capture_authoring();
        let limits = zipzi_core::state::ConversionLimits::default();
        Ok(crate::EditPreview::convert(
            &source,
            "Edit binding",
            limits,
            control,
            move |commands, control| apply_binding(commands, intent, limits, control),
        )?)
    }
    pub fn plan_binding_authoring(
        &self,
        intent: BindingAuthoringEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::ConversionOutcome>> {
        self.ensure_authoring_idle()?;
        self.plan_binding_authoring_with_control(intent, &Default::default())
    }
    pub fn execute_binding_authoring(
        &mut self,
        plan: &crate::EditPreview<zipzi_core::state::ConversionOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
        }
        self.consume_authoring_receipt(&receipt, None);
        Ok(self.authoring_result(receipt).with_preview(plan))
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
    ) -> Result<crate::EditPreview<zipzi_core::state::ConversionOutcome>> {
        self.ensure_binding_authoring_target(document)?;
        self.document().unwrap().plan_binding_authoring(intent)
    }
    pub fn reparent_support(
        &self,
        document: crate::DocumentId,
        edit: &zipzi_contract::document::ReparentEdit,
    ) -> Result<zipzi_core::document::ReparentSupport> {
        self.ensure_binding_authoring_target(document)?;
        self.document().unwrap().reparent_support(edit)
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        document: crate::DocumentId,
        intent: BindingAuthoringEdit,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<crate::EditPreview<zipzi_core::state::ConversionOutcome>> {
        self.ensure_binding_authoring_target(document)?;
        self.document()
            .unwrap()
            .plan_binding_authoring_with_control(intent, control)
    }
    pub fn execute_binding_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &crate::EditPreview<zipzi_core::state::ConversionOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_binding_authoring_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_binding_authoring(plan))
    }
}
