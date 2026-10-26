//! A document, its persistence baseline, and authoring resource drafts.
use super::assets::MemoryAsset;
use super::import::read_psd_bytes;
use super::manifest::{publication_result, resolved_manifest};
use super::{
    read_project_asset, read_project_asset_if_changed, DocumentStore, ProjectResult,
    ResourceDiagnostic,
};
use crate::filesystem::{self as io, FileSystem, NativeFileSystem};
use crate::package::{
    publish_with_resources, PackageOptions, PackageValidation, RuntimeValidation,
};
use crate::resources::{decode_png, AssetData, TextureData};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zipzi_core::{Document, ImageAsset, Status};
use zipzi_moc3::ImportReport;
use zipzi_psd::ImportReport as PsdImportReport;

/// Project persistence and resource state. Authoring history belongs to the SDK.
pub struct DocumentSession {
    pub(super) store: DocumentStore,
    document: Document,
    manifest: PathBuf,
    manifest_sha256: String,
    pub(crate) import_sources: Vec<PathBuf>,
}

impl DocumentSession {
    pub fn new() -> Self {
        Self::with_filesystem(Arc::new(NativeFileSystem))
    }

    /// Start an in-memory project session. The SDK owns authoring history.
    pub fn from_authoring_document(document: Document) -> Self {
        let mut session = Self::new();
        session.reset_authoring_document(document);
        session
    }

    pub fn from_authoring_document_with_filesystem(
        document: Document,
        filesystem: Arc<dyn FileSystem>,
    ) -> Self {
        let mut session = Self::with_filesystem(filesystem);
        session.reset_authoring_document(document);
        session
    }

    /// Replace an SDK document while retaining the configured publication backend.
    pub fn reset_authoring_document(&mut self, document: Document) {
        self.store.clear_memory_assets();
        self.document = document;
        self.manifest = PathBuf::new();
        self.manifest_sha256.clear();
        self.import_sources.clear();
    }

    /// Publish an SDK candidate while retaining the saved baseline.
    pub fn publish_authoring_candidate(
        &mut self,
        candidate: Document,
        kind: zipzi_core::ChangeKind,
        identity_changed: bool,
    ) -> Result<bool, Status> {
        self.document
            .publish_candidate(candidate, kind, identity_changed)
    }

    /// Restore a complete SDK checkpoint, retaining manifest and saved baseline.
    pub fn swap_authoring_checkpoint(
        &mut self,
        checkpoint: &mut zipzi_core::document::DocumentCheckpoint,
    ) -> Result<(), Status> {
        self.document.exchange_checkpoint(checkpoint)
    }

    pub fn with_filesystem(filesystem: Arc<dyn FileSystem>) -> Self {
        Self {
            store: DocumentStore::with_filesystem(filesystem),
            document: Document::new(),
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
            import_sources: Vec::new(),
        }
    }

    pub(crate) fn store_import_candidate(&mut self, candidate: Self, document: Document) {
        self.reset_authoring_document(document);
        *self
            .store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = candidate
            .store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        self.import_sources = candidate.import_sources;
        self.store.remember_imported_assets(&self.document);
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    pub fn root(&self) -> PathBuf {
        self.manifest
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    }

    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    pub fn open(&mut self, path: &Path) -> ProjectResult {
        if self.document.transaction_active() {
            return ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel first");
        }
        let (result, snapshot) = self.store.open(path);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            self.store.clear_memory_assets();
            self.import_sources.clear();
            self.document = s.document;
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        result
    }

    /// Atomically open a document for the SDK, including final graph validation.
    /// Resource diagnostics are returned separately and do not reject an editable project.
    pub fn open_authoring(&mut self, path: &Path) -> ProjectResult {
        if self.document.transaction_active() {
            return ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first");
        }
        let (result, snapshot) = self.store.open(path);
        if !result.status.is_ok() {
            return result;
        }
        let snapshot = snapshot.expect("successful open has a snapshot");
        if let Some(issue) = snapshot.document.validate_structure().into_iter().next() {
            return ProjectResult::from_status(issue.status);
        }
        self.store.clear_memory_assets();
        self.import_sources.clear();
        self.document = snapshot.document;
        self.manifest = snapshot.manifest;
        self.manifest_sha256 = snapshot.manifest_sha256;
        result
    }

    pub fn save(&mut self, path: &Path) -> ProjectResult {
        if self.document.transaction_active() {
            return ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel first");
        }
        let target = match resolved_manifest(path) {
            Ok(path) => path,
            Err(s) => return ProjectResult::from_status(s),
        };
        let expected = if self.manifest == target {
            Some(self.manifest_sha256.as_str())
        } else {
            None
        };
        let source_root = self.root();
        let (result, snapshot) = self
            .store
            .save(&self.document, &source_root, path, expected);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            self.document = s.document;
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        result
    }

    pub fn import_model3(&mut self, path: &Path) -> (ProjectResult, Option<ImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel edits first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_model3(path);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            if let Err(status) = self.install_runtime_import(s, path) {
                return (ProjectResult::from_status(status), None);
            }
        }
        (result, report)
    }

