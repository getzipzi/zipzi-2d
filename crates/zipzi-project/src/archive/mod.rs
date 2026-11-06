//! Native single-file project container, resource leases and atomic publication.

mod progress;
mod publication;
mod reader;
pub use progress::{ProjectSaveProgress, ProjectSaveStage, ProjectSaveState};
mod stream;
mod structure;
mod working_set;
mod writer;
pub use publication::{
    ArchiveFileSystem, ArchiveLock, ArchiveStaging, ArchiveStream, NativeArchiveFileSystem,
};
pub use working_set::{ProjectDiskBudget, ProjectResourceLease};
pub use zipzi_project_preview::{PreviewDescriptor, ProjectPreview};

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};

use serde::{Deserialize, Serialize};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};
use zipzi_core::Status;

use crate::{content_sha256, read_project_texture_bounded};

pub use stream::{read_fingerprint, IoCounts, MeteredIo, ProjectFingerprint};

const EXPERIMENT: &str = "prototype-directory-document-v21";
const NATIVE_FORMAT: &str = "zipzi-project";
const NATIVE_DOCUMENT_VERSION: u64 = 22;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{code}: {message}")]
    Document { code: String, message: String },
    #[error("Invalid project archive: {0}")]
    Invalid(String),
    #[error("Project limit exceeded: {0}")]
    Limit(&'static str),
    #[error("Archive operation cancelled before completion")]
    Cancelled,
}

impl From<Status> for ArchiveError {
    fn from(status: Status) -> Self {
        Self::Document {
            code: status.code,
            message: status.message,
        }
    }
}

