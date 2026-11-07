//! Resource loading and metadata plans attach editor context to SDK authority.
use super::*;
use anyhow::Result;
use zipzi_contract::document::DocumentAuthoringEdit;
use zipzi_contract::document::ResourceEdit;

#[derive(Debug, Clone)]
pub struct ResourceAuthoringCapabilities {
    pub domain: zipzi_contract::authoring::ResourceCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}
#[derive(Debug, Clone)]
pub struct TextureInfo {
    pub asset: zipzi_core::state::AssetRecord,
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
        image: &zipzi_project::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let image = self.session.resource_store().adopt_image(image.clone())?;
        let (_, receipt) = self.session.edit("Import image", None, |commands| {
            commands.create_empty_asset(id.clone(), name)?;
            commands.replace_image(&id, image)
        })?;
        self.refresh_textures()?;
        Ok(self.resource_operation_result(receipt, None))
    }
    pub fn resource_collection(&self) -> Result<Vec<ObjectRef>> {
        Ok(self
            .model()
            .asset_order()
            .iter()
            .map(|id| ObjectRef::new(ObjectKind::Texture, id))
            .collect())
    }

    pub fn resource_capabilities(&self, id: &str) -> Result<ResourceAuthoringCapabilities> {
        Ok(ResourceAuthoringCapabilities {
            domain: self
                .model()
                .resource_capabilities(zipzi_contract::authoring::ObjectKind::Asset, id)?,
            session_reason: self
                .pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn resource_collection_capabilities(
        &self,
    ) -> (
        zipzi_contract::authoring::CollectionCapabilities,
        Option<AuthoringSessionReason>,
    ) {
        (
            self.model()
                .collection_capabilities(&CollectionTarget::Resources)
                .expect("root resource collection"),
            self.pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        )
    }
    pub fn attachment_path_address(
        &self,
        id: &str,
    ) -> Result<crate::FieldCapture<zipzi_core::document::PackageAttachment>> {
        self.attachment_load_token(id)
    }
    pub fn plan_attachment_path_patch(
        &self,
        a: &crate::FieldCapture<zipzi_core::document::PackageAttachment>,
        path: String,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        let mut attachment = a.value.clone();
        attachment.path = path;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Change attachment path",
            Some(a.source),
            |commands| {
                commands.edit_attachment(zipzi_contract::document::AttachmentEdit::Replace {
                    expected: a.value.clone(),
                    attachment,
                })
            },
        )?)
    }
    pub fn document_field_capability(
        &self,
        f: zipzi_contract::document::DocumentField,
    ) -> (
        zipzi_contract::authoring::CapabilityState,
        Option<AuthoringSessionReason>,
    ) {
        (
            self.session.document_field_capability(f),
            self.pending
                .as_ref()
                .map(|_| AuthoringSessionReason::CandidateActive),
        )
    }
    pub fn plan_resource_authoring(&self, intent: ResourceEdit) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit resource",
            None,
            |commands| commands.edit_resource(intent),
        )?)
    }
    pub fn execute_resource_authoring(
        &mut self,
        p: &crate::EditPreview<()>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let textures = self.textures.prepare_refresh(&p.snapshot().snapshot)?;
        let receipt = p.publish(&mut self.session)?;
        if let Some(textures) = textures {
            self.textures = textures;
        }
        Ok(self
            .resource_operation_result(receipt, Some(p.baseline().document()))
            .with_preview(p))
    }
    pub fn resource_name_address(&self, id: &str) -> Result<crate::FieldCapture<String>> {
        let captured = self.image_load_token(id)?;
        Ok(crate::FieldCapture {
            source: captured.source,
            id: captured.id,
            owner: None,
            value: captured.value.name,
        })
    }
    pub fn plan_resource_name_patch(
        &self,
        a: &crate::FieldCapture<String>,
        name: String,
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Rename resource",
            Some(a.source),
            |commands| {
                commands.edit_resource(ResourceEdit::PatchName {
                    id: a.id.clone(),
                    expected: a.value.clone(),
                    name,
                })
            },
        )?)
    }
    pub fn image_load_token(
        &self,
        id: &str,
    ) -> Result<crate::FieldCapture<zipzi_core::state::AssetRecord>> {
        self.ensure_authoring_idle()?;
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let value = captured
            .snapshot
            .document()
            .asset(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Texture no longer exists"))?;
        Ok(crate::FieldCapture {
            source,
            id: id.into(),
            owner: None,
            value,
        })
    }

    pub fn replace_prepared_image_content(
        &mut self,
        t: &crate::FieldCapture<zipzi_core::state::AssetRecord>,
        image: &zipzi_project::PreparedImageContent,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let image = self.session.resource_store().adopt_image(image.clone())?;
        let (_, receipt) = self
            .session
            .edit("Replace image", Some(t.source), |commands| {
                commands.replace_image(&t.id, image)
            })?;
        self.refresh_textures()?;
        Ok(self.resource_operation_result(receipt, None))
    }
    pub fn attachment_load_token(
        &self,
        id: &str,
    ) -> Result<crate::FieldCapture<zipzi_core::document::PackageAttachment>> {
        self.ensure_authoring_idle()?;
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let value = captured
            .snapshot
            .document()
            .attachment(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Attachment no longer exists"))?;
        Ok(crate::FieldCapture {
            source,
            id: id.into(),
            owner: None,
            value,
        })
    }
    pub fn replace_attachment_content(
        &mut self,
        t: &crate::FieldCapture<zipzi_core::document::PackageAttachment>,
        b: Option<Arc<Vec<u8>>>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let mut attachment = t.value.clone();
        attachment.bytes = b;
        let (_, receipt) = self
            .session
            .edit("Replace attachment", Some(t.source), |commands| {
                commands.edit_attachment(zipzi_contract::document::AttachmentEdit::Replace {
                    expected: t.value.clone(),
                    attachment,
                })
            })?;
        Ok(self.resource_operation_result(receipt, None))
    }
    pub fn plan_document_authoring(
        &self,
        intent: DocumentAuthoringEdit,
    ) -> Result<crate::EditPreview<zipzi_core::state::DocumentOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit document",
            None,
            |commands| commands.edit_document(intent),
        )?)
    }
    pub fn execute_document_authoring(
        &mut self,
        p: &crate::EditPreview<zipzi_core::state::DocumentOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = p.publish(&mut self.session)?;
        Ok(self.resource_operation_result(receipt, Some(p.baseline().document())))
    }
    pub fn document_field_address(
        &self,
        f: zipzi_contract::document::DocumentField,
    ) -> Result<crate::FieldCapture<zipzi_contract::document::DocumentValue>> {
        let captured = self.session.sourced_snapshot();
        let zipzi_sdk::QuerySource::Published(source) = captured.source else {
            unreachable!()
        };
        let value = captured.snapshot.document().document_value(f)?;
        Ok(crate::FieldCapture {
            source,
            id: captured.snapshot.document().id().into(),
            owner: None,
            value,
        })
    }
    pub fn plan_document_field_patch(
        &self,
        a: &crate::FieldCapture<zipzi_contract::document::DocumentValue>,
        v: zipzi_contract::document::DocumentValue,
    ) -> Result<crate::EditPreview<zipzi_core::state::DocumentOutcome>> {
        self.ensure_authoring_idle()?;
        Ok(crate::EditPreview::prepare(
            &self.session,
            "Edit document field",
            Some(a.source),
            |commands| {
                commands.edit_document(DocumentAuthoringEdit::Patch {
                    expected: a.value.clone(),
                    value: v,
                })
            },
        )?)
    }
    fn resource_operation_result(
        &mut self,
        receipt: zipzi_sdk::Publication,
        before: Option<&zipzi_core::state::DocumentState>,
    ) -> OperationResult {
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, before);
        }
        self.authoring_result(receipt)
    }
}
impl crate::Editor {
    pub fn import_prepared_texture(
        &mut self,
        document: crate::DocumentId,
        id: String,
        name: String,
        image: &zipzi_project::PreparedImageContent,
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
    ) -> Result<crate::EditPreview<()>> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().plan_resource_authoring(intent)
    }
    pub fn execute_resource_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &crate::EditPreview<()>,
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
    ) -> Result<crate::EditPreview<zipzi_core::state::DocumentOutcome>> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().plan_document_authoring(intent)
    }
    pub fn execute_document_authoring(
        &mut self,
        id: crate::DocumentId,
        p: &crate::EditPreview<zipzi_core::state::DocumentOutcome>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_document_authoring(p))
    }
    pub fn image_load_token(
        &self,
        id: crate::DocumentId,
        asset: &str,
    ) -> Result<crate::FieldCapture<zipzi_core::state::AssetRecord>> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().image_load_token(asset)
    }

    pub fn replace_prepared_image_content(
        &mut self,
        id: crate::DocumentId,
        t: &crate::FieldCapture<zipzi_core::state::AssetRecord>,
        image: &zipzi_project::PreparedImageContent,
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
    ) -> Result<crate::FieldCapture<zipzi_core::document::PackageAttachment>> {
        self.ensure_authoring_target(id, "Resource")?;
        self.document().unwrap().attachment_load_token(asset)
    }
    pub fn replace_attachment_content(
        &mut self,
        id: crate::DocumentId,
        t: &crate::FieldCapture<zipzi_core::document::PackageAttachment>,
        b: Option<Arc<Vec<u8>>>,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(id, "Resource")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.replace_attachment_content(t, b))
    }
}
