//! Value-only native project I/O. No live document, transaction or history owner.
use super::*;
use crate::{ResourceStore, ResourceView};
use zipzi_core::state::{AssetContent, DocumentState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMetadata {
    pub path: PathBuf,
    pub fingerprint: ProjectFingerprint,
}

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub document: DocumentState,
    pub resources: ResourceView,
    pub metadata: ProjectMetadata,
    pub diagnostics: Vec<crate::ResourceDiagnostic>,
    pub stats: ProjectLoadStats,
}

#[derive(Debug, Clone)]
pub struct ProjectLoadStats {
    pub counts: IoCounts,
    pub unique_assets: usize,
    pub asset_bytes: u64,
    pub peak_working_bytes: u64,
}
/// Reusable I/O policy. All model and resource inputs/results are owned values.
#[derive(Clone)]
pub struct ProjectIo {
    pub(super) archive: ProjectArchiveStore,
    resources: ResourceStore,
}
impl ProjectIo {
    pub fn new(workspace_parent: impl Into<PathBuf>, resources: ResourceStore) -> Self {
        Self {
            archive: ProjectArchiveStore::new(workspace_parent),
            resources,
        }
    }
    pub fn with_filesystem(mut self, filesystem: Arc<dyn ArchiveFileSystem>) -> Self {
        self.archive = self.archive.with_filesystem(filesystem);
        self
    }
    pub fn with_limits(mut self, limits: ArchiveIoLimits) -> Self {
        self.archive = self.archive.with_limits(limits);
        self
    }
    pub fn with_disk_budget(mut self, budget: ProjectDiskBudget) -> Self {
        self.archive = self.archive.with_disk_budget(budget);
        self
    }
    pub fn normalize_path(&self, path: &Path) -> std::result::Result<PathBuf, Status> {
        crate::filesystem::reject_symlink(path)?;
        crate::filesystem::local_path(path)
    }
    pub fn fingerprint(
        &self,
        path: &Path,
        keep_going: impl FnMut() -> bool,
    ) -> Result<ProjectFingerprint> {
        self.archive.fingerprint(path, keep_going)
    }
    pub fn load(&self, path: &Path, mut keep_going: impl FnMut() -> bool) -> Result<LoadedProject> {
        check(&mut keep_going)?;
        let path = self.normalize_path(path)?;
        self.archive.prepare_workspace()?;
        let loaded = reader::open_content(
            &path,
            &self.archive.workspace_parent,
            self.archive.limits,
            self.archive.disk_budget.clone(),
            self.archive.open_memory.as_deref(),
            &mut keep_going,
        )?;
        let mut decoded = loaded.document;
        let mut resources = ResourceView::default();
        let mut captured = BTreeMap::new();
        for (record, descriptor) in decoded.model.assets.iter_mut().zip(&decoded.images) {
            check(&mut keep_going)?;
            let Some(content) = &descriptor.content else {
                continue;
            };
            // A malformed/missing image remains explicitly unresolved. Other
            // valid resources still load, matching the native recovery contract.
            if loaded
                .diagnostics
                .iter()
                .any(|d| d.asset_id == descriptor.id)
            {
                continue;
            }
            let image = if let Some(image) = captured.get(&content.source) {
                image
            } else {
                let image = self
                    .resources
                    .import_image(&loaded.working_set.path().join(&content.source))?;
                captured.entry(content.source.clone()).or_insert(image)
            };
            if (image.width(), image.height()) != (content.width, content.height) {
                return Err(Status::error("RESOURCE_DIMENSIONS", &descriptor.id).into());
            }
            record.content = AssetContent::Image {
                resource: image.key(),
                width: image.width(),
                height: image.height(),
            };
        }
        resources = resources.with_images(captured.into_values());
        let document = DocumentState::from_owned(decoded.model)?;
        let resources = resources.for_state(&document)?;
        check(&mut keep_going)?;
        Ok(LoadedProject {
            document,
            resources,
            metadata: ProjectMetadata {
                path,
                fingerprint: loaded.fingerprint,
            },
            stats: ProjectLoadStats {
                counts: loaded.counts,
                unique_assets: loaded.unique_assets,
                asset_bytes: loaded.asset_bytes,
                peak_working_bytes: loaded.peak_working_bytes,
            },
            diagnostics: loaded.diagnostics,
        })
    }
    /// File projection leaves all content identities and historical resource keys unchanged.
    pub fn save(
        &self,
        state: &DocumentState,
        resources: &ResourceView,
        target: &Path,
        expected: Option<&ProjectFingerprint>,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<ArchiveSaveReceipt> {
        check(&mut keep_going)?;
        self.archive.prepare_workspace()?;
        let target = self.normalize_path(target)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        check_archive_target(&target, expected, &mut keep_going)?;
        let prepared =
            writer::prepare_state(state, resources, self.archive.limits, &mut keep_going)?;
        self.archive
            .publish_prepared(prepared, &target, expected, keep_going)
    }
}

impl ProjectIo {
    pub fn set_save_preview(&mut self, preview: Option<ProjectPreview>) -> Option<ProjectPreview> {
        self.archive.set_save_preview(preview)
    }
    pub fn set_save_progress(
        &mut self,
        progress: Option<ProjectSaveProgress>,
    ) -> Option<ProjectSaveProgress> {
        self.archive.set_save_progress(progress)
    }
    pub fn set_open_memory(
        &mut self,
        memory: Option<Arc<crate::TransferReservation>>,
    ) -> Option<Arc<crate::TransferReservation>> {
        self.archive.set_open_memory(memory)
    }
}
