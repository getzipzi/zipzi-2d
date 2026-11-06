use super::*;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};
use zipzi_project::archive::ProjectFingerprint;
use zipzi_project::{ProjectIo, ProjectMetadata, ResourceDiagnostic};

/// Project destination is session metadata, never part of a historical snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    metadata: ProjectMetadata,
    generation: u64,
}
impl ProjectContext {
    pub fn path(&self) -> &Path {
        &self.metadata.path
    }
    pub fn fingerprint(&self) -> &ProjectFingerprint {
        &self.metadata.fingerprint
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedMarker {
    pub content: ContentId,
}

/// A prepared load owns values only. It may cross threads without moving Session.
#[derive(Debug)]
pub struct PreparedProject {
    source: Version,
    snapshot: Snapshot,
    metadata: ProjectMetadata,
    diagnostics: Vec<ResourceDiagnostic>,
}
impl PreparedProject {
    pub fn load(
        source: Version,
        io: &ProjectIo,
        path: &Path,
        keep_going: impl FnMut() -> bool,
    ) -> Result<Self, Status> {
        let loaded = io.load(path, keep_going).map_err(Status::from)?;
        Ok(Self {
            source,
            snapshot: Snapshot::new(loaded.document, loaded.resources)?,
            metadata: loaded.metadata,
            diagnostics: loaded.diagnostics,
        })
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn source(&self) -> Version {
        self.source
    }
}
#[derive(Debug, Clone)]
pub struct OpenReceipt {
    pub publication: Publication,
    pub diagnostics: Vec<ResourceDiagnostic>,
}
#[derive(Debug, Clone)]
pub struct SaveReceipt {
    pub content: ContentId,
    pub path: PathBuf,
    pub fingerprint: ProjectFingerprint,
    pub published: bool,
    pub durable: bool,
    pub warnings: Vec<String>,
}
// A save keeps the Session readable but prevents publication until its completion
// has installed metadata. Failed/unentered work releases exclusivity on drop.
const SAVING: u8 = 0;
const PUBLISHED: u8 = 1;
const FINISHED: u8 = 2;
#[derive(Debug)]
struct SaveLease(Arc<AtomicU8>);
impl Drop for SaveLease {
    fn drop(&mut self) {
        let _ = self
            .0
            .compare_exchange(SAVING, FINISHED, Ordering::AcqRel, Ordering::Acquire);
    }
}

/// Frozen save input with an exclusive publication permit, never Session/history.
/// Deliver every successful write to `AuthoringSession::complete_save`, even if
/// the requester has disconnected or cancelled after external publication.
pub struct PreparedSave {
    source: Version,
    snapshot: Snapshot,
    io: ProjectIo,
    target: PathBuf,
    expected: Option<ProjectFingerprint>,
    lease: SaveLease,
}
impl PreparedSave {
    pub fn source(&self) -> Version {
        self.source
    }
    pub fn path(&self) -> &Path {
        &self.target
    }
    pub fn write(self, keep_going: impl FnMut() -> bool) -> Result<CompletedSave, Status> {
        let published = self
            .io
            .save(
                self.snapshot.document(),
                self.snapshot.resources(),
                &self.target,
                self.expected.as_ref(),
                keep_going,
            )
            .map_err(Status::from)?;
        self.lease.0.store(PUBLISHED, Ordering::Release);
        Ok(CompletedSave {
            source: self.source,
            receipt: SaveReceipt {
                content: self.snapshot.content_id(),
                path: published.path,
                fingerprint: published.fingerprint,
                published: published.published,
                durable: published.durable,
                warnings: published.warnings,
            },
            lease: self.lease,
        })
    }
}

/// A published file is not rolled back by cancellation. Dropping this value
/// without completing it deliberately leaves its Session publication-blocked.
#[derive(Debug)]
#[must_use = "Install published save metadata with AuthoringSession::complete_save"]
pub struct CompletedSave {
    source: Version,
    receipt: SaveReceipt,
    lease: SaveLease,
}
impl CompletedSave {
    pub fn source(&self) -> Version {
        self.source
    }
    pub fn receipt(&self) -> &SaveReceipt {
        &self.receipt
    }
}
impl AuthoringSession {
    pub fn open(
        io: &ProjectIo,
        path: &Path,
        keep_going: impl FnMut() -> bool,
    ) -> Result<(Self, Vec<ResourceDiagnostic>), Status> {
        let loaded = io.load(path, keep_going).map_err(Status::from)?;
        let snapshot = Snapshot::new(loaded.document, loaded.resources)?;
        let mut session = Self::new(snapshot);
        session.saved = Some(SavedMarker {
            content: session.current.content_id(),
        });
        session.project = Some(ProjectContext {
            metadata: loaded.metadata,
            generation: session.version.generation,
        });
        Ok((session, loaded.diagnostics))
    }
    pub fn project(&self) -> Option<&ProjectContext> {
        self.project.as_ref()
    }
    pub fn saved_marker(&self) -> Option<SavedMarker> {
        self.saved
    }
    pub fn is_dirty(&self) -> bool {
        self.saved
            .is_none_or(|marker| marker.content != self.current.content_id())
    }
    /// Installing a prepared load checks the exact request source before changing
    /// generation, history, destination or saved identity.
    pub fn publish_loaded(&mut self, prepared: PreparedProject) -> Result<OpenReceipt, Status> {
        let content = prepared.snapshot.content_id();
        let publication = self.replace(prepared.source, prepared.snapshot)?;
        self.project = Some(ProjectContext {
            metadata: prepared.metadata,
            generation: publication.version.generation,
        });
        self.saved = Some(SavedMarker { content });
        Ok(OpenReceipt {
            publication,
            diagnostics: prepared.diagnostics,
        })
    }
    pub fn load(
        &mut self,
        io: &ProjectIo,
        path: &Path,
        keep_going: impl FnMut() -> bool,
    ) -> Result<OpenReceipt, Status> {
        let prepared = PreparedProject::load(self.version(), io, path, keep_going)?;
        self.publish_loaded(prepared)
    }
    pub(super) fn check_not_saving(&self) -> Result<(), Status> {
        if self
            .saving
            .as_ref()
            .is_some_and(|s| s.load(Ordering::Acquire) != FINISHED)
        {
            Err(error("PROJECT_BUSY", "Save completion is still pending"))
        } else {
            Ok(())
        }
    }
    pub fn prepare_save(&mut self, io: &ProjectIo) -> Result<PreparedSave, Status> {
        self.check_not_saving()?;
        let context = self
            .project
            .as_ref()
            .ok_or_else(|| error("NO_PROJECT_PATH", "Use save_as for a new document"))?;
        let target = context.metadata.path.clone();
        self.prepare_save_as(io, &target, None)
    }
    /// Acquire exclusivity before dispatch. Reads remain available. Publication,
    /// replacement, Undo/Redo and other saves are rejected until work fails or
    /// the host installs the successful completion.
    pub fn prepare_save_as(
        &mut self,
        io: &ProjectIo,
        target: &Path,
        expected: Option<&ProjectFingerprint>,
    ) -> Result<PreparedSave, Status> {
        self.check_not_saving()?;
        let target = io.normalize_path(target)?;
        let expected = match &self.project {
            Some(context) if context.metadata.path == target => {
                Some(context.metadata.fingerprint.clone())
            }
            _ => expected.cloned(),
        };
        let state = Arc::new(AtomicU8::new(SAVING));
        self.saving = Some(state.clone());
        Ok(PreparedSave {
            source: self.version,
            snapshot: self.current.clone(),
            io: io.clone(),
            target,
            expected,
            lease: SaveLease(state),
        })
    }
    /// Wrong-session delivery is rejected without consuming the receipt. For a
    /// matching permit, no validation/admission/cancellation point follows file
    /// publication: content could not change while this permit was outstanding.
    pub fn complete_save(&mut self, completed: &CompletedSave) -> Result<SaveReceipt, Status> {
        if !self
            .saving
            .as_ref()
            .is_some_and(|state| Arc::ptr_eq(state, &completed.lease.0))
            || self.version != completed.source
        {
            return Err(error(
                "STALE_SOURCE",
                "Save permit belongs to another Session",
            ));
        }
        let receipt = completed.receipt.clone();
        self.project = Some(ProjectContext {
            metadata: ProjectMetadata {
                path: receipt.path.clone(),
                fingerprint: receipt.fingerprint.clone(),
            },
            generation: self.version.generation,
        });
        self.saved = Some(SavedMarker {
            content: receipt.content,
        });
        completed.lease.0.store(FINISHED, Ordering::Release);
        self.saving = None;
        Ok(receipt)
    }
    pub fn save(
        &mut self,
        io: &ProjectIo,
        keep_going: impl FnMut() -> bool,
    ) -> Result<SaveReceipt, Status> {
        let completed = self.prepare_save(io)?.write(keep_going)?;
        self.complete_save(&completed)
    }
    pub fn save_as(
        &mut self,
        io: &ProjectIo,
        target: &Path,
        expected: Option<&ProjectFingerprint>,
        keep_going: impl FnMut() -> bool,
    ) -> Result<SaveReceipt, Status> {
        let completed = self
            .prepare_save_as(io, target, expected)?
            .write(keep_going)?;
        self.complete_save(&completed)
    }
}
