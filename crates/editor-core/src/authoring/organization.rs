//! Session coordination only. Collection rules and copy/delete closures live in SDK.
use super::*;
use anyhow::Result;
use kasane_sdk::{
    OrganizationAddress, OrganizationCapabilities, OrganizationEdit, OrganizationEditPlan,
};
#[derive(Debug, Clone)]
pub struct OrganizationAuthoringCapabilities {
    pub domain: OrganizationCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
fn editor_kind(k: kasane_sdk::ObjectKind) -> Option<ObjectKind> {
    Some(match k {
        kasane_sdk::ObjectKind::Parameter => ObjectKind::Parameter,
        kasane_sdk::ObjectKind::CdiParameterGroup => ObjectKind::ParameterGroup,
        kasane_sdk::ObjectKind::Part => ObjectKind::Part,
        kasane_sdk::ObjectKind::Mesh => ObjectKind::Mesh,
        kasane_sdk::ObjectKind::Transform => ObjectKind::Deformer,
        kasane_sdk::ObjectKind::Offscreen => ObjectKind::Offscreen,
        _ => return None,
    })
}
impl Document {
    pub fn organization_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<OrganizationAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Parameter => kasane_sdk::ObjectKind::Parameter,
            ObjectKind::ParameterGroup => kasane_sdk::ObjectKind::CdiParameterGroup,
            ObjectKind::Part => kasane_sdk::ObjectKind::Part,
            ObjectKind::Mesh => kasane_sdk::ObjectKind::Mesh,
            ObjectKind::Deformer => kasane_sdk::ObjectKind::Transform,
            ObjectKind::Offscreen => kasane_sdk::ObjectKind::Offscreen,
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
        let suggested_selection = receipt
            .changes
            .created
            .iter()
            .filter_map(|e| editor_kind(e.kind).map(|k| ObjectRef::new(k, &e.id)))
            .collect();
        let locations = receipt
            .changes
            .organization_references
            .iter()
            .filter_map(|r| {
                r.after.as_ref().map(|id| {
                    ObjectRef::new(
                        if matches!(
                            r.entity.kind,
                            kasane_sdk::ObjectKind::Parameter
                                | kasane_sdk::ObjectKind::CdiParameterGroup
                        ) {
                            ObjectKind::ParameterGroup
                        } else {
                            ObjectKind::Part
                        },
                        id,
                    )
                })
            })
            .collect();
        if receipt.changed {
            self.pristine_blank = false;
            let scene_changed = receipt
                .changes
                .created
                .iter()
                .chain(&receipt.changes.deleted)
                .any(|e| {
                    matches!(
                        e.kind,
                        kasane_sdk::ObjectKind::Part
                            | kasane_sdk::ObjectKind::Mesh
                            | kasane_sdk::ObjectKind::Transform
                            | kasane_sdk::ObjectKind::Offscreen
                    )
                })
                || receipt.changes.organization_references.iter().any(|r| {
                    matches!(
                        r.entity.kind,
                        kasane_sdk::ObjectKind::Part
                            | kasane_sdk::ObjectKind::Mesh
                            | kasane_sdk::ObjectKind::Transform
                            | kasane_sdk::ObjectKind::Offscreen
                    )
                });
            self.consume_authoring_receipt(&receipt, scene_changed);
            self.reconcile_parameter_preview()?;
            self.context.reconcile(&self.object_index());
        }
        Ok(OperationResult {
            receipt,
            suggested_selection,
            locations,
        })
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