    /// Import an external model for SDK editing, publishing only a structurally
    /// valid decoded document. Resource diagnostics remain non-fatal.
    pub fn import_model3_authoring(
        &mut self,
        path: &Path,
    ) -> (ProjectResult, Option<ImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_model3(path);
        if !result.status.is_ok() {
            return (result, None);
        }
        let snapshot = snapshot.expect("successful import has a snapshot");
        if let Some(issue) = snapshot.document.validate_structure().into_iter().next() {
            return (ProjectResult::from_status(issue.status), None);
        }
        if let Err(status) = self.install_runtime_import(snapshot, path) {
            return (ProjectResult::from_status(status), None);
        }
        (result, report)
    }

    pub fn import_bare_moc3(
        &mut self,
        moc3_path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
    ) -> (ProjectResult, Option<ImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel edits first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_bare_moc3(moc3_path, texture_map);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            if let Err(status) = self.install_runtime_import(s, moc3_path) {
                return (ProjectResult::from_status(status), None);
            }
        }
        (result, report)
    }

    /// Import bare MOC3 for SDK editing with the same atomic validation gate.
    pub fn import_bare_moc3_authoring(
        &mut self,
        moc3_path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
    ) -> (ProjectResult, Option<ImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_bare_moc3(moc3_path, texture_map);
        if !result.status.is_ok() {
            return (result, None);
        }
        let snapshot = snapshot.expect("successful import has a snapshot");
        if let Some(issue) = snapshot.document.validate_structure().into_iter().next() {
            return (ProjectResult::from_status(issue.status), None);
        }
        if let Err(status) = self.install_runtime_import(snapshot, moc3_path) {
            return (ProjectResult::from_status(status), None);
        }
        (result, report)
    }

    fn install_runtime_import(
        &mut self,
        snapshot: super::DocumentSnapshot,
        source: &Path,
    ) -> Result<(), Status> {
        let mut candidate = Self::from_authoring_document(snapshot.document);
        candidate.set_image_memory_budget(self.image_memory_budget().clone())?;
        candidate.import_sources = candidate
            .document()
            .asset_order()
            .iter()
            .filter_map(|id| {
                let content = candidate.document().get_asset(id)?.content.as_ref()?;
                fs::canonicalize(&content.source).ok()
            })
            .collect();
        candidate
            .import_sources
            .push(fs::canonicalize(source).map_err(io::io_error)?);
        let atlases = candidate.document().asset_order().iter().cloned().collect();
        candidate.unpack_atlas_images(&atlases)?;
        let document = candidate.document().clone();
        self.store_import_candidate(candidate, document);
        Ok(())
    }

    /// Import a PSD into a newly published project before replacing the
    /// current authoring document. Failures leave the session intact.
    pub fn import_psd_authoring(
        &mut self,
        source: &Path,
        destination: &Path,
    ) -> (ProjectResult, Option<PsdImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_psd(source, destination);
        if !result.status.is_ok() {
            return (result, None);
        }
        let snapshot = snapshot.expect("successful PSD import has a snapshot");
        self.store.clear_memory_assets();
        self.import_sources.clear();
        self.document = snapshot.document;
        self.manifest = snapshot.manifest;
        self.manifest_sha256 = snapshot.manifest_sha256;
        (result, report)
    }

    /// Retain PSD pixels in memory; encode PNGs only when read or saved.
    /// Nothing is published, and failed imports preserve the current session.
    pub fn import_psd_in_memory_authoring(
        &mut self,
        source: &Path,
    ) -> (ProjectResult, Option<PsdImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first"),
                None,
            );
        }
        let length = match std::fs::metadata(source) {
            Ok(m) => m.len() as usize,
            Err(e) => return (ProjectResult::failed("PROJECT_IO", &e.to_string()), None),
        };
        let _encoded = match self
            .store
            .image_budget
            .reserve_transfer(length.saturating_mul(2))
        {
            Ok(v) => v,
            Err(s) => return (ProjectResult::from_status(s), None),
        };
        let limit = (self.store.image_budget.available_bytes() / 3).min(256 << 20);
        if limit == 0 {
            return (
                ProjectResult::failed("RESOURCE_BUDGET_EXCEEDED", "PSD decoder budget"),
                None,
            );
        }
        let reservation = match self.store.image_budget.reserve(limit.saturating_mul(3)) {
            Ok(v) => v,
            Err(s) => return (ProjectResult::from_status(s), None),
        };
        let bundle = match read_psd_bytes(source).and_then(|bytes| {
            zipzi_psd::import_psd_pixels_with_limit(&bytes, limit)
                .map_err(|error| Status::error(error.code, error.message))
        }) {
            Ok(bundle) => bundle,
            Err(status) => return (ProjectResult::from_status(status), None),
        };
        if bundle.document.estimated_content_bytes() > limit {
            return (
                ProjectResult::failed(
                    "RESOURCE_BUDGET_EXCEEDED",
                    "PSD document exceeds decoder budget",
                ),
                None,
            );
        }
        let mut document = bundle.document;
        let mut assets = HashMap::new();
        for pixels in bundle.assets {
            let mut asset = document
                .get_asset(&pixels.id)
                .expect("PSD bundle asset exists")
                .clone();
            // Keep draft sources distinct from the content-addressed paths
            // created by save, so saved files always get read from disk.
            asset.content.as_mut().unwrap().source =
                format!("assets/.memory-psd/{}.png", pixels.id);
            let rgba: Arc<[u8]> = pixels.rgba.into();
            assets.insert(
                asset.content.as_ref().unwrap().source.clone(),
                Arc::new(MemoryAsset {
                    texture: TextureData {
                        rgba,
                        width: pixels.width,
                        height: pixels.height,
                        cache_key: format!("psd:{}", pixels.id),
                    },
                    png: Mutex::new(None),
                    budget: self.store.image_budget.clone(),
                }),
            );
            let edit = document.replace_asset(asset);
            if !edit.status.is_ok() {
                return (ProjectResult::from_status(edit.status), None);
            }
        }
        let buffers: Vec<_> = assets
            .values()
            .map(|image| (&image.texture.rgba, crate::ImageBufferKind::Rgba))
            .collect();
        if let Err(status) = reservation.commit_many(&buffers) {
            return (ProjectResult::from_status(status), None);
        }
        self.store.remember_imported_assets(&document);
        *self
            .store
            .memory_assets
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = assets;
        self.import_sources.clear();
        self.document = document;
        self.manifest = PathBuf::new();
        self.manifest_sha256.clear();
        let result = ProjectResult {
            status: Status::ok(),
            warnings: bundle.report.warnings.clone(),
            ..Default::default()
        };
        (result, Some(bundle.report))
    }

    /// Prepare immutable pixels with an explicit lease, without registering an
    /// asset. Keep the returned image until a candidate or resource store owns it.
    pub fn prepare_rgba_image(
        &self,
        width: u32,
        height: u32,
        rgba: Arc<[u8]>,
    ) -> Result<super::PreparedImageContent, Status> {
        super::PreparedImageContent::from_rgba(width, height, rgba, &self.store.image_budget)
    }

    /// Whether this source is owned by the current draft rather than a file.
    pub fn has_memory_asset(&self, asset: &ImageAsset) -> bool {
        self.store.has_memory_asset(asset)
    }

    pub fn diagnose(&self) -> Vec<ResourceDiagnostic> {
        self.store.diagnose(&self.document, &self.root())
    }

    pub fn read_asset(&self, asset_id: &str) -> Result<AssetData, Status> {
        self.read_asset_if_changed(asset_id, None)
            .map(|data| data.unwrap())
    }

    /// Read pixels without materializing a PNG for an in-memory PSD draft.
    /// Disk assets are always read and validated against current file bytes.
    pub fn read_texture(&self, asset_id: &str) -> Result<TextureData, Status> {
        let asset = self
            .document
            .get_asset(asset_id)
            .ok_or_else(|| Status::error("NOT_FOUND", asset_id))?;
        self.read_texture_asset(asset)
    }

    /// Resolve a candidate descriptor without publishing it to the document.
    pub fn read_texture_asset(&self, asset: &ImageAsset) -> Result<TextureData, Status> {
        if let Some(memory) = self.store.memory_asset(asset) {
            memory.validate_metadata(asset)?;
            return Ok(memory.texture.clone());
        }
        super::assets::read_project_asset(&self.root(), asset).map(Into::into)
    }

    pub fn read_asset_if_changed(
        &self,
        asset_id: &str,
        validated_image: Option<(&str, u32, u32)>,
    ) -> Result<Option<AssetData>, Status> {
        let asset = self
            .document
            .get_asset(asset_id)
            .ok_or_else(|| Status::error("NOT_FOUND", asset_id))?;
        let Some(memory) = self.store.memory_asset(asset) else {
            return read_project_asset_if_changed(&self.root(), asset, validated_image);
        };
        memory.validate_metadata(asset)?;
        let verified = memory.encoded_png()?;
        if validated_image.is_some_and(|(hash, width, height)| {
            hash == verified.sha256
                && width == asset.content.as_ref().unwrap().width
                && height == asset.content.as_ref().unwrap().height
        }) {
            return Ok(None);
        }
        Ok(Some(AssetData {
            bytes: verified.bytes.to_vec(),
            rgba: memory.texture.rgba.to_vec(),
            width: memory.texture.width,
            height: memory.texture.height,
            sha256: verified.sha256.clone(),
        }))
    }

    pub fn relocate_asset(&mut self, id: &str, path: &Path) -> zipzi_core::types::EditResult {
        if self.document.transaction_active() {
            return zipzi_core::types::EditResult {
                status: Status::error(
                    "TRANSACTION_ACTIVE",
                    "Commit or cancel the transaction first",
                ),
                ..Default::default()
            };
        }
        let path = match io::local_path(path) {
            Ok(path) => path,
            Err(status) => {
                return zipzi_core::types::EditResult {
                    status,
                    ..Default::default()
                };
            }
        };
        let existing = match self.document.get_asset(id) {
            Some(a) => a.clone(),
            None => {
                return zipzi_core::types::EditResult {
                    status: Status::error("MISSING_ASSET", id),
                    ..Default::default()
                };
            }
        };
        let mut asset = existing;
        if asset.content.is_none() {
            return zipzi_core::types::EditResult {
                status: Status::error("MISSING_IMAGE_CONTENT", id),
                ..Default::default()
            };
        }
        asset.content.as_mut().unwrap().source = path.to_string_lossy().to_string();
        let data = match read_project_asset(Path::new(""), &asset) {
            Ok(d) => d,
            Err(s) => {
                return zipzi_core::types::EditResult {
                    status: s,
                    ..Default::default()
                };
            }
        };
        asset.content.as_mut().unwrap().sha256 = data.sha256;
        self.document.replace_asset(asset)
    }

    pub fn replace_asset(&mut self, id: &str, path: &Path) -> zipzi_core::types::EditResult {
        if self.document.transaction_active() {
            return zipzi_core::types::EditResult {
                status: Status::error(
                    "TRANSACTION_ACTIVE",
                    "Commit or cancel the transaction first",
                ),
                ..Default::default()
            };
        }
        let path = match io::local_path(path) {
            Ok(path) => path,
            Err(status) => {
                return zipzi_core::types::EditResult {
                    status,
                    ..Default::default()
                };
            }
        };
        let existing = match self.document.get_asset(id) {
            Some(a) => a.clone(),
            None => {
                return zipzi_core::types::EditResult {
                    status: Status::error("MISSING_ASSET", id),
                    ..Default::default()
                };
            }
        };
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                return zipzi_core::types::EditResult {
                    status: Status::error("RESOURCE_IO", e.to_string()),
                    ..Default::default()
                };
            }
        };
        let data = match decode_png(&bytes) {
            Ok(d) => d,
            Err(s) => {
                return zipzi_core::types::EditResult {
                    status: s,
                    ..Default::default()
                };
            }
        };
        let mut asset = existing;
        asset.unresolved_source = None;
        asset.content = Some(zipzi_core::ImageContent {
            source: path.to_string_lossy().to_string(),
            sha256: data.sha256,
            width: data.width,
            height: data.height,
        });
        self.document.replace_asset(asset)
    }

    pub fn export_package(&self, destination: &Path) -> ProjectResult {
        if self.document.transaction_active() {
            return ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel first");
        }
        let options = PackageOptions {
            asset_root: self.root(),
            destination: destination.to_path_buf(),
            validate: Some(Box::new(|_artifact| {
                Ok(PackageValidation::structural(
                    if zipzi_moc3::HAS_CORE_VALIDATION {
                        RuntimeValidation::Passed
                    } else {
                        RuntimeValidation::Unavailable
                    },
                ))
            })),
        };
        match publish_with_resources(
            &self.document,
            &options,
            self.store.filesystem.as_ref(),
            &|asset| self.read_asset(&asset.id),
            &self.import_sources,
        ) {
            Ok(publication) => publication_result(publication),
            Err(status) => ProjectResult::from_status(status),
        }
    }
}

