use super::*;
use std::path::{Path, PathBuf};
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
#[derive(Debug)]
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
    /// The exclusive borrow covers capture, file publication and marker install.
    /// No Session ownership is handed to I/O. Other holders can read captured
    /// snapshots, but cannot edit or replace this Session during the save.
    ///
    /// ```compile_fail
    /// use zipzi_sdk::{AuthoringSession, ProjectIo};
    /// fn edit_during_save(session: &mut AuthoringSession, io: &ProjectIo) {
    ///     session.save(io, || {
    ///         session.undo(session.version()).unwrap();
    ///         true
    ///     });
    /// }
    /// ```
    pub fn save(
        &mut self,
        io: &ProjectIo,
        keep_going: impl FnMut() -> bool,
    ) -> Result<SaveReceipt, Status> {
        let context = self
            .project
            .as_ref()
            .ok_or_else(|| error("NO_PROJECT_PATH", "Use save_as for a new document"))?;
        let target = context.metadata.path.clone();
        let expected = context.metadata.fingerprint.clone();
        self.save_as(io, &target, Some(&expected), keep_going)
    }
    /// Existing destinations require an explicit fingerprint, except this
    /// Session's own destination, whose last published fingerprint is retained.
    pub fn save_as(
        &mut self,
        io: &ProjectIo,
        target: &Path,
        expected: Option<&ProjectFingerprint>,
        keep_going: impl FnMut() -> bool,
    ) -> Result<SaveReceipt, Status> {
        let target = io.normalize_path(target)?;
        let expected = match &self.project {
            Some(context) if context.metadata.path == target => {
                Some(context.metadata.fingerprint.clone())
            }
            _ => expected.cloned(),
        };
        let content = self.current.content_id();
        let saved = SavedMarker { content };
        let published = io
            .save(
                self.current.document(),
                self.current.resources(),
                &target,
                expected.as_ref(),
                keep_going,
            )
            .map_err(Status::from)?;
        // Project's success contract means the file was published. Everything
        // below is infallible metadata installation; no history/resource admission.
        let context = ProjectContext {
            metadata: ProjectMetadata {
                path: published.path.clone(),
                fingerprint: published.fingerprint.clone(),
            },
            generation: self.version.generation,
        };
        self.project = Some(context);
        self.saved = Some(saved);
        Ok(SaveReceipt {
            content,
            path: published.path,
            fingerprint: published.fingerprint,
            published: published.published,
            durable: published.durable,
            warnings: published.warnings,
        })
    }
}
