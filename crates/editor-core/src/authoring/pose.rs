//! Thin Editor adapter; Pose ownership and execution rules stay in SDK/core.
use super::*;
use anyhow::Result;
use kasane_sdk::{PoseCapabilities, PoseEdit, PoseEditPlan, PoseFieldAddress, PoseFields};
#[derive(Debug, Clone)]
pub struct PoseAuthoringCapabilities {
    pub domain: PoseCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
impl Document {
    pub fn pose_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<PoseAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Pose => kasane_sdk::ObjectKind::Pose,
            ObjectKind::PoseGroup => kasane_sdk::ObjectKind::PoseGroup,
            ObjectKind::PoseEntry => kasane_sdk::ObjectKind::PoseEntry,
            ObjectKind::PoseLink => kasane_sdk::ObjectKind::PoseLink,
            _ => anyhow::bail!("Expected a Pose object"),
        };
        let handle = self.session.handle(kind, &object.id)?;
        Ok(PoseAuthoringCapabilities {
            domain: self.session.pose_capabilities(&handle)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn plan_pose_authoring(&self, intent: PoseEdit) -> Result<PoseEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_pose_edit(intent)?)
    }
    pub fn execute_pose_authoring(&mut self, plan: &PoseEditPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_pose_plan(plan)?;
        let map = |kind| match kind {
            kasane_sdk::ObjectKind::Pose => Some(ObjectKind::Pose),
            kasane_sdk::ObjectKind::PoseGroup => Some(ObjectKind::PoseGroup),
            kasane_sdk::ObjectKind::PoseEntry => Some(ObjectKind::PoseEntry),
            kasane_sdk::ObjectKind::PoseLink => Some(ObjectKind::PoseLink),
            _ => None,
        };
        let suggested_selection = receipt
            .changes
            .created
            .iter()
            .filter_map(|e| map(e.kind).map(|k| ObjectRef::new(k, &e.id)))
            .collect();
        let locations = receipt
            .changes
            .collections
            .iter()
            .filter_map(|c| match c {
                kasane_sdk::CollectionRef::Pose {
                    owner: Some(id), ..
                } => self.model().pose_fields(id).map(|f| {
                    ObjectRef::new(
                        match f {
                            PoseFields::Asset { .. } => ObjectKind::Pose,
                            PoseFields::Group { .. } => ObjectKind::PoseGroup,
                            PoseFields::Entry { .. } => ObjectKind::PoseEntry,
                            PoseFields::Link { .. } => ObjectKind::PoseLink,
                        },
                        id,
                    )
                }),
                _ => None,
            })
            .collect();
        if receipt.changed {
            self.pristine_blank = false;
            self.consume_authoring_receipt(&receipt, false);
        }
        Ok(OperationResult {
            receipt,
            suggested_selection,
            locations,
        })
    }
    pub fn pose_field_address(&self, id: &str) -> Result<PoseFieldAddress> {
        Ok(self.session.pose_field_address(id)?)
    }
    pub fn plan_pose_field_patch(
        &self,
        address: &PoseFieldAddress,
        fields: PoseFields,
    ) -> Result<PoseEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_pose_field_patch(address, fields)?)
    }
}
impl crate::Editor {
    pub fn plan_pose_authoring(
        &self,
        document: crate::DocumentId,
        intent: PoseEdit,
    ) -> Result<PoseEditPlan> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_pose_authoring(intent)
    }
    pub fn execute_pose_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &PoseEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_pose_authoring(plan))
    }
}