impl Default for DocumentSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod memory_psd_tests {
    use super::*;
    use zipzi_psd::import_psd;

    #[test]
    fn texture_reads_share_pixels_and_defer_png_without_skipping_metadata_checks() {
        let fixture = include_bytes!("../../../zipzi-psd/tests/fixtures/layered.psd");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let source = std::env::temp_dir().join(format!(
            "zipzi-memory-psd-{}-{nonce}.psd",
            std::process::id()
        ));
        fs::write(&source, fixture).unwrap();
        let mut session = DocumentSession::new();
        let (result, _) = session.import_psd_in_memory_authoring(&source);
        fs::remove_file(source).unwrap();
        assert!(result.status.is_ok(), "{:?}", result.status);
        let id = session.document.asset_order()[0].clone();
        let descriptor = session.document.get_asset(&id).unwrap().clone();
        let memory = session.store.memory_asset(&descriptor).unwrap();
        let first = session.read_texture(&id).unwrap();
        let second = session.read_texture(&id).unwrap();
        assert!(Arc::ptr_eq(&first.rgba, &second.rgba));
        assert!(session.diagnose().is_empty());
        assert!(memory.png.lock().unwrap().is_none());

        let mut invalid = descriptor.clone();
        invalid.content.as_mut().unwrap().width += 1;
        assert!(session.document.replace_asset(invalid).status.is_ok());
        assert_eq!(
            session.read_texture(&id).unwrap_err().code,
            "RESOURCE_DIMENSIONS"
        );
        assert!(memory.png.lock().unwrap().is_none());
        assert!(session
            .document
            .replace_asset(descriptor.clone())
            .status
            .is_ok());

        let eager = import_psd(fixture).unwrap();
        let data = session.read_asset(&id).unwrap();
        assert_eq!(data.bytes, eager.assets[0].bytes);
        assert_eq!(
            data.sha256,
            eager
                .document
                .get_asset(&id)
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .sha256
        );
        assert_eq!(data.rgba.as_slice(), first.rgba.as_ref());
        assert!(memory.png.lock().unwrap().is_some());
        assert!(session
            .read_asset_if_changed(&id, Some((&data.sha256, data.width, data.height)),)
            .unwrap()
            .is_none());

        let mut invalid = descriptor;
        invalid.content.as_mut().unwrap().sha256 = "bad hash".into();
        assert!(session.document.replace_asset(invalid).status.is_ok());
        assert_eq!(session.read_texture(&id).unwrap_err().code, "RESOURCE_HASH");
        assert_eq!(session.read_asset(&id).unwrap_err().code, "RESOURCE_HASH");
        assert_eq!(session.diagnose()[0].code, "RESOURCE_HASH");
    }
}
