//! Disk resource validation and lazily encoded in-memory draft textures.
use super::{DocumentStore, ResourceDiagnostic};
use crate::filesystem as io;
use crate::resources::{content_sha256, decode_png, AssetData, TextureData};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zipzi_core::{Document, ImageAsset, Status};

pub fn read_project_asset(root: &Path, asset: &ImageAsset) -> Result<AssetData, Status> {
    read_project_asset_if_changed(root, asset, None).map(|data| data.unwrap())
}

/// Skip PNG decoding only when these exact bytes have already been validated.
/// Cached dimensions are checked against current metadata before skipping decode.
pub fn read_project_asset_if_changed(
    root: &Path,
    asset: &ImageAsset,
    validated_image: Option<(&str, u32, u32)>,
) -> Result<Option<AssetData>, Status> {
    let source_path = asset_path(root, asset)?;

    let bytes = fs::read(&source_path)
        .map_err(|e| Status::error("PROJECT_IO", format!("{}: {}", asset.id, e)))?;

    if let Some((hash, width, height)) = validated_image {
        if width == asset.content.as_ref().unwrap().width
            && height == asset.content.as_ref().unwrap().height
            && (asset.content.as_ref().unwrap().sha256.is_empty()
                || asset.content.as_ref().unwrap().sha256 == hash)
            && content_sha256(&bytes) == hash
        {
            return Ok(None);
        }
    }

    let data = decode_png(&bytes).map_err(|mut s| {
        s.message = format!("{}: {}", asset.id, s.message);
        s
    })?;

    if data.width != asset.content.as_ref().unwrap().width
        || data.height != asset.content.as_ref().unwrap().height
    {
        return Err(Status::error(
            "RESOURCE_DIMENSIONS",
            format!("{}: PNG dimensions differ from metadata", asset.id),
        ));
    }

    if !asset.content.as_ref().unwrap().sha256.is_empty()
        && data.sha256 != asset.content.as_ref().unwrap().sha256
    {
        return Err(Status::error(
            "RESOURCE_HASH",
            format!("{}: PNG differs from saved SHA-256", asset.id),
        ));
    }

    Ok(Some(data))
}

pub(super) struct MemoryAsset {
    pub(super) texture: TextureData,
    pub(super) png: Mutex<Option<Arc<VerifiedAsset>>>,
    pub(super) budget: crate::ImageMemoryBudget,
}

impl MemoryAsset {
    pub(super) fn encoded_png(&self) -> Result<Arc<VerifiedAsset>, Status> {
        let mut cache = self.png.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(png) = cache.as_ref() {
            return Ok(png.clone());
        }
        // Conservative bound for an uncompressed zlib stream, row filters and
        // PNG chunk framing. The capped writer checks before every Vec growth.
        let raw = self
            .texture
            .rgba
            .len()
            .saturating_add(self.texture.height as usize);
        let bound = raw.saturating_add(raw / 100).saturating_add(1024);
        let reservation = self.budget.reserve(bound.saturating_mul(2))?;
        let mut output = BoundedPng {
            bytes: Vec::new(),
            limit: bound,
        };
        {
            let mut encoder =
                png::Encoder::new(&mut output, self.texture.width, self.texture.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| Status::error("PNG_ENCODE", e.to_string()))?;
            writer
                .write_image_data(&self.texture.rgba)
                .map_err(|e| Status::error("PNG_ENCODE", e.to_string()))?;
        }
        let bytes: Arc<[u8]> = output.bytes.into();
        reservation.commit(&bytes, crate::ImageBufferKind::Png)?;
        let result = Arc::new(VerifiedAsset {
            sha256: content_sha256(&bytes),
            bytes,
        });
        *cache = Some(result.clone());
        Ok(result)
    }

    pub(super) fn validate_metadata(&self, asset: &ImageAsset) -> Result<(), Status> {
        asset.image_content()?;
        if self.texture.width != asset.content.as_ref().unwrap().width
            || self.texture.height != asset.content.as_ref().unwrap().height
        {
            return Err(Status::error(
                "RESOURCE_DIMENSIONS",
                format!("{}: PNG dimensions differ from metadata", asset.id),
            ));
        }
        if !asset.content.as_ref().unwrap().sha256.is_empty()
            && self.encoded_png()?.sha256 != asset.content.as_ref().unwrap().sha256
        {
            return Err(Status::error(
                "RESOURCE_HASH",
                format!("{}: PNG differs from saved SHA-256", asset.id),
            ));
        }
        Ok(())
    }
}

