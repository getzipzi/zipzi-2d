//! Project and session adapters for the filesystem-independent CMO3 codec.
use crate::filesystem::{self as io, FileSystem, NativeFileSystem};
use crate::{ProjectIo, Publication, ResourceStore, ResourceView};
use std::path::Path;
use zipzi_cmo3::{encode_cmo3, Cmo3Artifact, Cmo3Error, Cmo3Options};
use zipzi_core::state::{AssetRecord, DocumentState};

/// Encode a saved project's document, resolving and verifying PNGs under `root`.
pub fn encode_document_cmo3(
    document: &DocumentState,
    resources: &ResourceView,
    options: &Cmo3Options,
) -> Result<Cmo3Artifact, Cmo3Error> {
    encode_cmo3(
        document,
        &|asset: &AssetRecord| {
            resources
                .read_asset(asset)
                .map(|data| data.bytes)
                .map_err(|status| Cmo3Error::Asset {
                    asset_id: asset.id.clone(),
                    message: status.message,
                })
        },
        options,
    )
}

/// Load a Zipzi directory or manifest and encode its editable Cubism project.
/// No files are written; CLI callers choose the output location and publication.
pub fn encode_project_cmo3(
    source: &Path,
    options: &Cmo3Options,
) -> Result<Cmo3Artifact, Cmo3Error> {
    let snapshot = ProjectIo::new(std::env::temp_dir(), ResourceStore::default())
        .load(source, || true)
        .map_err(|e| Cmo3Error::InvalidDocument(e.to_string()))?;
    encode_document_cmo3(&snapshot.document, &snapshot.resources, options)
}

/// Publish captured CMO3 bytes by staging and atomic rename. Encoding failures
/// happen before this call, so an existing destination remains untouched.
pub fn publish_cmo3(artifact: &Cmo3Artifact, destination: &Path) -> Result<Publication, Cmo3Error> {
    publish_bytes(&artifact.bytes, destination, &NativeFileSystem)
}
pub(crate) fn publish_bytes(
    bytes: &[u8],
    destination: &Path,
    files: &dyn FileSystem,
) -> Result<Publication, Cmo3Error> {
    let status =
        |s: zipzi_core::Status| Cmo3Error::InvalidDocument(format!("{}: {}", s.code, s.message));
    io::reject_symlink(destination).map_err(status)?;
    let destination = io::local_path(destination).map_err(status)?;
    let parent = destination
        .parent()
        .filter(|_| destination.file_name().is_some())
        .ok_or_else(|| Cmo3Error::InvalidDocument("Output cannot be a filesystem root".into()))?;
    let stage = io::Stage::new(parent).map_err(status)?;
    let staged = stage.0.join("model.cmo3");
    files.write_new(&staged, bytes)?;
    files.rename(&staged, &destination)?;
    Ok(Publication::finish(files, parent))
}
