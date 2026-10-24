//! Session coordination only. Collection rules and copy/delete closures live in SDK.
use super::*;
use anyhow::Result;
use zipzi_sdk::{
    OrganizationAddress, OrganizationCapabilities, OrganizationEdit, OrganizationEditPlan,
};
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
    ) -> Result<OrganizationEditPlan> {
        self.check_parameter_version(expected)?;
        let policy = match object.kind {
            ObjectKind::Parameter => zipzi_sdk::OrganizationDeletePolicy::Contents,
            ObjectKind::ParameterGroup => zipzi_sdk::OrganizationDeletePolicy::Dissolve,
            _ => anyhow::bail!("Expected a parameter or folder"),
        };
        self.session
            .handle(object.kind.sdk_kind().unwrap(), &object.id)?;
        self.plan_organization_authoring(OrganizationEdit::Delete {
            ids: vec![object.id.clone()],
            policy,
        })
        .map_err(|error| {
            if let Some(native) = error.downcast_ref::<zipzi_sdk::SdkError>() {
                if native.code.as_ref() == "OBJECT_REFERENCED" {
                    return crate::ParameterManagementError::Referenced(
                        native.referrers.iter().cloned().collect(),
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
            ObjectKind::Parameter => zipzi_sdk::ObjectKind::Parameter,
            ObjectKind::ParameterGroup => zipzi_sdk::ObjectKind::CdiParameterGroup,
            ObjectKind::Part => zipzi_sdk::ObjectKind::Part,
            ObjectKind::Mesh => zipzi_sdk::ObjectKind::Mesh,
            ObjectKind::Deformer => zipzi_sdk::ObjectKind::Transform,
            ObjectKind::Offscreen => zipzi_sdk::ObjectKind::Offscreen,
            _ => anyhow::bail!("Expected an organization object"),
        };
        Ok(OrganizationAuthoringCapabilities {
            domain: self
                .session
                .organization_capabilities(&self.session.handle(kind, &object.id)?)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn organization_address(&self, id: &str) -> Result<OrganizationAddress> {
        Ok(self.session.organization_address(id)?)
    }
    pub fn plan_organization_authoring(
        &self,
        intent: OrganizationEdit,
    ) -> Result<OrganizationEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_organization_edit(intent)?)
    }
    pub fn execute_organization_authoring(
        &mut self,
        plan: &OrganizationEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_organization_plan(plan)?;
        if receipt.changed {
            self.pristine_blank = None;
            let scene_changed = receipt
                .changes
                .created
                .iter()
                .chain(&receipt.changes.deleted)
                .any(|e| {
                    matches!(
                        e.kind,
                        zipzi_sdk::ObjectKind::Part
                            | zipzi_sdk::ObjectKind::Mesh
                            | zipzi_sdk::ObjectKind::Transform
                            | zipzi_sdk::ObjectKind::Offscreen
                    )
                })
                || receipt.changes.organization_references.iter().any(|r| {
                    matches!(
                        r.entity.kind,
                        zipzi_sdk::ObjectKind::Part
                            | zipzi_sdk::ObjectKind::Mesh
                            | zipzi_sdk::ObjectKind::Transform
                            | zipzi_sdk::ObjectKind::Offscreen
                    )
                });
            self.consume_authoring_receipt(&receipt, scene_changed);
            self.reconcile_parameter_preview()?;
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn plan_organization_authoring(
        &self,
        id: crate::DocumentId,
        intent: OrganizationEdit,
    ) -> Result<OrganizationEditPlan> {
        self.ensure_authoring_target(id, "Organization")?;
        self.document().unwrap().plan_organization_authoring(intent)
    }
    pub fn execute_organization_authoring(
        &mut self,
        id: crate::DocumentId,
        plan: &OrganizationEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Organization")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_organization_authoring(plan))
    }
}
