//! Editor session coordination; scene rules and copy closures live in the SDK.
use super::*;
use anyhow::Result;
use zipzi_contract::authoring::SceneCapabilities;
use zipzi_contract::document::SceneEdit;
use zipzi_contract::document::SceneField;
use zipzi_contract::document::SceneFieldValue;

#[derive(Debug, Clone)]
pub struct SceneAuthoringCapabilities {
    pub domain: SceneCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}

impl Document {
    pub fn scene_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<SceneAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Mesh => zipzi_contract::authoring::ObjectKind::Mesh,
            ObjectKind::Deformer => zipzi_contract::authoring::ObjectKind::Transform,
            ObjectKind::Offscreen => zipzi_contract::authoring::ObjectKind::Offscreen,
            ObjectKind::Glue => zipzi_contract::authoring::ObjectKind::Glue,
            ObjectKind::DrawOrderGroup => zipzi_contract::authoring::ObjectKind::DrawOrderGroup,
            _ => anyhow::bail!("Expected a scene object or drawing group"),
        };
        Ok(SceneAuthoringCapabilities {
            domain: self.model().scene_capabilities(kind, &object.id)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn scene_field_address(
        &self,
        id: &str,
        field: SceneField,
    ) -> Result<crate::FieldCapture<zipzi_contract::document::SceneFieldValue>> {
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let value = captured.snapshot.document().scene_field(id, field)?;
        Ok(crate::FieldCapture {
            source,
            id: id.into(),
            owner: None,
            value,
        })
    }
    pub fn drawing_group_address(&self, owner: &str) -> Result<crate::FieldCapture<[i32; 2]>> {
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let document = captured.snapshot.document();
        let owner = if owner == document.id() { "" } else { owner };
        let value = document.drawing_group_range(owner)?;
        Ok(crate::FieldCapture {
            source,
            id: owner.into(),
            owner: None,
            value,
        })
    }
    pub fn plan_scene_field_patch(
        &self,
        address: &crate::FieldCapture<zipzi_contract::document::SceneFieldValue>,
        value: SceneFieldValue,
        clear_pairs: bool,
    ) -> Result<crate::EditPreview<zipzi_core::state::SceneOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit scene field",
            Some(address.source),
            |commands| {
                commands.edit_scene(
                    SceneEdit::Patch {
                        id: address.id.clone(),
                        expected: address.value.clone(),
                        value,
                        clear_glue_pairs: clear_pairs,
                    },
                    Default::default(),
                    &Default::default(),
                )
            },
        )?)
    }
    pub fn plan_drawing_range_patch(
        &self,
        address: &crate::FieldCapture<[i32; 2]>,
        range: [i32; 2],
    ) -> Result<crate::EditPreview<zipzi_core::state::SceneOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit drawing range",
            Some(address.source),
            |commands| {
                commands.edit_scene(
                    SceneEdit::SetDrawingRange {
                        owner: address.id.clone(),
                        expected: address.value,
                        range,
                    },
                    Default::default(),
                    &Default::default(),
                )
            },
        )?)
    }
    pub fn plan_scene_authoring(
        &self,
        intent: SceneEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::SceneOutcome>> {
        self.ensure_authoring_idle()?;
        let source = self.session.capture_authoring();
        let limits = Default::default();
        Ok(crate::EditPreview::convert(
            &source,
            "Edit scene",
            limits,
            &Default::default(),
            move |commands, control| commands.edit_scene(intent, limits, control),
        )?)
    }
    pub fn execute_scene_authoring(
        &mut self,
        plan: &crate::EditPreview<zipzi_core::state::SceneOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, Some(plan.baseline().document()));
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt).with_preview(plan))
    }
}
impl crate::Editor {
    pub fn plan_scene_authoring(
        &self,
        id: crate::DocumentId,
        intent: SceneEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::SceneOutcome>> {
        self.ensure_authoring_target(id, "Scene")?;
        self.document().unwrap().plan_scene_authoring(intent)
    }
    pub fn execute_scene_authoring(
        &mut self,
        id: crate::DocumentId,
        plan: &crate::EditPreview<zipzi_core::state::SceneOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Scene")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_scene_authoring(plan))
    }
}
