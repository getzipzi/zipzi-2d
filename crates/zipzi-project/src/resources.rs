use sha2::{Digest, Sha256};
use std::sync::Arc;
use zipzi_core::types::Status;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetData {
    pub bytes: Vec<u8>,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

/// Pixels for rendering. The opaque cache key identifies immutable image
/// content; it need not be a PNG hash for an unsaved PSD draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureData {
    pub rgba: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub cache_key: String,
}

impl From<AssetData> for TextureData {
    fn from(data: AssetData) -> Self {
        Self {
            rgba: data.rgba.into(),
            width: data.width,
            height: data.height,
            cache_key: format!("png:{}", data.sha256),
        }
    }
}

pub fn content_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

pub const RGBA8_CONTENT_IDENTITY_VERSION: u32 = 1;

fn hash_controlled(
    mut hasher: Sha256,
    bytes: &[u8],
    mut keep_going: impl FnMut() -> bool,
) -> Result<String, Status> {
    if !keep_going() {
        return Err(Status::error("CANCELLED", "Image hashing cancelled"));
    }
    for chunk in bytes.chunks(64 << 10) {
        if !keep_going() {
            return Err(Status::error("CANCELLED", "Image hashing cancelled"));
        }
        hasher.update(chunk);
    }
    if !keep_going() {
        return Err(Status::error("CANCELLED", "Image hashing cancelled"));
    }
    let mut hash = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write;
        write!(&mut hash, "{byte:02x}").unwrap();
    }
    Ok(hash)
}

/// PNG byte identity, with bounded cancellation intervals and no pixel copy.
pub fn content_sha256_with_control(
    bytes: &[u8],
    keep_going: impl FnMut() -> bool,
) -> Result<String, Status> {
    hash_controlled(Sha256::new(), bytes, keep_going)
}

/// Versioned source/render identity for top-left, row-major, straight RGBA8.
/// Dimensions are explicit little-endian u32 values and are part of the domain.
pub fn rgba8_sha256_with_control(
    width: u32,
    height: u32,
    rgba: &[u8],
    keep_going: impl FnMut() -> bool,
) -> Result<String, Status> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(rgba.len()) {
        return Err(Status::error(
            "RESOURCE_DIMENSIONS",
            "RGBA length differs from source dimensions",
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"zipzi-rgba8-sha256-v1\0top-left\0row-major\0straight-rgba8\0");
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());
    hash_controlled(hasher, rgba, keep_going)
}

pub fn is_valid_asset_path(s: &str) -> bool {
    if !s.starts_with("assets/") || s.contains('\\') || s.contains(':') || s.contains('\0') {
        return false;
    }
    let p = std::path::Path::new(s);
    if p.is_absolute() || p.file_name().is_none() {
        return false;
    }
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            _ => return false,
        }
    }
    true
}

pub fn decode_png(bytes: &[u8]) -> Result<AssetData, Status> {
    let image = zipzi_core::image::decode_png(bytes)?;
    Ok(AssetData {
        bytes: bytes.to_vec(),
        rgba: image.rgba,
        width: image.width,
        height: image.height,
        sha256: content_sha256(bytes),
    })
}
