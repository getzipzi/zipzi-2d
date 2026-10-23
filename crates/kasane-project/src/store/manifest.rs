//! Project encoding, conflict checks, and atomic manifest publication.
use super::{DocumentSnapshot, DocumentStore, ProjectResult};
use crate::codec::{decode_project, encode_project};
#[cfg(feature = "binary-prototype")]
use crate::codec::{decode_project_cbor, encode_project_cbor};
use crate::filesystem::{self as io, Publication};
use crate::resources::content_sha256;
use kasane_core::{Document, Status};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn project_manifest(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|ext| ext == "json") {
        path.to_path_buf()
    } else {
        path.join("project.kasane.json")
    }
}

#[derive(Clone, Copy)]
enum ProjectEncoding {
    Json,
    #[cfg(feature = "binary-prototype")]
    Cbor,
}

impl ProjectEncoding {
    fn manifest(self, path: &Path) -> PathBuf {
        match self {
            Self::Json => project_manifest(path),
            #[cfg(feature = "binary-prototype")]
            Self::Cbor => {
                if path.extension().is_some_and(|ext| ext == "cbor") {
                    path.to_path_buf()
                } else {
                    path.join("project.kasane.cbor")
                }
            }
        }
    }

    fn decode(self, bytes: &[u8]) -> Result<Document, Status> {
        match self {
            Self::Json => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| Status::error("INVALID_PROJECT", "Manifest is not valid UTF-8"))?;
                decode_project(text)
            }
            #[cfg(feature = "binary-prototype")]
            Self::Cbor => decode_project_cbor(bytes),
        }
    }

    fn encode(self, document: &Document) -> Result<Vec<u8>, Status> {
        match self {
            Self::Json => encode_project(document).map(String::into_bytes),
            #[cfg(feature = "binary-prototype")]
            Self::Cbor => encode_project_cbor(document),
        }
    }
}

impl DocumentStore {
    pub fn open(&self, path: &Path) -> (ProjectResult, Option<DocumentSnapshot>) {
        self.open_inner(path, ProjectEncoding::Json)
    }

    #[cfg(feature = "binary-prototype")]
    pub fn open_cbor(&self, path: &Path) -> (ProjectResult, Option<DocumentSnapshot>) {
        self.open_inner(path, ProjectEncoding::Cbor)
    }

    fn open_inner(
        &self,
        path: &Path,
        encoding: ProjectEncoding,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        let manifest = match resolved_manifest_for(path, encoding) {
            Ok(path) => path,
            Err(s) => return (ProjectResult::from_status(s), None),
        };

        let bytes = match fs::read(&manifest) {
            Ok(b) => b,
            Err(e) => {
                return (
                    ProjectResult::failed("PROJECT_IO", &format!("{}: {}", manifest.display(), e)),
                    None,
                )
            }
        };

        let sha256 = content_sha256(&bytes);
        let document = match encoding.decode(&bytes) {
            Ok(d) => d,
            Err(s) => {
                return (ProjectResult::from_status(s), None);
            }
        };

        let root = manifest.parent().unwrap_or_else(|| Path::new("."));
        let diagnostics = self.diagnose(&document, root);

        let snapshot = DocumentSnapshot {
            document,
            manifest,
            manifest_sha256: sha256,
        };

        let mut res = ProjectResult::ok();
        res.diagnostics = diagnostics;
        (res, Some(snapshot))
    }

    pub fn save(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected_manifest_sha256: Option<&str>,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        self.save_with_encoding(
            document,
            source_root,
            path,
            expected_manifest_sha256,
            ProjectEncoding::Json,
        )
    }

    #[cfg(feature = "binary-prototype")]
    pub fn save_cbor(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected_manifest_sha256: Option<&str>,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        self.save_with_encoding(
            document,
            source_root,
            path,
            expected_manifest_sha256,
            ProjectEncoding::Cbor,
        )
    }

    fn save_with_encoding(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected_manifest_sha256: Option<&str>,
        encoding: ProjectEncoding,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        match self.save_inner(
            document,
            source_root,
            path,
            expected_manifest_sha256,
            encoding,
        ) {
            Ok((result, snapshot)) => (result, Some(snapshot)),
            Err(status) => (ProjectResult::from_status(status), None),
        }
    }