impl From<ArchiveError> for Status {
    fn from(error: ArchiveError) -> Self {
        match error {
            ArchiveError::Document { code, message } => Status::error(code, message),
            ArchiveError::Cancelled => {
                Status::error("CANCELLED", "Project I/O cancelled before publication")
            }
            ArchiveError::Limit(message) => Status::error("PROJECT_LIMIT_EXCEEDED", message),
            ArchiveError::Io(error) => Status::error("PROJECT_IO", error.to_string()),
            error => Status::error("INVALID_PROJECT", error.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

/// Compatibility name for the opt-in P0 benchmark API.
#[cfg(any(test, feature = "archive-prototype"))]
pub type PrototypeLimits = ArchiveIoLimits;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Format {
    format: String,
    container_version: u32,
    document: String,
    document_sha256: String,
    required_features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preview: Option<serde_json::Value>,
}

pub struct WrittenArchive<D = ()> {
    /// Same authoring data with complete content-addressed asset paths.
    pub document: D,
    pub unique_assets: usize,
    pub asset_bytes: u64,
    pub document_bytes: u64,
    /// Logical reads for resource preparation and verification during writing.
    pub resource_read_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveDocumentFormat {
    NativeV22,
}

#[derive(Debug, Clone)]
pub struct ArchiveSaveReceipt<D = ()> {
    pub document: D,
    pub path: PathBuf,
    pub fingerprint: ProjectFingerprint,
    pub published: bool,
    pub durable: bool,
    pub warnings: Vec<String>,
    pub document_bytes: u64,
    pub asset_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ArchiveIoLimits {
    pub archive_bytes: u64,
    pub directory_bytes: u64,
    pub format_bytes: u64,
    pub document_bytes: u64,
    pub expanded_bytes: u64,
    pub entries: usize,
}

impl Default for ArchiveIoLimits {
    fn default() -> Self {
        Self {
            archive_bytes: 16 << 30,
            directory_bytes: 64 << 20,
            format_bytes: 64 << 10,
            document_bytes: 256 << 20,
            expanded_bytes: 16 << 30,
            entries: 100_000,
        }
    }
}

#[derive(Clone)]
pub struct ProjectArchiveStore {
    workspace_parent: PathBuf,
    workspace_maintenance: Arc<Once>,
    limits: ArchiveIoLimits,
    filesystem: Arc<dyn ArchiveFileSystem>,
    disk_budget: ProjectDiskBudget,
    save_progress: Option<ProjectSaveProgress>,
    open_memory: Option<Arc<crate::TransferReservation>>,
    save_preview: Option<ProjectPreview>,
}

impl ProjectArchiveStore {
    pub fn new(workspace_parent: impl Into<PathBuf>) -> Self {
        Self {
            workspace_parent: workspace_parent.into(),
            workspace_maintenance: Arc::new(Once::new()),
            limits: ArchiveIoLimits::default(),
            filesystem: Arc::new(NativeArchiveFileSystem),
            disk_budget: ProjectDiskBudget::default(),
            save_progress: None,
            open_memory: None,
            save_preview: None,
        }
    }

    pub fn with_filesystem(mut self, filesystem: Arc<dyn ArchiveFileSystem>) -> Self {
        self.filesystem = filesystem;
        self
    }

    /// Scoped save input; hosts must supply a fresh image of the saved snapshot.
    pub fn set_save_preview(&mut self, preview: Option<ProjectPreview>) -> Option<ProjectPreview> {
        std::mem::replace(&mut self.save_preview, preview)
    }

    pub fn with_disk_budget(mut self, budget: ProjectDiskBudget) -> Self {
        self.disk_budget = budget;
        self
    }

    pub fn set_save_progress(
        &mut self,
        progress: Option<ProjectSaveProgress>,
    ) -> Option<ProjectSaveProgress> {
        std::mem::replace(&mut self.save_progress, progress)
    }

    /// Attach a host's per-operation reservation, retained through job completion.
    pub fn set_open_memory(
        &mut self,
        memory: Option<Arc<crate::TransferReservation>>,
    ) -> Option<Arc<crate::TransferReservation>> {
        std::mem::replace(&mut self.open_memory, memory)
    }

    pub fn with_limits(mut self, limits: ArchiveIoLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Capture a replacement baseline through the same path and full-file hash
    /// rules used immediately before publication. This does not mutate the file.
    pub fn fingerprint(
        &self,
        path: &Path,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<ProjectFingerprint> {
        crate::filesystem::reject_symlink(path)?;
        let path = crate::filesystem::local_path(path)?;
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(
                Status::error("UNSUPPORTED_PROJECT_PATH", "Expected a project file").into(),
            );
        }
        #[cfg(unix)]
        if std::os::unix::fs::MetadataExt::nlink(&metadata) > 1 {
            return Err(Status::error(
                "INVALID_PATH",
                "Hard-linked project paths are not writable",
            )
            .into());
        }
        crate::archive::read_fingerprint(
            &mut File::open(path)?,
            self.limits.archive_bytes,
            &mut keep_going,
        )
    }

    pub fn new_resources(&self) -> Result<ProjectResourceLease> {
        self.prepare_workspace()?;
        ProjectResourceLease::create(&self.workspace_parent, self.disk_budget.clone())
    }

    // Run on the I/O worker before its first allocation, shared by all store
    // clones in a Runtime. A new process reclaims the previous owner's leftovers;
    // live sessions/snapshots remain protected by their working-set owner locks.
    fn prepare_workspace(&self) -> Result<()> {
        if !self.workspace_parent.is_absolute() {
            return Err(invalid("Working set parent must be absolute"));
        }
        fs::create_dir_all(&self.workspace_parent)?;
        self.workspace_maintenance.call_once(|| {
            match ProjectResourceLease::clean_abandoned(&self.workspace_parent) {
                Ok(warnings) => {
                    for warning in warnings {
                        eprintln!("Project cache cleanup: {warning}");
                    }
                }
                Err(error) => eprintln!("Project cache cleanup: {error}"),
            }
        });
        Ok(())
    }

    fn publish_prepared<D>(
        &self,
        mut prepared: writer::PreparedArchive<D>,
        target: &Path,
        expected: Option<&ProjectFingerprint>,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<ArchiveSaveReceipt<D>> {
        check(&mut keep_going)?;
        crate::filesystem::reject_symlink(target)?;
        let target = crate::filesystem::local_path(target)?;
        let parent = target
            .parent()
            .ok_or_else(|| invalid("Missing target parent"))?;
        fs::create_dir_all(parent)?;
        let _lock = self
            .filesystem
            .acquire_lock(&target)
            .map_err(|e| Status::error("PROJECT_BUSY", e.to_string()))?;
        check_archive_target(&target, expected, &mut keep_going)?;
        let base_extent = prepared.extent;
        let base_format = prepared.preview_format();
        prepared.set_preview(self.save_preview.clone(), self.limits)?;
        let _reservation = match self.disk_budget.reserve(prepared.extent) {
            Ok(reservation) => reservation,
            Err(_) if prepared.extent != base_extent => {
                prepared.omit_preview(base_format, base_extent);
                self.disk_budget.reserve(prepared.extent)?
            }
            Err(error) => return Err(error),
        };
        let mut stage = self.filesystem.create_staging(parent)?;
        let mut writer = self.filesystem.staging_stream(&mut stage.file);
        let written = writer::write(
            prepared,
            &mut writer,
            &mut keep_going,
            self.save_progress.as_ref(),
        )?;
        drop(writer);
        self.filesystem.sync_file(&stage.file)?;
        let length = stage.file.metadata()?.len();
        structure::validate(&mut stage.file, length, self.limits, &mut keep_going)?;
        stage.file.rewind()?;
        let fingerprint = crate::archive::read_fingerprint(
            &mut stage.file,
            self.limits.archive_bytes,
            &mut keep_going,
        )?;
        check_archive_target(&target, expected, &mut keep_going)?;
        check(&mut keep_going)?;
        if expected.is_some() {
            self.filesystem.publish_replace(&stage.path, &target)?;
        } else {
            self.filesystem.publish_no_replace(&stage.path, &target)?;
        }
        let mut warnings = Vec::new();
        if let Err(error) = self.filesystem.sync_directory(parent) {
            warnings.push(format!(
                "Published, but directory synchronization failed: {error}"
            ));
        }
        if let Some(progress) = &self.save_progress {
            progress.set_stage(ProjectSaveStage::Published);
        }
        Ok(ArchiveSaveReceipt {
            document: written.document,
            path: target,
            fingerprint,
            published: true,
            durable: warnings.is_empty(),
            warnings,
            document_bytes: written.document_bytes,
            asset_bytes: written.asset_bytes,
        })
    }
}

fn check_archive_target(
    target: &Path,
    expected: Option<&ProjectFingerprint>,
    keep_going: &mut impl FnMut() -> bool,
) -> Result<()> {
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if !metadata.is_file() && !metadata.file_type().is_symlink() => {
            Err(Status::error(
                "UNSUPPORTED_PROJECT_PATH",
                "Destination must be a regular file; directory projects are unsupported",
            )
            .into())
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(Status::error("INVALID_PATH", "Destination is a symlink").into())
        }
        Ok(metadata) => {
            #[cfg(unix)]
            if std::os::unix::fs::MetadataExt::nlink(&metadata) > 1 {
                return Err(Status::error(
                    "INVALID_PATH",
                    "Hard-linked project paths are not writable",
                )
                .into());
            }
            if expected.is_none() {
                return Err(Status::error("PROJECT_EXISTS", "Destination exists").into());
            }
            let baseline = expected.expect("existing target requires a baseline");
            if baseline.length != metadata.len() {
                return Err(
                    Status::error("PROJECT_CONFLICT", "On-disk archive length changed").into(),
                );
            }
            let actual = crate::archive::read_fingerprint(
                &mut File::open(target)?,
                baseline.length,
                keep_going,
            )?;
            if expected.is_some_and(|baseline| baseline != &actual) {
                return Err(ArchiveError::Document {
                    code: "PROJECT_CONFLICT".into(),
                    message: "On-disk archive changed".into(),
                });
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if expected.is_some() {
                Err(ArchiveError::Document {
                    code: "PROJECT_CONFLICT".into(),
                    message: "Expected archive not found".into(),
                })
            } else {
                Ok(())
            }
        }
        Err(error) => Err(error.into()),
    }
}

enum ResourceInput {
    Bytes(Arc<[u8]>),
}
struct Resource {
    input: ResourceInput,
    fingerprint: ProjectFingerprint,
}

fn invalid(message: impl Into<String>) -> ArchiveError {
    ArchiveError::Invalid(message.into())
}

pub(super) fn check(keep_going: &mut impl FnMut() -> bool) -> Result<()> {
    if keep_going() {
        Ok(())
    } else {
        Err(ArchiveError::Cancelled)
    }
}

fn add(total: &mut u64, amount: u64, limit: u64) -> Result<()> {
    *total = total
        .checked_add(amount)
        .filter(|n| *n <= limit)
        .ok_or(ArchiveError::Limit("expanded bytes"))?;
    Ok(())
}

fn options(method: CompressionMethod) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(method)
        .compression_level((method == CompressionMethod::Deflated).then_some(6))
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o600)
        // Force ZIP64 even on small fixtures to exercise its headers in P0.
        .large_file(true)
}

fn asset_hash(name: &str) -> Option<&str> {
    let hash = name.strip_prefix("assets/")?.strip_suffix(".png")?;
    (hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(hash)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod publication_tests;

mod state_io;
pub use state_io::{LoadedProject, ProjectIo, ProjectLoadStats, ProjectMetadata};

#[cfg(test)]
mod state_fixture;
#[cfg(test)]
mod state_tests;
