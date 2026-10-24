//! Resource loading and metadata plans attach editor context to SDK authority.
use super::*;
use anyhow::Result;
use zipzi_sdk::{
    AttachmentLoadToken, DocumentAuthoringEdit, DocumentEditPlan, ImageLoadToken, ResourceEdit,
    ResourceEditPlan,
};
#[derive(Debug, Clone)]
pub struct ResourceAuthoringCapabilities {
    pub domain: zipzi_sdk::ResourceCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
#[derive(Debug, Clone)]
pub struct TextureInfo {
    pub asset: zipzi_core::ImageAsset,
    pub users: Vec<ObjectRef>,
}
impl Document {
    pub fn texture_info(&self, id: &str) -> Result<TextureInfo> {
        let asset = self
            .session
            .asset(id)
            .ok_or_else(|| anyhow::anyhow!("Texture no longer exists"))?;
        let index = self.object_index();
        let users = self
            .session
            .references_to(id)
            .into_iter()
            .filter_map(|id| {
                index
                    .objects
                    .iter()
                    .find(|o| o.reference.id == id)
                    .map(|o| o.reference.clone())
            })
            .collect();
        Ok(TextureInfo { asset, users })
    }
    pub fn import_prepared_texture(
        &mut self,
        id: String,
        name: String,
        image: &zipzi_sdk::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.import_prepared_image(id, name, image)?;
        self.refresh_textures()?;
        Ok(self.resource_operation_result(receipt))
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
                .resource_capabilities(&self.session.handle(zipzi_sdk::ObjectKind::Asset, id)?)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn resource_collection_capabilities(
        &self,
    ) -> (
        zipzi_sdk::ResourceCollectionCapabilities,
        Option<AuthoringSessionReason>,
    ) {
        (
            self.session.resource_collection_capabilities(),
            self.pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        )
    }
    pub fn attachment_path_address(&self, id: &str) -> Result<zipzi_sdk::AttachmentPathAddress> {
        Ok(self.session.attachment_path_address(id)?)
    }
    pub fn plan_attachment_path_patch(
        &self,
        a: &zipzi_sdk::AttachmentPathAddress,
        path: String,
    ) -> Result<zipzi_sdk::AttachmentEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_attachment_path_patch(a, path)?)
    }
    pub fn document_field_capability(
        &self,
        f: zipzi_sdk::DocumentField,
    ) -> (zipzi_sdk::CapabilityState, Option<AuthoringSessionReason>) {
        (
            self.session.document_field_capability(f),
            self.pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        )
    }
    pub fn plan_resource_authoring(&self, intent: ResourceEdit) -> Result<ResourceEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_resource_edit(intent)?)
    }
    pub fn execute_resource_authoring(&mut self, p: &ResourceEditPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let mut edit = self
            .session
            .begin_owned_edit("Edit resource", Some(p.version()))?;
        edit.apply_resource_plan(p)?;
        self.commit_resource_edit(edit)
    }
    pub fn resource_name_address(&self, id: &str) -> Result<zipzi_sdk::ResourceNameAddress> {
        Ok(self.session.resource_name_address(id)?)
    }
    pub fn plan_resource_name_patch(
        &self,
        a: &zipzi_sdk::ResourceNameAddress,
        name: String,
    ) -> Result<ResourceEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_resource_name_patch(a, name)?)
    }
    pub fn image_load_token(&self, id: &str) -> Result<ImageLoadToken> {
        self.ensure_authoring_idle()?;
        Ok(self.session.image_load_token(id)?)
    }
    pub fn replace_image_content(
        &mut self,
        t: &ImageLoadToken,
        c: Option<zipzi_core::ImageContent>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let edit = self.session.begin_image_content_edit(t, c)?;
        self.commit_resource_edit(edit)
    }
    pub fn replace_prepared_image_content(
        &mut self,
        t: &ImageLoadToken,
        image: &zipzi_sdk::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.replace_prepared_image_content(t, image)?;
        self.refresh_textures()?;
        Ok(self.resource_operation_result(receipt))
    }
    pub fn attachment_load_token(&self, id: &str) -> Result<AttachmentLoadToken> {
        self.ensure_authoring_idle()?;
        Ok(self.session.attachment_load_token(id)?)
    }
    pub fn replace_attachment_content(
        &mut self,
        t: &AttachmentLoadToken,
        b: Option<Arc<Vec<u8>>>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.replace_attachment_content(t, b)?;
        Ok(self.resource_operation_result(receipt))
    }
    pub fn plan_document_authoring(
        &self,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_document_edit(intent)?)
    }
    pub fn execute_document_authoring(&mut self, p: &DocumentEditPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_document_plan(p)?;
        Ok(self.resource_operation_result(receipt))
    }
    pub fn document_field_address(
        &self,
        f: zipzi_sdk::DocumentField,
    ) -> Result<zipzi_sdk::DocumentFieldAddress> {
        Ok(self.session.document_field_address(f)?)
    }
    pub fn plan_document_field_patch(
        &self,
        a: &zipzi_sdk::DocumentFieldAddress,
        v: zipzi_sdk::DocumentValue,
    ) -> Result<DocumentEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_document_field_patch(a, v)?)
    }
    fn resource_operation_result(&mut self, receipt: zipzi_sdk::EditReceipt) -> OperationResult {
        if receipt.changed {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, false);
        }
        self.authoring_result(receipt)
    }

    fn commit_resource_edit(
        &mut self,
        edit: crate::runtime_host::HostedEdit,
    ) -> Result<OperationResult> {
        let textures = self
            .textures
            .prepare_refresh(&self.session.read(), &edit.candidate_document())?;
        let receipt = edit.commit_to(&mut self.session)?;
        if let Some(textures) = textures {
            self.textures = textures;
        }
        Ok(self.resource_operation_result(receipt))
    }
}
impl crate::Editor {
    pub fn import_prepared_texture(
        &mut self,
        document: crate::DocumentId,
        id: String,
        name: String,
        image: &zipzi_sdk::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(document, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|d| d.import_prepared_texture(id, name, image))
    }
    pub fn plan_resource_authoring(
        &self,
        id: crate::DocumentId,
        intent: ResourceEdit,
    ) -> Result<ResourceEditPlan> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().plan_resource_authoring(intent)
    }
    pub fn execute_resource_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &ResourceEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_resource_authoring(p))
    }
    pub fn plan_document_authoring(
        &self,
        id: crate::DocumentId,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentEditPlan> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().plan_document_authoring(intent)
    }
    pub fn execute_document_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &DocumentEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_document_authoring(p))
    }
    pub fn image_load_token(&self, id: crate::DocumentId, asset: &str) -> Result<ImageLoadToken> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().image_load_token(asset)
    }
    pub fn replace_image_content(
        &mut self,
        id: crate::DocumentId,
        t: &ImageLoadToken,
        c: Option<zipzi_core::ImageContent>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.replace_image_content(t, c))
    }
    pub fn replace_prepared_image_content(
        &mut self,
        id: crate::DocumentId,
        t: &ImageLoadToken,
        image: &zipzi_sdk::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.replace_prepared_image_content(t, image))
    }
    pub fn attachment_load_token(
        &self,
        id: crate::DocumentId,
        asset: &str,
    ) -> Result<AttachmentLoadToken> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().attachment_load_token(asset)
    }
    pub fn replace_attachment_content(
        &mut self,
        id: crate::DocumentId,
        t: &AttachmentLoadToken,
        b: Option<Arc<Vec<u8>>>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.replace_attachment_content(t, b))
    }
}