pub(super) struct VerifiedAsset {
    pub(super) bytes: Arc<[u8]>,
    pub(super) sha256: String,
}

impl DocumentStore {
    pub(super) fn clear_memory_assets(&self) {
        self.memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }

    pub(super) fn has_memory_asset(&self, asset: &ImageAsset) -> bool {
        self.memory_asset(asset).is_some()
    }

    pub(super) fn memory_asset(&self, asset: &ImageAsset) -> Option<Arc<MemoryAsset>> {
        self.memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&asset.content.as_ref()?.source)
            .cloned()
    }

    /// Immutable draft pixels are encoded and hashed once. Disk bytes are read
    /// and hashed on every call, so external edits are detected.
    pub(super) fn read_verified_asset(
        &self,
        root: &Path,
        asset: &ImageAsset,
    ) -> Result<VerifiedAsset, Status> {
        if let Some(memory) = self.memory_asset(asset) {
            memory.validate_metadata(asset)?;
            let png = memory.encoded_png()?;
            return Ok(VerifiedAsset {
                bytes: png.bytes.clone(),
                sha256: png.sha256.clone(),
            });
        }
        let source_path = asset_path(root, asset)?;
        let bytes = fs::read(&source_path)
            .map_err(|e| Status::error("PROJECT_IO", format!("{}: {}", asset.id, e)))?;
        let sha256 = content_sha256(&bytes);
        let cached_dimensions = self
            .verified_pngs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&sha256)
            .copied();
        let (width, height) = match cached_dimensions {
            Some(dimensions) => dimensions,
            None => {
                let image = zipzi_core::image::decode_png(&bytes).map_err(|mut status| {
                    status.message = format!("{}: {}", asset.id, status.message);
                    status
                })?;
                let dimensions = (image.width, image.height);
                self.verified_pngs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(sha256.clone(), dimensions);
                dimensions
            }
        };
        if width != asset.content.as_ref().unwrap().width
            || height != asset.content.as_ref().unwrap().height
        {
            return Err(Status::error(
                "RESOURCE_DIMENSIONS",
                format!("{}: PNG dimensions differ from metadata", asset.id),
            ));
        }
        if !asset.content.as_ref().unwrap().sha256.is_empty()
            && sha256 != asset.content.as_ref().unwrap().sha256
        {
            return Err(Status::error(
                "RESOURCE_HASH",
                format!("{}: PNG differs from saved SHA-256", asset.id),
            ));
        }
        Ok(VerifiedAsset {
            bytes: bytes.into(),
            sha256,
        })
    }

    pub(super) fn remember_imported_assets(&self, document: &Document) {
        let mut verified = self
            .verified_pngs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for id in document.asset_order() {
            let asset = document.get_asset(id).unwrap();
            if asset.content.is_none() {
                continue;
            }
            // MOC3 import assigns a hash only after successfully decoding the PNG.
            if !asset.content.as_ref().unwrap().sha256.is_empty() {
                verified.insert(
                    asset.content.as_ref().unwrap().sha256.clone(),
                    (
                        asset.content.as_ref().unwrap().width,
                        asset.content.as_ref().unwrap().height,
                    ),
                );
            }
        }
    }

    pub fn diagnose(&self, document: &Document, root: &Path) -> Vec<ResourceDiagnostic> {
        let mut result = Vec::new();
        for id in document.asset_order() {
            let asset = document.get_asset(id).unwrap();
            let validation = match self.memory_asset(asset) {
                Some(memory) => memory.validate_metadata(asset),
                None => self.read_verified_asset(root, asset).map(|_| ()),
            };
            if let Err(s) = validation {
                result.push(ResourceDiagnostic {
                    asset_id: id.clone(),
                    code: s.code,
                    message: s.message,
                });
            }
        }
        result
    }
}

