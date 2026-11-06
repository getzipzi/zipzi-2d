//! Session coordination around the SDK's dedicated content rules.
use super::*;
use anyhow::Result;
use zipzi_contract::authoring::SceneContentCapabilities;
use zipzi_contract::document::SceneContentEdit;
use zipzi_contract::document::SceneContentTarget;
use zipzi_contract::document::SceneContentValue;
#[derive(Debug, Clone)]
pub struct SceneContentAuthoringCapabilities {
    pub domain: SceneContentCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
impl Document {
    pub fn scene_content_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<SceneContentAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Mesh => zipzi_contract::authoring::ObjectKind::Mesh,
            ObjectKind::Deformer => zipzi_contract::authoring::ObjectKind::Transform,
            ObjectKind::Glue => zipzi_contract::authoring::ObjectKind::Glue,
            ObjectKind::GlueVertexPair => zipzi_contract::authoring::ObjectKind::GlueVertexPair,
            ObjectKind::Offscreen => zipzi_contract::authoring::ObjectKind::Offscreen,
            _ => anyhow::bail!("Expected a content owner"),
        };
        Ok(SceneContentAuthoringCapabilities {
            domain: self.model().scene_content_capabilities(kind, &object.id)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn scene_content_address(
        &self,
        owner: &str,
        target: SceneContentTarget,
    ) -> Result<
        crate::FieldCapture<(
            zipzi_contract::document::SceneContentTarget,
            zipzi_contract::document::SceneContentValue,
        )>,
    > {
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let value = captured
            .snapshot
            .document()
            .scene_content_value(owner, &target)?;
        Ok(crate::FieldCapture {
            source,
            id: owner.into(),
            owner: None,
            value: (target, value),
        })
    }
    pub fn plan_scene_content_patch(
        &self,
        address: &crate::FieldCapture<(
            zipzi_contract::document::SceneContentTarget,
            zipzi_contract::document::SceneContentValue,
        )>,
        value: SceneContentValue,
    ) -> Result<crate::EditPreview<zipzi_core::state::SceneContentOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit scene content",
            Some(address.source),
            |commands| {
                commands.patch_scene_content(
                    &address.id,
                    &address.value.0,
                    address.value.1.clone(),
                    value,
                    Default::default(),
                    &Default::default(),
                )
            },
        )?)
    }
    pub fn plan_scene_content(
        &self,
        intent: SceneContentEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::SceneContentOutcome>> {
        self.ensure_authoring_idle()?;
        let source = self.session.capture_authoring();
        let limits = Default::default();
        Ok(crate::EditPreview::convert(
            &source,
            "Edit scene content",
            limits,
            &Default::default(),
            move |commands, control| commands.edit_scene_content(intent, limits, control),
        )?)
    }
    pub fn execute_scene_content(
        &mut self,
        plan: &crate::EditPreview<zipzi_core::state::SceneContentOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, None);
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt).with_preview(plan))
    }
}
impl crate::Editor {
    pub fn plan_scene_content(
        &self,
        id: crate::DocumentId,
        intent: SceneContentEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::SceneContentOutcome>> {
        self.ensure_authoring_target(id, "Content")?;
        self.document().unwrap().plan_scene_content(intent)
    }
    pub fn execute_scene_content(
        &mut self,
        id: crate::DocumentId,
        plan: &crate::EditPreview<zipzi_core::state::SceneContentOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Content")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_scene_content(plan))
    }
}
