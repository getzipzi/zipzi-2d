//! The single production project backend. Bare JSON stays in codec tooling.
use super::{DocumentSnapshot, DocumentStore, ProjectResult};
use crate::archive::{ProjectFingerprint, ProjectResourceLease};
use crate::filesystem::{self as io, Publication};
use std::path::{Path, PathBuf};
use zipzi_core::{Document, ImageAsset, Status};

/// One host's identity for an existing archive or a not-yet-created file.
/// The extension is a UI convention; explicit SDK paths are preserved.
pub fn project_identity(path: &Path) -> Result<PathBuf, Status> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(io::io_error)?.join(path)
    };
    io::reject_symlink(&absolute)?;
    let resolved = io::local_path(&absolute)?;
    if std::fs::metadata(&resolved).is_ok_and(|m| !m.is_file()) {
        return Err(Status::error(
            "UNSUPPORTED_PROJECT_PATH",
            "Expected a project file; directory projects are unsupported",
        ));
    }
    Ok(resolved)
}

impl DocumentStore {
    pub fn open(&self, path: &Path) -> (ProjectResult, Option<DocumentSnapshot>) {
        self.open_controlled(path, &mut || true)
    }
    pub fn open_controlled(
        &self,
        path: &Path,
        keep_going: &mut impl FnMut() -> bool,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        // Local SDK callers share the image/host pool as well. Runtime injects
        // its existing Work reservation so decoded results remain charged until
        // the owner thread consumes Completion.
        let mut archive = self.archive.clone();
        if !archive.has_open_memory() {
            let memory = match self.image_budget.reserve_transfer(0) {
                Ok(memory) => std::sync::Arc::new(memory),
                Err(status) => return (ProjectResult::from_status(status), None),
            };
            archive.set_open_memory(Some(memory));
        }
        match archive.open(path, keep_going) {
            Ok(opened) => {
                let resources = opened.resource_lease();
                let path = match project_identity(path) {
                    Ok(p) => p,
                    Err(s) => return (ProjectResult::from_status(s), None),
                };
                let result = ProjectResult {
                    diagnostics: opened.diagnostics,
                    ..ProjectResult::ok()
                };
                (
                    result,
                    Some(DocumentSnapshot {
                        document: opened.document,
                        path: Some(path),
                        fingerprint: Some(opened.fingerprint),
                        resources: Some(resources),
                    }),
                )
            }
            Err(error) => (ProjectResult::from_status(error.into()), None),
        }
    }
    pub fn new_resources(&self) -> Result<ProjectResourceLease, Status> {
        self.archive.new_resources().map_err(Into::into)
    }

    /// Bind resources before publication without decoding PNG pixels.
    pub fn bind_asset(
        &self,
        root: &Path,
        asset: &ImageAsset,
        resources: &ProjectResourceLease,
        keep_going: &mut impl FnMut() -> bool,
    ) -> Result<ImageAsset, Status> {
        let Some(content) = &asset.content else {
            return Ok(asset.clone());
        };
        let mut bound = asset.clone();
        let hash = if self.has_memory_asset(asset) {
            let data = self.read_verified_asset(root, asset)?;
            resources
                .insert_png(&data.bytes, &data.sha256)
                .map_err(Status::from)?;
            data.sha256
        } else {
            let source = super::assets::asset_path(root, asset)?;
            resources
                .bind_png(&source, content, keep_going)
                .map_err(Status::from)?
        };
        let target = bound.content.as_mut().unwrap();
        target.source = format!("assets/{hash}.png");
        target.sha256 = hash;
        Ok(bound)
    }
    pub fn prepare_document(
        &self,
        document: &Document,
        root: &Path,
        resources: &ProjectResourceLease,
        keep_going: &mut impl FnMut() -> bool,
    ) -> Result<Document, Status> {
        let mut candidate = document.fork_candidate();
        for id in document.asset_order() {
            if !keep_going() {
                return Err(Status::error(
                    "CANCELLED",
                    "Project resource preparation cancelled",
                ));
            }
            let bound =
                self.bind_asset(root, document.get_asset(id).unwrap(), resources, keep_going)?;
            if document.get_asset(id) == Some(&bound) {
                continue;
            }
            let content = bound.content.as_ref().expect("bound content");
            let edit = candidate.relocate_asset_storage(
                document.get_asset(id).unwrap(),
                content.source.clone(),
                content.sha256.clone(),
            );
            if !edit.status.is_ok() {
                return Err(edit.status);
            }
        }
        Ok(candidate)
    }
    pub fn save(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected: Option<&ProjectFingerprint>,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        let resources = match self.new_resources() {
            Ok(r) => r,
            Err(s) => return (ProjectResult::from_status(s), None),
        };
        let candidate = match self.prepare_document(document, source_root, &resources, &mut || true)
        {
            Ok(d) => d,
            Err(s) => return (ProjectResult::from_status(s), None),
        };
        self.save_prepared(&candidate, resources, path, expected, &mut || true)
    }
    pub(super) fn save_prepared(
        &self,
        document: &Document,
        resources: ProjectResourceLease,
        path: &Path,
        expected: Option<&ProjectFingerprint>,
        keep_going: &mut impl FnMut() -> bool,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        match self
            .archive
            .save(document, &resources, path, expected, keep_going)
        {
            Ok(receipt) => {
                let result = ProjectResult {
                    warnings: receipt.warnings,
                    durable: receipt.durable,
                    published: receipt.published,
                    ..Default::default()
                };
                (
                    result,
                    Some(DocumentSnapshot {
                        document: receipt.document,
                        path: Some(receipt.path),
                        fingerprint: Some(receipt.fingerprint),
                        resources: Some(resources),
                    }),
                )
            }
            Err(error) => (ProjectResult::from_status(error.into()), None),
        }
    }
}

pub(super) fn publication_result(publication: Publication) -> ProjectResult {
    ProjectResult {
        durable: publication.durable(),
        warnings: publication.warnings,
        ..ProjectResult::ok()
    }
}