pub(crate) fn asset_path(root: &Path, asset: &ImageAsset) -> Result<PathBuf, Status> {
    let content = asset.image_content()?;
    let path = Path::new(&content.source);
    if path.is_absolute() {
        return io::local_path(path);
    }
    if !crate::resources::is_valid_asset_path(&asset.content.as_ref().unwrap().source) {
        return Err(Status::error(
            "INVALID_PATH",
            &asset.content.as_ref().unwrap().source,
        ));
    }
    let root = io::local_path(root)?;
    let resolved = io::local_path(&root.join(path))?;
    if !resolved.starts_with(&root) {
        return Err(Status::error(
            "INVALID_PATH",
            "Asset escapes the project root",
        ));
    }
    Ok(resolved)
}

/// Validated immutable pixels and encoded bytes, prepared without an editing session.
#[derive(Clone)]
pub struct PreparedImageContent {
    memory: Arc<MemoryAsset>,
    content: zipzi_core::ImageContent,
}
impl std::fmt::Debug for PreparedImageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedImageContent")
            .field("content", &self.content)
            .finish_non_exhaustive()
    }
}
impl PreparedImageContent {
    pub fn texture(&self) -> TextureData {
        self.memory.texture.clone()
    }
    pub fn with_budget(&self, budget: &crate::ImageMemoryBudget) -> Result<Self, Status> {
        Ok(Self::with_budget_many(std::slice::from_ref(self), budget)?
            .pop()
            .unwrap())
    }
    pub fn with_budget_many(
        images: &[Self],
        budget: &crate::ImageMemoryBudget,
    ) -> Result<Vec<Self>, Status> {
        let captured: Vec<_> = images
            .iter()
            .map(|image| {
                (
                    image.memory.texture.clone(),
                    image
                        .memory
                        .png
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .clone(),
                )
            })
            .collect();
        let mut buffers = Vec::new();
        for (texture, png) in &captured {
            buffers.push((&texture.rgba, crate::ImageBufferKind::Rgba));
            if let Some(png) = png {
                buffers.push((&png.bytes, crate::ImageBufferKind::Png));
            }
        }
        budget.track_buffers(&buffers)?;
        Ok(images
            .iter()
            .zip(captured)
            .map(|(image, (texture, png))| {
                if image.memory.budget.same_pool(budget) {
                    image.clone()
                } else {
                    Self {
                        content: image.content.clone(),
                        memory: Arc::new(MemoryAsset {
                            texture,
                            png: Mutex::new(png),
                            budget: budget.clone(),
                        }),
                    }
                }
            })
            .collect())
    }
    pub fn content(&self) -> &zipzi_core::ImageContent {
        &self.content
    }
    pub fn read_asset(&self) -> Result<AssetData, Status> {
        if self.content.width != self.memory.texture.width
            || self.content.height != self.memory.texture.height
        {
            return Err(Status::error("RESOURCE_DIMENSIONS", &self.content.source));
        }
        let png = self.memory.encoded_png()?;
        if !self.content.sha256.is_empty() && self.content.sha256 != png.sha256 {
            return Err(Status::error("RESOURCE_HASH", &self.content.source));
        }
        Ok(AssetData {
            bytes: png.bytes.to_vec(),
            rgba: self.memory.texture.rgba.to_vec(),
            width: self.memory.texture.width,
            height: self.memory.texture.height,
            sha256: png.sha256.clone(),
        })
    }
}
pub fn prepare_image_content(path: &Path) -> Result<PreparedImageContent, Status> {
    prepare_image_content_with_budget(path, &crate::ImageMemoryBudget::default())
}
pub fn prepare_image_content_with_budget(
    path: &Path,
    budget: &crate::ImageMemoryBudget,
) -> Result<PreparedImageContent, Status> {
    use std::io::Read;
    if !path.is_absolute() {
        return Err(Status::error("INVALID_PATH", "PNG source must be absolute"));
    }
    let file = fs::File::open(path).map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?;
    let length = file
        .metadata()
        .map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?
        .len();
    if length > 64 * 1024 * 1024 {
        return Err(Status::error("CAPACITY", "PNG source exceeds 64 MiB"));
    }
    let encoded = budget.reserve((length as usize).saturating_mul(2))?;
    let mut bytes = Vec::new();
    file.take(length + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?;
    if bytes.len() as u64 != length {
        return Err(Status::error("RESOURCE_IO", "PNG changed while preparing"));
    }
    let bytes: Arc<[u8]> = bytes.into();
    encoded.commit(&bytes, crate::ImageBufferKind::Png)?;
    prepare_png_content(bytes, budget)
}
/// Decode immutable in-memory PNG bytes under the same allocation budget as
/// file preparation. Input Arc ownership pins the encoded source during decode.
pub fn prepare_png_content(
    bytes: Arc<[u8]>,
    budget: &crate::ImageMemoryBudget,
) -> Result<PreparedImageContent, Status> {
    if bytes.len() > 64 << 20 {
        return Err(Status::error("CAPACITY", "PNG source exceeds 64 MiB"));
    }
    budget.track(&bytes, crate::ImageBufferKind::Png)?;
    if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n") || bytes.len() < 24 {
        return Err(Status::error("INVALID_IMAGE", "Invalid PNG header"));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    let size = rgba_size(width, height)?;
    let pixels = budget.reserve(size.saturating_mul(2))?;
    let data = zipzi_core::image::decode_png(&bytes)?;
    let rgba: Arc<[u8]> = data.rgba.into();
    pixels.commit(&rgba, crate::ImageBufferKind::Rgba)?;
    let sha256 = content_sha256(&bytes);
    Ok(PreparedImageContent {
        content: zipzi_core::ImageContent {
            source: format!("assets/.memory-loaded/{sha256}.png"),
            width,
            height,
            sha256: sha256.clone(),
        },
        memory: Arc::new(MemoryAsset {
            texture: TextureData {
                rgba,
                width,
                height,
                cache_key: format!("png:{sha256}"),
            },
            png: Mutex::new(Some(Arc::new(VerifiedAsset { bytes, sha256 }))),
            budget: budget.clone(),
        }),
    })
}
fn rgba_size(width: u32, height: u32) -> Result<usize, Status> {
    let size = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || size.is_none_or(|n| n > 256 * 1024 * 1024)
    {
        return Err(Status::error(
            "INVALID_IMAGE",
            "Invalid or excessive RGBA dimensions",
        ));
    }
    Ok(size.unwrap())
}
impl PreparedImageContent {
    pub fn from_rgba_bytes(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        budget: &crate::ImageMemoryBudget,
    ) -> Result<Self, Status> {
        if rgba_size(width, height)? != rgba.len() {
            return Err(Status::error(
                "INVALID_IMAGE",
                "RGBA length differs from dimensions",
            ));
        }
        let reservation = budget.reserve(rgba.len().saturating_mul(2))?;
        #[cfg(feature = "bench-instrumentation")]
        crate::resource_budget::record_rgba_copy(rgba.len());
        let rgba: Arc<[u8]> = rgba.into();
        reservation.commit(&rgba, crate::ImageBufferKind::Rgba)?;
        Self::from_rgba(width, height, rgba, budget)
    }
    /// Callers owning input pixels pay no additional copy. Host-created input
    /// buffers should reserve before decoding/allocating, then commit the lease.
    pub fn from_rgba(
        width: u32,
        height: u32,
        rgba: Arc<[u8]>,
        budget: &crate::ImageMemoryBudget,
    ) -> Result<Self, Status> {
        if rgba_size(width, height)? != rgba.len() {
            return Err(Status::error(
                "INVALID_IMAGE",
                "RGBA length differs from dimensions",
            ));
        }
        budget.track(&rgba, crate::ImageBufferKind::Rgba)?;
        let key = format!("{width}x{height}-{}", content_sha256(&rgba));
        Ok(Self {
            content: zipzi_core::ImageContent {
                source: format!("assets/.memory-generated/{key}.png"),
                width,
                height,
                sha256: String::new(),
            },
            memory: Arc::new(MemoryAsset {
                texture: TextureData {
                    rgba,
                    width,
                    height,
                    cache_key: format!("generated:{key}"),
                },
                png: Mutex::new(None),
                budget: budget.clone(),
            }),
        })
    }
}
impl super::DocumentSession {
    /// Called only after a successful SDK publication. Registration clones an Arc.
    pub fn retain_prepared_image(&self, image: &PreparedImageContent) {
        self.store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(image.content.source.clone())
            .or_insert_with(|| image.memory.clone());
    }
    pub(crate) fn retain_image_sources(&self, sources: &std::collections::HashSet<String>) {
        self.store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|source, _| sources.contains(source));
    }

    pub fn memory_image(&self, asset: &ImageAsset) -> Option<PreparedImageContent> {
        Some(PreparedImageContent {
            memory: self.store.memory_asset(asset)?,
            content: asset.content.clone()?,
        })
    }
}

