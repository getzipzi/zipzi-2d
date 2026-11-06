//! Thin Editor adapter; Pose ownership and execution rules stay in SDK/core.
use super::*;
use anyhow::{Result, ensure};
use zipzi_contract::authoring::PoseCapabilities;
use zipzi_contract::document::PoseEdit;
use zipzi_contract::document::PoseFields;

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
        ensure!(
            matches!(
                object.kind,
                ObjectKind::Pose
                    | ObjectKind::PoseGroup
                    | ObjectKind::PoseEntry
                    | ObjectKind::PoseLink
            ),
            "Expected an authorable pose object"
        );

        Ok(PoseAuthoringCapabilities {
            domain: self.model().pose_capabilities(&object.id)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn plan_pose_authoring(&self, intent: PoseEdit) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(EditPreview::prepare(
            &self.session,
            "Edit pose",
            None,
            |c| c.edit_pose(intent),
        )?)
    }
    pub fn execute_pose_authoring(
        &mut self,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, Some(plan.baseline().document()));
        }
        Ok(self.authoring_result(receipt).with_preview(plan))
    }
    pub fn pose_field_address(&self, id: &str) -> Result<crate::FieldCapture<PoseFields>> {
        let source = self.session.sourced_snapshot();
        let value = source
            .snapshot
            .document()
            .pose_fields(id)
            .ok_or_else(|| zipzi_sdk::Status::error("MISSING_POSE_OBJECT", id))?;
        Ok(FieldCapture {
            source: match source.source {
                zipzi_sdk::QuerySource::Published(v) => v,
                _ => unreachable!(),
            },
            id: id.into(),
            owner: None,
            value,
        })
    }
    pub fn plan_pose_field_patch(
        &self,
        address: &crate::FieldCapture<PoseFields>,
        fields: PoseFields,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(EditPreview::prepare(
            &self.session,
            "Edit Pose fields",
            Some(address.source),
            |c| {
                c.edit_pose(PoseEdit::Patch {
                    id: address.id.clone(),
                    expected: address.value.clone(),
                    fields,
                })
            },
        )?)
    }
}
impl crate::Editor {
    pub fn plan_pose_authoring(
        &self,
        document: crate::DocumentId,
        intent: PoseEdit,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_pose_authoring(intent)
    }
    pub fn execute_pose_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_pose_authoring(plan))
    }
}
