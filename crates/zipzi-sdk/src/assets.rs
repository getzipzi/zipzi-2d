//! PNG descriptors, resource relocation and the rectangle mesh helper.
use std::fs;
use std::path::Path;

use crate::types::SdkError;
use zipzi_core::{ImageAsset, Mesh, Vec2};
use zipzi_project::{decode_png, is_valid_asset_path};

pub(crate) fn validate_asset_root(
    root: &Path,
    asset: &ImageAsset,
    operation: &'static str,
) -> Result<(), SdkError> {
    if asset.content.is_none() {
        return Ok(());
    }
    if Path::new(&asset.content.as_ref().unwrap().source).is_absolute() {
        return Ok(());
    }
    if root.as_os_str().is_empty()
        || !root.is_absolute()
        || !is_valid_asset_path(&asset.content.as_ref().unwrap().source)
        || root
            .join(&asset.content.as_ref().unwrap().source)
            .to_str()
            .is_none()
    {
        let mut error = SdkError::new(
            "INVALID_ASSET_BASE",
            "Relative asset source requires a valid absolute project root",
            operation,
        );
        error.object_ids.push(asset.id.clone());
        return Err(error);
    }
    Ok(())
}

/// Read a PNG once and return a validated, absolute-path resource description.
/// File publication is a separate `AuthoringSession::save_project` operation.
pub fn prepare_png_asset(id: &str, name: &str, path: &Path) -> Result<ImageAsset, SdkError> {
    if !path.is_absolute() {
        return Err(SdkError::new(
            "INVALID_PATH",
            "PNG path must be absolute",
            "prepare_png_asset",
        ));
    }
    let path = fs::canonicalize(path)
        .map_err(|e| SdkError::new("RESOURCE_IO", &e.to_string(), "prepare_png_asset"))?;
    let bytes = fs::read(&path)
        .map_err(|e| SdkError::new("RESOURCE_IO", &e.to_string(), "prepare_png_asset"))?;
    let data = decode_png(&bytes)
        .map_err(|s| SdkError::from_status(s, "prepare_png_asset", vec![id.into()]))?;
    let source = path.to_str().ok_or_else(|| {
        SdkError::new(
            "INVALID_PATH",
            "PNG path cannot be represented as a document source string",
            "prepare_png_asset",
        )
    })?;
    Ok(ImageAsset {
        unresolved_source: None,
        id: id.into(),
        name: name.into(),
        content: Some(zipzi_core::ImageContent {
            source: source.into(),
            width: data.width,
            height: data.height,
            sha256: data.sha256,
        }),
    })
}

/// Resolve a user-supplied relative PNG path from an explicit absolute base.
pub fn prepare_png_asset_from_base(
    id: &str,
    name: &str,
    base: &Path,
    relative: &Path,
) -> Result<ImageAsset, SdkError> {
    if !base.is_absolute() || relative.is_absolute() {
        return Err(SdkError::new(
            "INVALID_PATH",
            "Base must be absolute and PNG path relative",
            "prepare_png_asset_from_base",
        ));
    }
    prepare_png_asset(id, name, &base.join(relative)).map_err(|mut error| {
        error.operation = "prepare_png_asset_from_base";
        error
    })
}

/// Prepare a new source path for the same image bytes and dimensions. Submit
/// the returned descriptor with `EditSession::replace_asset` inside an edit.
pub fn prepare_relocated_asset(existing: &ImageAsset, path: &Path) -> Result<ImageAsset, SdkError> {
    let content = existing.image_content().map_err(|s| {
        SdkError::from_status(s, "prepare_relocated_asset", vec![existing.id.clone()])
    })?;
    let prepared = prepare_png_asset(&existing.id, &existing.name, path).map_err(|mut error| {
        error.operation = "prepare_relocated_asset";
        error
    })?;
    if prepared.content.as_ref().unwrap().width != content.width
        || prepared.content.as_ref().unwrap().height != content.height
    {
        let mut error = SdkError::new(
            "RESOURCE_DIMENSIONS",
            "Relocated PNG dimensions differ from the current asset",
            "prepare_relocated_asset",
        );
        error.object_ids.push(existing.id.clone());
        return Err(error);
    }
    if !content.sha256.is_empty() && prepared.content.as_ref().unwrap().sha256 != content.sha256 {
        let mut error = SdkError::new(
            "RESOURCE_HASH",
            "Relocated PNG content differs from the current asset",
            "prepare_relocated_asset",
        );
        error.object_ids.push(existing.id.clone());
        return Err(error);
    }
    Ok(prepared)
}

/// Positions are source canvas pixels for a root mesh. UVs follow core source
/// convention and are never flipped in the document.
pub fn rectangle_mesh(
    id: &str,
    name: &str,
    asset_id: &str,
    min: Vec2,
    max: Vec2,
) -> Result<Mesh, SdkError> {
    if !min.x.is_finite()
        || !min.y.is_finite()
        || !max.x.is_finite()
        || !max.y.is_finite()
        || min.x >= max.x
        || min.y >= max.y
    {
        return Err(SdkError::new(
            "INVALID_RECTANGLE",
            "Finite min must be below max",
            "rectangle_mesh",
        ));
    }
    Ok(Mesh {
        id: id.into(),
        name: name.into(),
        texture_asset_id: asset_id.into(),
        vertex_ids: vec![0, 1, 2, 3],
        base_positions: vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        ..Mesh::default()
    })
}
