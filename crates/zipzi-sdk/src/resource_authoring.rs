//! Resource plans and cancellable loads, independent of dialogs and file IO.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
pub use zipzi_core::document::ResourceEdit;
use zipzi_core::{ImageContent, document::ResourcePlan};
#[derive(Debug, Clone)]
pub struct ResourceEditPlan {
    version: Version,
    domain: ResourcePlan,
}
impl ResourceEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn is_noop(&self) -> bool {
        self.domain.is_noop()
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
    pub fn copied_ids(&self) -> &std::collections::BTreeMap<String, String> {
        self.domain.copied_ids()
    }
    pub fn drawing_changed(&self) -> bool {
        self.domain.drawing_changed()
    }
}
#[derive(Debug, Clone)]
pub struct ResourceNameAddress {
    handle: ObjectHandle,
    expected: String,
    epoch: u64,
}
impl ResourceNameAddress {
    pub fn id(&self) -> &str {
        self.handle.id()
    }
}
#[derive(Debug, Clone)]
pub struct ImageLoadToken {
    handle: ObjectHandle,
    epoch: u64,
    cancelled: Arc<AtomicBool>,
}
impl ImageLoadToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct ResourceCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to load the referenced resource contents; inspect its state/reason
    /// before invoking the operation.
    pub load_content: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to change its collection order; inspect its state/reason before
    /// invoking the operation.
    pub reorder: CapabilityState,
}
impl AuthoringSession {
    pub(crate) fn begin_resource_load(&self, id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        if let Some(previous) = self
            .resource_loads
            .lock()
            .unwrap()
            .insert(id.into(), flag.clone())
        {
            previous.store(true, Ordering::Release);
        }
        flag
    }
    pub fn resource_collection(&self) -> Result<Vec<ObjectHandle>, SdkError> {
        self.asset_ids()
            .iter()
            .map(|id| self.handle(ObjectKind::Asset, id))
            .collect()
    }
    pub fn resource_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<ResourceCapabilities, SdkError> {
        self.resolve_handle(handle)?;
        if handle.kind() != ObjectKind::Asset {
            return Err(SdkError::new(
                "WRONG_OBJECT_KIND",
                handle.id(),
                "resource_capabilities",
            ));
        }
        let state = match self.document().resource_unlocked(handle.id()) {
            Ok(()) => CapabilityState::Available,
            Err(_) => CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            },
        };
        let delete =
            if state == CapabilityState::Available && !self.references_to(handle.id()).is_empty() {
                CapabilityState::Unavailable {
                    code: "OBJECT_REFERENCED",
                }
            } else {
                state.clone()
            };
        Ok(ResourceCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            load_content: state.clone(),
            duplicate: state.clone(),
            reorder: state,
            delete,
        })
    }
    pub fn plan_resource_edit(&self, intent: ResourceEdit) -> Result<ResourceEditPlan, SdkError> {
        let users = match &intent {
            ResourceEdit::Delete { id, .. } => self.references_to(id),
            _ => vec![],
        };
        Ok(ResourceEditPlan {
            version: self.version(),
            domain: self.document().plan_resource_edit(intent).map_err(|s| {
                let mut error = SdkError::from_status(s, "plan_resource_edit", vec![]);
                if error.code.as_ref() == "OBJECT_REFERENCED" {
                    error.referrers = users.into();
                }
                error
            })?,
        })
    }

    pub fn execute_resource_plan(
        &mut self,
        plan: &ResourceEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        self.edit("Edit resource", Some(plan.version), |e| {
            e.apply_resource_plan(plan)
        })
        .map(|(_, r)| r)
    }
    pub fn resource_name_address(&self, id: &str) -> Result<ResourceNameAddress, SdkError> {
        let handle = self.handle(ObjectKind::Asset, id)?;
        Ok(ResourceNameAddress {
            handle,
            expected: self.document().get_asset(id).unwrap().name.clone(),
            epoch: self.resource_names.get(id).map_or(0, |v| v.1),
        })
    }
    pub fn plan_resource_name_patch(
        &self,
        address: &ResourceNameAddress,
        name: String,
    ) -> Result<ResourceEditPlan, SdkError> {
        self.validate_field_address(
            [&address.handle],
            || self.resource_names.get(address.id()).map_or(0, |v| v.1) != address.epoch,
            address.id(),
            "plan_resource_name_patch",
        )?;
        self.plan_resource_edit(ResourceEdit::PatchName {
            id: address.id().into(),
            expected: address.expected.clone(),
            name,
        })
    }
    pub fn image_load_token(&self, id: &str) -> Result<ImageLoadToken, SdkError> {
        let handle = self.handle(ObjectKind::Asset, id)?;
        self.document()
            .resource_unlocked(id)
            .map_err(|s| SdkError::from_status(s, "image_load_token", vec![id.into()]))?;
        Ok(ImageLoadToken {
            handle,
            epoch: *self.attachment_content_epochs.get(id).unwrap_or(&0),
            cancelled: self.begin_resource_load(id),
        })
    }
    /// Caller performs decoding outside the session; only validated immutable content is published here.
    pub fn replace_image_content(
        &mut self,
        token: &ImageLoadToken,
        content: Option<ImageContent>,
    ) -> Result<EditReceipt, SdkError> {
        let edit = self.begin_image_content_edit(token, content)?;
        edit.commit_to(self)
    }

    /// Stage a token-checked load so hosts can resolve candidate textures before
    /// publication. Dropping the edit leaves the document and request intact;
    /// commit rechecks the version and cancellation flag.
    pub fn begin_image_content_edit(
        &self,
        token: &ImageLoadToken,
        content: Option<ImageContent>,
    ) -> Result<EditSession<'static>, SdkError> {
        self.resolve_handle(&token.handle)?;
        if token.cancelled.load(Ordering::Acquire) {
            return Err(SdkError::new(
                "CANCELLED",
                token.handle.id(),
                "replace_image_content",
            ));
        }
        if *self
            .attachment_content_epochs
            .get(token.handle.id())
            .unwrap_or(&0)
            != token.epoch
        {
            return Err(SdkError::new(
                "STALE_CONTENT",
                token.handle.id(),
                "replace_image_content",
            ));
        }
        let id = token.handle.id();
        let p = self.plan_resource_edit(ResourceEdit::SetContent {
            id: id.into(),
            expected: self.document().get_asset(id).unwrap().content.clone(),
            content,
        })?;
        let mut edit = self.begin_owned_edit("Load image content", Some(p.version))?;
        edit.resource_load_controls.push(token.cancelled.clone());
        edit.apply_resource_plan(&p)?;
        Ok(edit)
    }
}
impl EditSession<'_> {
    pub fn apply_resource_plan(&mut self, plan: &ResourceEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(plan.version, "apply_resource_plan", "Replan resources")?;
        self.apply_authoring_change("apply_resource_plan", "", false, |document| {
            document.apply_resource_plan(&plan.domain)
        })?;
        Ok(())
    }
    pub fn edit_resource(&mut self, intent: ResourceEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_resource")?;
        let p = self
            .candidate_document()
            .plan_resource_edit(intent)
            .map_err(|s| self.abort_with(SdkError::from_status(s, "edit_resource", vec![])))?;
        self.apply_authoring_change("edit_resource", "", false, |document| {
            document.apply_resource_plan(&p)
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AttachmentPathAddress {
    handle: ObjectHandle,
    expected: String,
    epoch: u64,
}
impl AuthoringSession {
    pub fn attachment_path_address(&self, id: &str) -> Result<AttachmentPathAddress, SdkError> {
        let handle = self.handle(ObjectKind::Attachment, id)?;
        Ok(AttachmentPathAddress {
            handle,
            expected: self.document().get_attachment(id).unwrap().path.clone(),
            epoch: self.resource_names.get(id).map_or(0, |v| v.1),
        })
    }
    pub fn plan_attachment_path_patch(
        &self,
        a: &AttachmentPathAddress,
        path: String,
    ) -> Result<crate::AttachmentEditPlan, SdkError> {
        self.validate_field_address(
            [&a.handle],
            || self.resource_names.get(a.handle.id()).map_or(0, |v| v.1) != a.epoch,
            a.handle.id(),
            "plan_attachment_path_patch",
        )?;
        let expected = self
            .document()
            .get_attachment(a.handle.id())
            .unwrap()
            .clone();
        if expected.path != a.expected {
            return Err(SdkError::new(
                "STALE_FIELD",
                a.handle.id(),
                "plan_attachment_path_patch",
            ));
        }
        let mut attachment = expected.clone();
        attachment.path = path;
        self.plan_attachment_edit(zipzi_core::document::AttachmentEdit::Replace {
            expected,
            attachment,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceCollectionCapabilities {
    pub create: CapabilityState,
    pub reorder: CapabilityState,
}
impl AuthoringSession {
    pub fn resource_collection_capabilities(&self) -> ResourceCollectionCapabilities {
        ResourceCollectionCapabilities {
            create: CapabilityState::Available,
            reorder: CapabilityState::Available,
        }
    }
    pub fn attachment_collection(&self) -> Result<Vec<ObjectHandle>, SdkError> {
        self.document()
            .package_attachments()
            .iter()
            .map(|a| self.handle(ObjectKind::Attachment, &a.id))
            .collect()
    }
    pub fn attachment_collection_capabilities(&self) -> ResourceCollectionCapabilities {
        self.resource_collection_capabilities()
    }
}

pub use zipzi_project::PreparedImageContent;
pub fn prepare_image_load(path: &std::path::Path) -> Result<PreparedImageContent, SdkError> {
    zipzi_project::prepare_image_content(path)
        .map_err(|s| SdkError::from_status(s, "prepare_image_load", vec![]))
}
impl AuthoringSession {
    /// Publish a decoded image and its resource in one history entry. The
    /// retained payload keeps save/redo independent of the original file.
    pub fn import_prepared_image(
        &mut self,
        id: String,
        name: String,
        image: &PreparedImageContent,
    ) -> Result<EditReceipt, SdkError> {
        self.begin_import_prepared_image(id, name, image)?
            .commit_to(self)
    }

    /// Stage resource metadata and retain its payload only upon publication.
    pub fn begin_import_prepared_image(
        &self,
        id: String,
        name: String,
        image: &PreparedImageContent,
    ) -> Result<EditSession<'static>, SdkError> {
        let image = image.with_budget(self.image_memory_budget()).map_err(|s| {
            SdkError::from_status(s, "begin_import_prepared_image", vec![id.clone()])
        })?;
        let mut edit = self.begin_owned_edit("Import texture", None)?;
        edit.edit_resource(ResourceEdit::Create {
            id: id.clone(),
            name,
            before: None,
        })?;
        edit.edit_resource(ResourceEdit::SetContent {
            id,
            expected: None,
            content: Some(image.content().clone()),
        })?;
        edit.prepared_images.push(image.clone());
        Ok(edit)
    }

    pub fn replace_prepared_image_content(
        &mut self,
        token: &ImageLoadToken,
        image: &PreparedImageContent,
    ) -> Result<EditReceipt, SdkError> {
        self.begin_prepared_image_content_edit(token, image)?
            .commit_to(self)
    }

    pub fn begin_prepared_image_content_edit(
        &self,
        token: &ImageLoadToken,
        image: &PreparedImageContent,
    ) -> Result<EditSession<'static>, SdkError> {
        let image = image
            .with_budget(self.image_memory_budget())
            .map_err(|s| SdkError::from_status(s, "begin_prepared_image_content_edit", vec![]))?;
        let mut edit = self.begin_image_content_edit(token, Some(image.content().clone()))?;
        edit.prepared_images.push(image.clone());
        Ok(edit)
    }
}

impl crate::AuthoringSnapshot {
    pub fn plan_resource_edit(&self, intent: ResourceEdit) -> Result<ResourceEditPlan, SdkError> {
        Ok(ResourceEditPlan {
            version: self.version(),
            domain: self
                .document()
                .plan_resource_edit(intent)
                .map_err(|s| SdkError::from_status(s, "plan_resource_edit", vec![]))?,
        })
    }
}