struct BoundedPng {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for BoundedPng {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(std::io::Error::other(
                "PNG exceeds reserved encoding capacity",
            ));
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Shared lookup for owned SDK candidates. Pins keep resources readable after
/// collection, while this lookup alone does not keep an unused payload alive.
#[derive(Clone)]
pub struct ImageResourceStore {
    images: Arc<Mutex<std::collections::HashMap<String, Arc<MemoryAsset>>>>,
    budget: crate::ImageMemoryBudget,
}
impl ImageResourceStore {
    pub fn with_budget(&self, budget: crate::ImageMemoryBudget) -> Self {
        Self {
            images: self.images.clone(),
            budget,
        }
    }
    pub fn budget(&self) -> &crate::ImageMemoryBudget {
        &self.budget
    }
    pub fn memory_image(&self, asset: &ImageAsset) -> Option<PreparedImageContent> {
        Some(PreparedImageContent {
            memory: self
                .images
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&asset.content.as_ref()?.source)?
                .clone(),
            content: asset.content.clone()?,
        })
    }
}
impl super::DocumentSession {
    pub fn image_resource_store(&self) -> ImageResourceStore {
        ImageResourceStore {
            images: self.store.memory_assets.clone(),
            budget: self.store.image_budget.clone(),
        }
    }
    pub fn image_memory_budget(&self) -> &crate::ImageMemoryBudget {
        &self.store.image_budget
    }
    pub fn set_image_memory_budget(
        &mut self,
        budget: crate::ImageMemoryBudget,
    ) -> Result<(), Status> {
        let mut images = self
            .store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let captured: Vec<_> = images
            .iter()
            .map(|(source, memory)| {
                (
                    source.clone(),
                    memory.texture.clone(),
                    memory.png.lock().unwrap_or_else(|p| p.into_inner()).clone(),
                )
            })
            .collect();
        let mut buffers = Vec::new();
        for (_, texture, png) in &captured {
            buffers.push((&texture.rgba, crate::ImageBufferKind::Rgba));
            if let Some(png) = png {
                buffers.push((&png.bytes, crate::ImageBufferKind::Png));
            }
        }
        budget.track_buffers(&buffers)?;
        *images = captured
            .into_iter()
            .map(|(source, texture, png)| {
                (
                    source,
                    Arc::new(MemoryAsset {
                        texture,
                        png: Mutex::new(png),
                        budget: budget.clone(),
                    }),
                )
            })
            .collect();
        self.store.image_budget = budget;
        Ok(())
    }
    /// Keep formal/history descriptors and independently pinned images. No byte
    /// payload is copied; textures are separately tracked until their last Arc dies.
    pub fn collect_image_resources(&self, sources: &std::collections::HashSet<String>) -> usize {
        let mut images = self
            .store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let before = images.len();
        images.retain(|source, image| sources.contains(source) || Arc::strong_count(image) > 1);
        before - images.len()
    }
    pub fn memory_image_entries(&self) -> usize {
        self.store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }
}

/// Pin verified on-disk PNG bytes and pixels without installing a registry entry.
/// The descriptor remains the captured source, independent of later file writes.
pub fn prepare_project_image(
    root: &Path,
    asset: &ImageAsset,
    budget: &crate::ImageMemoryBudget,
) -> Result<PreparedImageContent, Status> {
    let path = asset_path(root, asset)?;
    let mut image = prepare_image_content_with_budget(&path, budget)?;
    let content = asset
        .content
        .as_ref()
        .ok_or_else(|| Status::error("MISSING_CONTENT", &asset.id))?;
    if image.content.width != content.width || image.content.height != content.height {
        return Err(Status::error("RESOURCE_DIMENSIONS", &asset.id));
    }
    if !content.sha256.is_empty() && image.content.sha256 != content.sha256 {
        return Err(Status::error("RESOURCE_HASH", &asset.id));
    }
    image.content = content.clone();
    Ok(image)
}
