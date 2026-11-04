//! Static, advisory previews. Never opens a Document or expands project assets.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;

mod ffi;
mod reader;

pub const PREVIEW_PATH: &str = "preview.png";
pub const MAX_PREVIEW_EDGE: u32 = 1024;
pub const MAX_PREVIEW_BYTES: usize = 8 << 20;

#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Invalid preview: {0}")]
    Invalid(&'static str),
}
pub type Result<T> = std::result::Result<T, PreviewError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewDescriptor {
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}

/// Validated PNG bytes. A new instance must be generated from each saved snapshot.
#[derive(Debug, Clone)]
pub struct ProjectPreview {
    png: Arc<[u8]>,
    descriptor: PreviewDescriptor,
}
impl ProjectPreview {
    pub fn from_png(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() > MAX_PREVIEW_BYTES {
            return Err(PreviewError::Invalid("PNG byte limit"));
        }
        let mut decoder = png::Decoder::new(Cursor::new(&bytes));
        decoder.set_limits(png::Limits { bytes: 16 << 20 });
        let mut reader = decoder
            .read_info()
            .map_err(|_| PreviewError::Invalid("PNG header"))?;
        let info = reader.info();
        if info.width == 0
            || info.height == 0
            || info.width > MAX_PREVIEW_EDGE
            || info.height > MAX_PREVIEW_EDGE
            || info.animation_control.is_some()
            || info.bit_depth != png::BitDepth::Eight
            || info.color_type != png::ColorType::Rgba
        {
            return Err(PreviewError::Invalid("PNG must be bounded static RGBA8"));
        }
        let (width, height) = (info.width, info.height);
        let mut pixels = vec![0; reader.output_buffer_size()];
        reader
            .next_frame(&mut pixels)
            .map_err(|_| PreviewError::Invalid("PNG pixels"))?;
        reader
            .finish()
            .map_err(|_| PreviewError::Invalid("PNG end"))?;
        drop(reader);
        Ok(Self {
            descriptor: PreviewDescriptor {
                sha256: Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
                width,
                height,
            },
            png: bytes.into(),
        })
    }
    pub fn png(&self) -> &[u8] {
        &self.png
    }
    pub fn descriptor(&self) -> &PreviewDescriptor {
        &self.descriptor
    }
}

/// Reads only the ZIP index, format.json and preview.png, without extracting files.
/// A valid preview does not certify the remaining project content. Stricter shell
/// limits (8 MiB directory / 16,384 entries) can omit previews for large projects.
pub fn read_preview(input: impl Read + Seek) -> Result<Option<ProjectPreview>> {
    reader::read(input)
}
