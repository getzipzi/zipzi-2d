//! Session-independent immutable image resources for the new authoring kernel.
//! A store prepares resources; handles and views own their actual lifetime.
use crate::{ImageMemoryBudget, PreparedImageContent, TextureData};
use std::{collections::HashMap, path::Path, sync::Arc};
use zipzi_contract::kernel::ResourceKey;
use zipzi_core::{state::DocumentState, Status};

/// Resource version whose bytes have been validated and taken into owned storage.
/// Callers cannot attach a key to different pixels or a mutable external path.
#[derive(Debug, Clone)]
pub struct ImageResource(Arc<Resource>);

#[derive(Debug)]
struct Resource {
    key: ResourceKey,
    image: PreparedImageContent,
}

impl ImageResource {
    pub(crate) fn encoded_png(&self) -> Result<(Arc<[u8]>, String), Status> {
        self.0.image.encoded_png()
    }
    pub fn key(&self) -> ResourceKey {
        self.0.key
    }
    pub fn width(&self) -> u32 {
        self.0.image.content().width
    }
    pub fn height(&self) -> u32 {
        self.0.image.content().height
    }
    pub fn texture(&self) -> TextureData {
        self.0.image.texture()
    }
    /// Read verified captured pixels without revisiting a source file or encoding PNG.
    pub fn verified_texture_with_control(
        &self,
        keep_running: impl FnMut() -> bool,
    ) -> Result<crate::VerifiedTexture, Status> {
        self.0.image.verified_texture_with_control(keep_running)
    }

    /// The existing budgeted encoder produces a save projection, not new model content.
    pub fn read_asset(&self) -> Result<crate::AssetData, Status> {
        self.0.image.read_asset()
    }
}

/// Preparation policy, without an active document or strong resource registry.
/// Dropping this store does not invalidate retained ResourceViews.
#[derive(Debug, Clone, Default)]
pub struct ResourceStore {
    budget: ImageMemoryBudget,
}

impl ResourceStore {
    pub fn new(budget: ImageMemoryBudget) -> Self {
        Self { budget }
    }
    pub fn budget(&self) -> &ImageMemoryBudget {
        &self.budget
    }

    /// Adopt already prepared immutable pixels without an RGBA copy. Retain all
    /// existing image-budget responsibilities in the store's configured pool.
    pub fn adopt_image(&self, image: PreparedImageContent) -> Result<ImageResource, Status> {
        let image = image.with_budget(&self.budget)?;
        image.verified_texture()?;
        Ok(ImageResource(Arc::new(Resource {
            key: ResourceKey::new(),
            image,
        })))
    }

    /// Capture an external file now. Future reads use owned bytes even if the
    /// file is overwritten, removed or the source project closes.
    pub fn import_image(&self, path: &Path) -> Result<ImageResource, Status> {
        self.adopt_image(crate::prepare_image_content_with_budget(
            path,
            &self.budget,
        )?)
    }

    pub fn prepare_rgba(
        &self,
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
    ) -> Result<ImageResource, Status> {
        self.adopt_image(PreparedImageContent::from_rgba(
            width,
            height,
            pixels,
            &self.budget,
        )?)
    }
}

/// Immutable, shareable ownership of resource versions. Unresolved assets stay
/// in model content and never masquerade as successfully acquired resources.
#[derive(Debug, Clone, Default)]
pub struct ResourceView {
    images: Arc<HashMap<ResourceKey, ImageResource>>,
}

impl ResourceView {
    pub fn image(&self, key: ResourceKey) -> Option<&ImageResource> {
        self.images.get(&key)
    }
    pub fn len(&self) -> usize {
        self.images.len()
    }
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Batch resource delta. Existing views and their resources remain unchanged.
    pub fn with_images(&self, images: impl IntoIterator<Item = ImageResource>) -> Self {
        let mut result = self.clone();
        let entries = Arc::make_mut(&mut result.images);
        for image in images {
            entries.insert(image.key(), image);
        }
        result
    }

    /// Validate every resolved reference, then retain exactly this state's
    /// resources. SDK must perform this pairing before preparing a Snapshot.
    pub fn for_state(&self, state: &DocumentState) -> Result<Self, Status> {
        let mut images = HashMap::new();
        for (key, width, height) in state.image_resources() {
            let image = self
                .image(key)
                .ok_or_else(|| Status::error("MISSING_RESOURCE", format!("{key:?}")))?;
            if (image.width(), image.height()) != (width, height) {
                return Err(Status::error("RESOURCE_DIMENSIONS", format!("{key:?}")));
            }
            images.entry(key).or_insert_with(|| image.clone());
        }
        Ok(Self {
            images: Arc::new(images),
        })
    }
}

#[cfg(test)]
mod tests;

impl ResourceView {
    pub fn read_asset(
        &self,
        asset: &zipzi_core::state::AssetRecord,
    ) -> Result<crate::AssetData, Status> {
        let zipzi_core::state::AssetContent::Image {
            resource,
            width,
            height,
        } = asset.content
        else {
            return Err(Status::error("MISSING_RESOURCE", &asset.id));
        };
        let image = self
            .image(resource)
            .ok_or_else(|| Status::error("MISSING_RESOURCE", &asset.id))?;
        if (width, height) != (image.width(), image.height()) {
            return Err(Status::error("RESOURCE_DIMENSIONS", &asset.id));
        }
        image.read_asset()
    }
}
