//! Resource loading and metadata plans attach editor context to SDK authority.
use super::*;
use anyhow::{Result, ensure};
use kasane_sdk::{
    AttachmentLoadToken, DocumentAuthoringEdit, DocumentEditPlan, ImageLoadToken, ResourceEdit,
    ResourceEditPlan,
};
#[derive(Debug, Clone)]
pub struct ResourceAuthoringCapabilities {
    pub domain: kasane_sdk::ResourceCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
impl Document {
    fn ensure_resource_idle(&self) -> Result<()> {
        ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(())
    }
    pub fn resource_collection(&self) -> Result<Vec<ObjectRef>> {
        Ok(self
            .session
            .resource_collection()?
            .iter()
            .map(|h| ObjectRef::new(ObjectKind::Texture, h.id()))
            .collect())
    }
    pub fn resource_capabilities(&self, id: &str) -> Result<ResourceAuthoringCapabilities> {
        Ok(ResourceAuthoringCapabilities {
            domain: self
                .session
                .resource_capabilities(&self.session.handle(kasane_sdk::ObjectKind::Asset, id)?)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn resource_collection_capabilities(
        &self,
    ) -> (
        kasane_sdk::ResourceCollectionCapabilities,
        Option<AuthoringSessionReason>,
    ) {
        (
            self.session.resource_collection_capabilities(),
            self.pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        )
    }
    pub fn attachment_path_address(&self, id: &str) -> Result<kasane_sdk::AttachmentPathAddress> {
        Ok(self.session.attachment_path_address(id)?)
    }
    pub fn plan_attachment_path_patch(
        &self,
        a: &kasane_sdk::AttachmentPathAddress,
        path: String,
    ) -> Result<kasane_sdk::AttachmentEditPlan> {
        self.ensure_resource_idle()?;
        Ok(self.session.plan_attachment_path_patch(a, path)?)
    }
    pub fn document_field_capability(
        &self,
        f: kasane_sdk::DocumentField,
    ) -> (kasane_sdk::CapabilityState, Option<AuthoringSessionReason>) {
        (
            self.session.document_field_capability(f),
            self.pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        )
    }
    pub fn plan_resource_authoring(&self, intent: ResourceEdit) -> Result<ResourceEditPlan> {
        self.ensure_resource_idle()?;
        Ok(self.session.plan_resource_edit(intent)?)
    }
    pub fn execute_resource_authoring(&mut self, p: &ResourceEditPlan) -> Result<OperationResult> {
        self.ensure_resource_idle()?;
        let mut edit = self
            .session
            .begin_owned_edit("Edit resource", Some(p.version()))?;
        edit.apply_resource_plan(p)?;
        self.commit_resource_edit(edit)
    }
    pub fn resource_name_address(&self, id: &str) -> Result<kasane_sdk::ResourceNameAddress> {
        Ok(self.session.resource_name_address(id)?)
    }
    pub fn plan_resource_name_patch(
        &self,
        a: &kasane_sdk::ResourceNameAddress,
        name: String,
    ) -> Result<ResourceEditPlan> {
        self.ensure_resource_idle()?;
        Ok(self.session.plan_resource_name_patch(a, name)?)
    }
    pub fn image_load_token(&self, id: &str) -> Result<ImageLoadToken> {
        self.ensure_resource_idle()?;
        Ok(self.session.image_load_token(id)?)
    }
    pub fn replace_image_content(
        &mut self,
        t: &ImageLoadToken,
        c: Option<kasane_core::ImageContent>,
    ) -> Result<OperationResult> {
        self.ensure_resource_idle()?;
        let edit = self.session.begin_image_content_edit(t, c)?;
        self.commit_resource_edit(edit)
    }
    pub fn replace_prepared_image_content(
        &mut self,
        t: &ImageLoadToken,
        image: &kasane_sdk::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_resource_idle()?;
        let receipt = self.session.replace_prepared_image_content(t, image)?;
        self.refresh_textures()?;
        Ok(self.resource_operation_result(receipt))
    }
    pub fn attachment_load_token(&self, id: &str) -> Result<AttachmentLoadToken> {
        self.ensure_resource_idle()?;
        Ok(self.session.attachment_load_token(id)?)
    }
    pub fn replace_attachment_content(
        &mut self,
        t: &AttachmentLoadToken,
        b: Option<Arc<Vec<u8>>>,
    ) -> Result<OperationResult> {
        self.ensure_resource_idle()?;
        let receipt = self.session.replace_attachment_content(t, b)?;
        Ok(self.resource_operation_result(receipt))
    }
    pub fn plan_document_authoring(
        &self,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentEditPlan> {
        self.ensure_resource_idle()?;
        Ok(self.session.plan_document_edit(intent)?)
    }
    pub fn execute_document_authoring(&mut self, p: &DocumentEditPlan) -> Result<OperationResult> {
        self.ensure_resource_idle()?;
        let receipt = self.session.execute_document_plan(p)?;
        Ok(self.resource_operation_result(receipt))
    }
    pub fn document_field_address(
        &self,
        f: kasane_sdk::DocumentField,
    ) -> Result<kasane_sdk::DocumentFieldAddress> {
        Ok(self.session.document_field_address(f)?)
    }
    pub fn plan_document_field_patch(
        &self,
        a: &kasane_sdk::DocumentFieldAddress,
        v: kasane_sdk::DocumentValue,
    ) -> Result<DocumentEditPlan> {
        self.ensure_resource_idle()?;
        Ok(self.session.plan_document_field_patch(a, v)?)
    }
    fn resource_operation_result(&mut self, receipt: kasane_sdk::EditReceipt) -> OperationResult {
        let selection = receipt
            .changes
            .created
            .iter()
            .filter_map(|e| match e.kind {
                kasane_sdk::ObjectKind::Asset => Some(ObjectRef::new(ObjectKind::Texture, &e.id)),
                kasane_sdk::ObjectKind::Attachment => {
                    Some(ObjectRef::new(ObjectKind::Attachment, &e.id))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if receipt.changed {
            self.pristine_blank = false;
            self.consume_authoring_receipt(&receipt, false);
        }
        OperationResult {
            receipt,
            suggested_selection: selection.clone(),
            locations: selection,
        }
    }

    fn commit_resource_edit(
        &mut self,
        edit: kasane_sdk::EditSession<'static>,
    ) -> Result<OperationResult> {
        let textures = self
            .textures
            .prepare_refresh(&self.session, edit.candidate_document())?;
        let receipt = edit.commit_to(&mut self.session)?;
        if let Some(textures) = textures {
            self.textures = textures;
        }
        Ok(self.resource_operation_result(receipt))
    }
}
impl crate::Editor {
    fn ensure_resource_target(&self, id: crate::DocumentId) -> Result<()> {
        ensure!(
            self.active_id() == Some(id) && !self.is_busy() && !self.exiting(),
            "Resource context changed"
        );
        ensure!(
            self.active_session()
                .unwrap()
                .animation
                .timeline
                .edit
                .is_none(),
            "Finish the active Timeline gesture first"
        );
        Ok(())
    }
    pub fn plan_resource_authoring(
        &self,
        id: crate::DocumentId,
        intent: ResourceEdit,
    ) -> Result<ResourceEditPlan> {
        self.ensure_resource_target(id)?;
        self.document().unwrap().plan_resource_authoring(intent)
    }
    pub fn execute_resource_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &ResourceEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_resource_target(id)?;
        let s = self.active_session_mut().unwrap();
        let r = s.document.execute_resource_authoring(p)?;
        s.reconcile_animation();
        Ok(r)
    }
    pub fn plan_document_authoring(
        &self,
        id: crate::DocumentId,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentEditPlan> {
        self.ensure_resource_target(id)?;
        self.document().unwrap().plan_document_authoring(intent)
    }
    pub fn execute_document_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &DocumentEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_resource_target(id)?;
        let s = self.active_session_mut().unwrap();
        let r = s.document.execute_document_authoring(p)?;
        s.reconcile_animation();
        Ok(r)
    }
    pub fn image_load_token(&self, id: crate::DocumentId, asset: &str) -> Result<ImageLoadToken> {
        self.ensure_resource_target(id)?;
        self.document().unwrap().image_load_token(asset)
    }
    pub fn replace_image_content(
        &mut self,
        id: crate::DocumentId,
        t: &ImageLoadToken,
        c: Option<kasane_core::ImageContent>,
    ) -> Result<OperationResult> {
        self.ensure_resource_target(id)?;
        let s = self.active_session_mut().unwrap();
        let r = s.document.replace_image_content(t, c)?;
        s.reconcile_animation();
        Ok(r)
    }
    pub fn replace_prepared_image_content(
        &mut self,
        id: crate::DocumentId,
        t: &ImageLoadToken,
        image: &kasane_sdk::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_resource_target(id)?;
        let s = self.active_session_mut().unwrap();
        let r = s.document.replace_prepared_image_content(t, image)?;
        s.reconcile_animation();
        Ok(r)
    }
    pub fn attachment_load_token(
        &self,
        id: crate::DocumentId,
        asset: &str,
    ) -> Result<AttachmentLoadToken> {
        self.ensure_resource_target(id)?;
        self.document().unwrap().attachment_load_token(asset)
    }
    pub fn replace_attachment_content(
        &mut self,
        id: crate::DocumentId,
        t: &AttachmentLoadToken,
        b: Option<Arc<Vec<u8>>>,
    ) -> Result<OperationResult> {
        self.ensure_resource_target(id)?;
        let s = self.active_session_mut().unwrap();
        let r = s.document.replace_attachment_content(t, b)?;
        s.reconcile_animation();
        Ok(r)
    }
}
