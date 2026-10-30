//! Observation resource acquisition: bounded disk buffers or shared memory pixels.
use super::*;
use crate::resources::{content_sha256_with_control, rgba8_sha256_with_control};
use std::io::Read;
use zipzi_contract::runtime::ObservationContentIdentity;
use zipzi_core::image::{decode_png_bounded, png_decode_workspace_bytes, PngDecodeLimits};

/// Validated source pixels and representation-specific identity. Encoded input
/// is not retained. Memory textures share the existing image budget pool's Arc.
#[derive(Clone, Debug)]
pub struct VerifiedTexture {
    pub texture: TextureData,
    pub source_content_identity: ObservationContentIdentity,
}

fn check(keep_going: &mut impl FnMut() -> bool) -> Result<(), Status> {
    if keep_going() {
        Ok(())
    } else {
        Err(Status::error("CANCELLED", "Texture acquisition cancelled"))
    }
}

pub fn read_project_texture_bounded(
    root: &Path,
    asset: &ImageAsset,
    limits: PngDecodeLimits,
    mut keep_going: impl FnMut() -> bool,
) -> Result<VerifiedTexture, Status> {
    check(&mut keep_going)?;
    let content = asset.image_content()?;
    let path = asset_path(root, asset)?;
    let mut file =
        fs::File::open(&path).map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?;
    let length = file
        .metadata()
        .map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?
        .len();
    let length = usize::try_from(length)
        .ok()
        .filter(|n| *n <= limits.max_encoded_bytes)
        .ok_or_else(|| {
            Status::error(
                "ENCODED_READ_LIMIT",
                format!(
                    "{}: encoded PNG exceeds {} bytes",
                    asset.id, limits.max_encoded_bytes
                ),
            )
        })?;
    let required = png_decode_workspace_bytes(content.width, content.height, length)?;
    if required > limits.max_workspace_bytes {
        return Err(Status::error(
            "DECODE_WORKSPACE_LIMIT",
            format!(
                "{}: PNG decode requires {required} bytes, limit {}",
                asset.id, limits.max_workspace_bytes
            ),
        ));
    }
    check(&mut keep_going)?;
    // Fixed-size storage and exact reads prevent a file growing after metadata
    // inspection from causing an unbounded read or Vec capacity growth.
    let mut bytes = vec![0; length];
    for chunk in bytes.chunks_mut(64 << 10) {
        check(&mut keep_going)?;
        file.read_exact(chunk)
            .map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?;
    }
    check(&mut keep_going)?;
    if file
        .read(&mut [0])
        .map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?
        != 0
    {
        return Err(Status::error(
            "RESOURCE_CHANGED",
            "PNG length changed during bounded acquisition",
        ));
    }
    let hash = content_sha256_with_control(&bytes, &mut keep_going)?;
    if !content.sha256.is_empty() && content.sha256 != hash {
        return Err(Status::error(
            "RESOURCE_HASH",
            format!("{}: PNG differs from its declared SHA256", asset.id),
        ));
    }
    check(&mut keep_going)?;
    let image = decode_png_bounded(&bytes, (content.width, content.height), limits)?;
    check(&mut keep_going)?;
    drop(bytes);
    Ok(VerifiedTexture {
        texture: TextureData {
            rgba: image.rgba.into(),
            width: image.width,
            height: image.height,
            cache_key: format!("png:{hash}"),
        },
        source_content_identity: ObservationContentIdentity::PngSha256 { hash },
    })
}

impl PreparedImageContent {
    pub fn verified_texture(&self) -> Result<VerifiedTexture, Status> {
        self.verified_texture_with_control(|| true)
    }

    /// Validate captured metadata and reuse immutable source pixels. A declared
    /// PNG hash must already be available in the verified encoded cache; this
    /// path never invokes the PNG encoder or makes a full RGBA copy.
    pub fn verified_texture_with_control(
        &self,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<VerifiedTexture, Status> {
        check(&mut keep_going)?;
        let texture = &self.memory.texture;
        if (self.content.width, self.content.height) != (texture.width, texture.height) {
            return Err(Status::error(
                "RESOURCE_DIMENSIONS",
                "Memory texture dimensions differ from captured metadata",
            ));
        }
        if !self.content.sha256.is_empty() {
            let png = self.memory.png.lock().unwrap_or_else(|p| p.into_inner());
            if png
                .as_ref()
                .is_none_or(|png| png.sha256 != self.content.sha256)
            {
                return Err(Status::error(
                    "RESOURCE_HASH",
                    "Memory texture PNG identity differs from captured metadata",
                ));
            }
        }
        let hash = rgba8_sha256_with_control(
            texture.width,
            texture.height,
            &texture.rgba,
            &mut keep_going,
        )?;
        Ok(VerifiedTexture {
            texture: texture.clone(),
            source_content_identity: ObservationContentIdentity::Rgba8Sha256 { hash },
        })
    }
}

#[cfg(test)]
mod tests;
