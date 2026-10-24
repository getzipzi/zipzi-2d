//! Client-owned preparation leases, distinct from document resources/history.
use super::*;
use crate::authoring::{ImagePixels, ImageSnapshot, MAX_IMAGE_BYTES, PreparedImageInfo};

pub(super) struct ImageEntry {
    pub client: ClientId,
    pub image: zipzi_sdk::PreparedImageContent,
}

fn preparation_error(s: zipzi_core::Status) -> RuntimeError {
    RuntimeError::Sdk(zipzi_sdk::SdkError {
        code: s.code.into(),
        message: s.message.into(),
        operation: "resources.prepare_image",
        object_ids: vec![],
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    })
}

impl Runtime {
    pub fn prepare_image(
        &mut self,
        client: ClientId,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<PreparedImageInfo> {
        self.client(client)?;
        if self
            .prepared_images
            .values()
            .filter(|image| image.client == client)
            .count()
            >= self.limits.prepared_images_per_client
        {
            return Err(RuntimeError::Budget("prepared images"));
        }
        if rgba.len() > MAX_IMAGE_BYTES {
            return Err(RuntimeError::Budget("prepared image payload"));
        }
        self.check_bytes(rgba.len().saturating_mul(2).saturating_add(1024), 0, 0)?;
        let image = zipzi_sdk::PreparedImageContent::from_rgba_bytes(
            width,
            height,
            rgba,
            &self.image_budget,
        )
        .map_err(preparation_error)?;
        let id = ImageId {
            runtime: self.instance,
            serial: self.serial(),
        };
        self.prepared_images
            .insert(id, ImageEntry { client, image });
        Ok(PreparedImageInfo {
            image: id,
            width,
            height,
        })
    }
    pub(super) fn prepared_image(
        &self,
        client: ClientId,
        image: ImageId,
    ) -> Result<&zipzi_sdk::PreparedImageContent> {
        self.client(client)?;
        let entry = self
            .prepared_images
            .get(&image)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        Ok(&entry.image)
    }
    pub fn prepared_image_pixels(&self, client: ClientId, image: ImageId) -> Result<ImagePixels> {
        let texture = self.prepared_image(client, image)?.texture();
        self.check_bytes(texture.rgba.len().saturating_mul(128), 0, 0)?;
        Ok(ImagePixels {
            width: texture.width,
            height: texture.height,
            rgba: texture.rgba,
        })
    }
    pub fn release_prepared_image(&mut self, client: ClientId, image: ImageId) -> Result<()> {
        self.client(client)?;
        if image.runtime_instance() != self.instance {
            return Err(RuntimeError::InvalidReference);
        }
        if self.prepared_images.contains_key(&image) {
            self.prepared_image(client, image)?;
            self.prepared_images.remove(&image);
        }
        self.collect_image_resources();
        Ok(())
    }
    pub fn prepared_image_count(&self) -> usize {
        self.prepared_images.len()
    }

    pub fn image_snapshot(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        id: &str,
    ) -> Result<ImageSnapshot> {
        self.client(client)?;
        let (asset, source, memory) = if let Some(edit) = edit {
            let entry = self.edit(client, edit)?;
            if entry.session != session {
                return Err(RuntimeError::InvalidReference);
            }
            let candidate = entry.sdk.as_ref().ok_or(RuntimeError::Aborted)?;
            let asset = candidate.candidate_document().get_asset(id).cloned();
            (
                asset,
                SourceRef::Edit {
                    session,
                    edit,
                    base: entry.base,
                    sequence: entry.sequence,
                },
                candidate.memory_image(id),
            )
        } else {
            let entry = self.entry(session)?;
            let source = SourceRef::Formal {
                session,
                version: entry.info.version,
            };
            if let Some(sdk) = &entry.sdk {
                (sdk.document().get_asset(id).cloned(), source, None)
            } else {
                let snapshot = entry.frozen.as_ref().ok_or(RuntimeError::Busy)?;
                (
                    snapshot.document().get_asset(id).cloned(),
                    source,
                    snapshot.memory_image(id).cloned(),
                )
            }
        };
        let asset = asset.ok_or_else(|| {
            RuntimeError::Sdk(zipzi_sdk::SdkError {
                code: "MISSING_IMAGE".into(),
                message: "Image does not exist".into(),
                operation: "resources.get",
                object_ids: vec![id.into()],
                field_path: None,
                expected_version: None,
                actual_version: None,
                referrers: Box::new([]),
            })
        })?;
        let bytes = asset
            .content
            .as_ref()
            .and_then(|c| (c.width as usize).checked_mul(c.height as usize))
            .and_then(|n| n.checked_mul(4))
            .ok_or(RuntimeError::Budget("image snapshot"))?;
        if bytes > MAX_IMAGE_BYTES {
            return Err(RuntimeError::Budget("image snapshot"));
        }
        self.check_bytes(
            bytes
                .saturating_mul(128)
                .saturating_add(asset.name.len().saturating_mul(128)),
            0,
            0,
        )?;
        let texture = match memory {
            Some(image) => image.texture(),
            None => self.ready(session)?.read_texture_asset(&asset)?,
        };
        if texture.rgba.len() > MAX_IMAGE_BYTES {
            return Err(RuntimeError::Budget("image snapshot"));
        }
        Ok(ImageSnapshot {
            source,
            id: asset.id,
            name: asset.name,
            pixels: ImagePixels {
                width: texture.width,
                height: texture.height,
                rgba: texture.rgba,
            },
        })
    }
}
