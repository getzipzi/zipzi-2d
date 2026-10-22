//! Disk resource validation and lazily encoded in-memory draft textures.
use super::{DocumentStore, ResourceDiagnostic};
use crate::filesystem as io;
use crate::resources::{content_sha256, decode_png, AssetData, TextureData};
use kasane_core::{Document, ImageAsset, Status};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

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
    pub(super) png: OnceLock<Result<VerifiedAsset, Status>>,
}

impl MemoryAsset {
    pub(super) fn encoded_png(&self) -> Result<&VerifiedAsset, Status> {
        self.png
            .get_or_init(|| {
                let mut bytes = Vec::new();
                {
                    let mut encoder =
                        png::Encoder::new(&mut bytes, self.texture.width, self.texture.height);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder
                        .write_header()
                        .map_err(|error| Status::error("PNG_ENCODE", error.to_string()))?;
                    writer
                        .write_image_data(&self.texture.rgba)
                        .map_err(|error| Status::error("PNG_ENCODE", error.to_string()))?;
                }
                let sha256 = content_sha256(&bytes);
                Ok(VerifiedAsset { bytes, sha256 })
            })
            .as_ref()
            .map_err(Clone::clone)
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
    pub(super) bytes: Vec<u8>,
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
                let image = kasane_core::image::decode_png(&bytes).map_err(|mut status| {
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
        Ok(VerifiedAsset { bytes, sha256 })
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
    content: kasane_core::ImageContent,
}
impl std::fmt::Debug for PreparedImageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedImageContent")
            .field("content", &self.content)
            .finish_non_exhaustive()
    }
}
impl PreparedImageContent {
    pub fn content(&self) -> &kasane_core::ImageContent {
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
            bytes: png.bytes.clone(),
            rgba: self.memory.texture.rgba.to_vec(),
            width: self.memory.texture.width,
            height: self.memory.texture.height,
            sha256: png.sha256.clone(),
        })
    }
}
pub fn prepare_image_content(path: &Path) -> Result<PreparedImageContent, Status> {
    if !path.is_absolute() {
        return Err(Status::error("INVALID_PATH", "PNG source must be absolute"));
    }
    let metadata = fs::metadata(path).map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err(Status::error("CAPACITY", "PNG source exceeds 64 MiB"));
    }
    let bytes = fs::read(path).map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(Status::error("CAPACITY", "PNG source exceeds 64 MiB"));
    }
    if bytes.get(..8) == Some(b"\x89PNG\r\n\x1a\n") {
        if let Some(header) = bytes.get(16..24) {
            let width = u32::from_be_bytes(header[..4].try_into().unwrap());
            let height = u32::from_be_bytes(header[4..].try_into().unwrap());
            if width > 16384
                || height > 16384
                || u64::from(width) * u64::from(height) * 4 > 256 * 1024 * 1024
            {
                return Err(Status::error(
                    "CAPACITY",
                    "PNG pixels exceed resource limits",
                ));
            }
        }
    }
    let data = decode_png(&bytes)?;
    let content = kasane_core::ImageContent {
        source: format!("assets/.memory-loaded/{}.png", data.sha256),
        width: data.width,
        height: data.height,
        sha256: data.sha256.clone(),
    };
    let memory = MemoryAsset {
        texture: TextureData {
            rgba: data.rgba.into(),
            width: data.width,
            height: data.height,
            cache_key: format!("png:{}", data.sha256),
        },
        png: OnceLock::from(Ok(VerifiedAsset {
            bytes: data.bytes,
            sha256: data.sha256,
        })),
    };
    Ok(PreparedImageContent {
        memory: Arc::new(memory),
        content,
    })
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
    pub fn memory_image(&self, asset: &ImageAsset) -> Option<PreparedImageContent> {
        Some(PreparedImageContent {
            memory: self.store.memory_asset(asset)?,
            content: asset.content.clone()?,
        })
    }
}