    fn save_inner(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected: Option<&str>,
        encoding: ProjectEncoding,
    ) -> Result<(ProjectResult, DocumentSnapshot), Status> {
        if document.transaction_active() {
            return Err(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel edits first",
            ));
        }
        if !document.initialized() {
            return Err(Status::error(
                "NOT_INITIALIZED",
                "Initialize Document first",
            ));
        }
        let manifest = resolved_manifest_for(path, encoding)?;
        let root = manifest
            .parent()
            .ok_or_else(|| Status::error("INVALID_PATH", "Missing project directory"))?;
        fs::create_dir_all(root).map_err(io::io_error)?;
        let _lock = io::lock(root)?;
        check_target(&manifest, expected)?;
        let assets_dir = root.join("assets");
        io::reject_symlink(&assets_dir)?;
        fs::create_dir_all(&assets_dir).map_err(io::io_error)?;
        let stage = io::Stage::new(root)?;
        let files = self.filesystem.as_ref();
        let mut candidate = document.fork_candidate();
        for id in document.asset_order() {
            let mut asset = document.get_asset(id).unwrap().clone();
            if asset.content.is_none() {
                continue;
            }
            let data = self.read_verified_asset(source_root, &asset)?;
            // Start with 16 hex characters. Extend the hash prefix when an
            // occupied name belongs to different bytes; keep the full hash in
            // the manifest for integrity checks.
            let mut prefix_len = 16;
            let (name, target, reuse) = loop {
                let name = format!("{}.png", &data.sha256[..prefix_len]);
                let target = assets_dir.join(&name);
                match fs::symlink_metadata(&target) {
                    Ok(info) => {
                        let reuse = info.is_file()
                            && !info.file_type().is_symlink()
                            && content_sha256(&fs::read(&target).map_err(io::io_error)?)
                                == data.sha256;
                        if reuse {
                            break (name, target, true);
                        }
                        if prefix_len < 64 {
                            prefix_len += 8;
                            continue;
                        }
                        let name = format!("{}-{}.png", &data.sha256[..16], io::unique_name());
                        break (name.clone(), assets_dir.join(name), false);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        break (name, target, false)
                    }
                    Err(e) => return Err(io::io_error(e)),
                }
            };
            if !reuse {
                files
                    .write_new(&target, &data.bytes)
                    .map_err(io::io_error)?;
            }
            asset.content.as_mut().unwrap().source = format!("assets/{name}");
            asset.content.as_mut().unwrap().sha256 = data.sha256;
            let edit = candidate.replace_asset(asset);
            if !edit.status.is_ok() {
                return Err(edit.status);
            }
        }
        files.sync_directory(&assets_dir).map_err(io::io_error)?;
        files.sync_directory(root).map_err(io::io_error)?;
        let manifest_bytes = encoding.encode(&candidate)?;
        let temporary = stage.0.join("manifest.json");
        files
            .write_new(&temporary, &manifest_bytes)
            .map_err(io::io_error)?;
        check_target(&manifest, expected)?;
        let manifest_sha256 = content_sha256(&manifest_bytes);
        drop(manifest_bytes);
        // Prepare committed state before publishing. A post-commit sync failure is a warning.
        candidate.mark_saved();
        let snapshot = DocumentSnapshot {
            document: candidate,
            manifest: manifest.clone(),
            manifest_sha256,
        };
        files.rename(&temporary, &manifest).map_err(io::io_error)?;
        let result = publication_result(Publication::finish(files, root));
        Ok((result, snapshot))
    }
}

pub(super) fn resolved_manifest(path: &Path) -> Result<PathBuf, Status> {
    resolved_manifest_for(path, ProjectEncoding::Json)
}

fn resolved_manifest_for(path: &Path, encoding: ProjectEncoding) -> Result<PathBuf, Status> {
    let requested = encoding.manifest(path);
    io::reject_symlink(&requested)?;
    io::local_path(&requested)
}

fn check_target(manifest: &Path, expected: Option<&str>) -> Result<(), Status> {
    io::reject_symlink(manifest)?;
    match (fs::symlink_metadata(manifest), expected) {
        (Ok(_), None) => Err(Status::error(
            "DESTINATION_EXISTS",
            "Reopen the existing project before saving",
        )),
        (Ok(_), Some(hash)) => {
            let bytes = fs::read(manifest).map_err(io::io_error)?;
            if content_sha256(&bytes) != hash {
                return Err(Status::error(
                    "PROJECT_CONFLICT",
                    "On-disk manifest changed; reopen or save to a new path",
                ));
            }
            Ok(())
        }
        (Err(e), baseline) if e.kind() == std::io::ErrorKind::NotFound => {
            if baseline.is_some() {
                Err(Status::error(
                    "PROJECT_CONFLICT",
                    "Expected on-disk manifest not found",
                ))
            } else {
                Ok(())
            }
        }
        (Err(e), _) => Err(io::io_error(e)),
    }
}

pub(super) fn publication_result(publication: Publication) -> ProjectResult {
    ProjectResult {
        durable: publication.durable(),
        warnings: publication.warnings,
        ..ProjectResult::ok()
    }
}
