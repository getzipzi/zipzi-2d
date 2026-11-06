//! Session coordination only. Collection rules and copy/delete closures live in SDK.
use super::*;
use anyhow::Result;
use zipzi_contract::authoring::OrganizationCapabilities;
use zipzi_contract::document::OrganizationEdit;
#[derive(Debug, Clone)]
pub struct OrganizationAuthoringCapabilities {
    pub domain: OrganizationCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}

impl Document {
    pub fn plan_parameter_object_deletion(
        &self,
        object: &ObjectRef,
        expected: u64,
    ) -> Result<crate::EditPreview<zipzi_core::state::OrganizationOutcome>> {
        self.check_parameter_version(expected)?;
        let policy = match object.kind {
            ObjectKind::Parameter => zipzi_contract::document::OrganizationDeletePolicy::Contents,
            ObjectKind::ParameterGroup => {
                zipzi_contract::document::OrganizationDeletePolicy::Dissolve
            }
            _ => anyhow::bail!("Expected a parameter or folder"),
        };
        self.model()
            .require_object(object.kind.sdk_kind().unwrap(), &object.id)?;
        self.plan_organization_authoring(OrganizationEdit::Delete {
            ids: vec![object.id.clone()],
            policy,
        })
        .map_err(|error| {
            if let Some(native) = error.downcast_ref::<zipzi_core::Status>() {
                if native.code == "OBJECT_REFERENCED" {
                    return crate::ParameterManagementError::Referenced(
                        self.model().references_to(&object.id),
                    )
                    .into();
                }
            }
            error
        })
    }
    pub fn organization_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<OrganizationAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Parameter => zipzi_contract::authoring::ObjectKind::Parameter,
            ObjectKind::ParameterGroup => zipzi_contract::authoring::ObjectKind::CdiParameterGroup,
            ObjectKind::Part => zipzi_contract::authoring::ObjectKind::Part,
            ObjectKind::Mesh => zipzi_contract::authoring::ObjectKind::Mesh,
            ObjectKind::Deformer => zipzi_contract::authoring::ObjectKind::Transform,
            ObjectKind::Offscreen => zipzi_contract::authoring::ObjectKind::Offscreen,
            _ => anyhow::bail!("Expected an organization object"),
        };
        Ok(OrganizationAuthoringCapabilities {
            domain: self.model().organization_capabilities(kind, &object.id)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn plan_organization_authoring(
        &self,
        intent: OrganizationEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::OrganizationOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit organization",
            None,
            |commands| commands.edit_organization(intent),
        )?)
    }
    pub fn execute_organization_authoring(
        &mut self,
        plan: &crate::EditPreview<zipzi_core::state::OrganizationOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, Some(plan.baseline().document()));
            self.reconcile_parameter_preview()?;
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt).with_preview(plan))
    }
}
impl crate::Editor {
    pub fn plan_organization_authoring(
        &self,
        id: crate::DocumentId,
        intent: OrganizationEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::OrganizationOutcome>> {
        self.ensure_authoring_target(id, "Organization")?;
        self.document().unwrap().plan_organization_authoring(intent)
    }
    pub fn execute_organization_authoring(
        &mut self,
        id: crate::DocumentId,
        plan: &crate::EditPreview<zipzi_core::state::OrganizationOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Organization")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_organization_authoring(plan))
    }
}
