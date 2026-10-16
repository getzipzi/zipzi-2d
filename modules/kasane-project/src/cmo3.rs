//! Project and session adapters for the filesystem-independent CMO3 codec.
use crate::filesystem::{self as io, FileSystem, NativeFileSystem};
use crate::{DocumentSession, DocumentStore, Publication};
use kasane_cmo3::{encode_cmo3, Cmo3Artifact, Cmo3Error, Cmo3Options, Cmo3Report};
use kasane_core::{Document, ImageAsset};
use std::path::Path;

/// Encode a saved project's document, resolving and verifying PNGs under `root`.
pub fn encode_document_cmo3(
    document: &Document,
    root: &Path,
    options: &Cmo3Options,
) -> Result<Cmo3Artifact, Cmo3Error> {
    encode_cmo3(
        document,
        &|asset: &ImageAsset| {
            crate::read_project_asset(root, asset)
                .map(|data| data.bytes)
                .map_err(|status| Cmo3Error::Asset {
                    asset_id: asset.id.clone(),
                    message: status.message,
                })
        },
        options,
    )
}

/// Load a Kasane directory or manifest and encode its editable Cubism project.
/// No files are written; CLI callers choose the output location and publication.
pub fn encode_project_cmo3(
    source: &Path,
    options: &Cmo3Options,
) -> Result<Cmo3Artifact, Cmo3Error> {
    let (result, snapshot) = DocumentStore::new().open(source);
    let snapshot = snapshot.ok_or_else(|| {
        Cmo3Error::InvalidDocument(format!("{}: {}", result.status.code, result.status.message))
    })?;
    encode_document_cmo3(
        &snapshot.document,
        snapshot.manifest.parent().unwrap(),
        options,
    )
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
        |s: kasane_core::Status| Cmo3Error::InvalidDocument(format!("{}: {}", s.code, s.message));
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

impl DocumentSession {
    /// Includes lazily encoded in-memory PSD artwork and unsaved model edits.
    pub fn encode_cmo3(&self, options: &Cmo3Options) -> Result<Cmo3Artifact, Cmo3Error> {
        encode_cmo3(
            self.document(),
            &|asset: &ImageAsset| {
                self.read_asset(&asset.id)
                    .map(|data| data.bytes)
                    .map_err(|s| Cmo3Error::Asset {
                        asset_id: asset.id.clone(),
                        message: s.message,
                    })
            },
            options,
        )
    }
    pub fn export_cmo3(
        &self,
        destination: &Path,
        options: &Cmo3Options,
    ) -> Result<(Cmo3Report, Publication), Cmo3Error> {
        let artifact = self.encode_cmo3(options)?;
        let publication = publish_cmo3(&artifact, destination)?;
        Ok((artifact.report, publication))
    }
}
