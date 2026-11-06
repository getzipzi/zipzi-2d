//! Runtime-owned IO; the view keeps frozen presentation state only.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use zipzi_runtime::{Completion, JobId, JobValue, Work};

/// Coarse worker phases, without estimating progress inside a format decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LoadStage {
    Queued,
    Reading,
    PreparingView,
}

/// Read-only progress that can outlive the task while its completion is published.
#[derive(Clone)]
pub struct LoadProgress(Arc<AtomicU8>);

impl LoadProgress {
    pub fn stage(&self) -> LoadStage {
        match self.0.load(Ordering::Relaxed) {
            0 => LoadStage::Queued,
            1 => LoadStage::Reading,
            _ => LoadStage::PreparingView,
        }
    }

    fn set_stage(&self, stage: LoadStage) {
        self.0.store(stage as u8, Ordering::Relaxed);
    }
}

pub struct LoadTask {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    work: Work,
    progress: LoadProgress,
}
pub struct LoadCompletion {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    returned: Option<Completion>,
    prepared: Option<Result<crate::document::PreparedDocument>>,
}
impl LoadTask {
    pub fn progress(&self) -> LoadProgress {
        self.progress.clone()
    }

    pub fn run(self) -> LoadCompletion {
        let Self {
            id,
            operation,
            job,
            work,
            progress,
        } = self;
        progress.set_stage(LoadStage::Reading);
        // Real model decoders need an explicit stack, independent of GPUI's pool.
        let returned = work.run_with_stack("zipzi-document-load", 8 * 1024 * 1024);
        let prepared = returned.prepared_snapshot().map(|snapshot| {
            progress.set_stage(LoadStage::PreparingView);
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Document::prepare_runtime_view(snapshot)
            }))
            .unwrap_or_else(|_| Err(anyhow!("Document view preparation panicked")))
        });
        LoadCompletion {
            id,
            operation,
            job,
            returned: Some(returned),
            prepared,
        }
    }
}
impl LoadCompletion {
    pub fn job_id(&self) -> JobId {
        self.job
    }
    /// Application host calls this before touching any weak view/window handle.
    pub fn publish(&mut self, host: &crate::RuntimeHost) {
        if let Some(returned) = self.returned.take() {
            if let Err(returned) = host.write().complete(returned) {
                self.returned = Some(returned);
                return;
            }
            let published = host
                .read()
                .job(host.client_id(), self.job)
                .ok()
                .and_then(|job| {
                    if job.state != zipzi_runtime::JobState::Succeeded {
                        return None;
                    }
                    match job.source {
                        zipzi_sdk::QuerySource::Published(version) => Some(version.session),
                        zipzi_sdk::QuerySource::Draft(source) => Some(source.base.session),
                    }
                });
            if let (Some(session), Some(prepared)) = (published, self.prepared.take()) {
                host.prepare_view(session, prepared);
            }
        }
    }
}
pub struct SaveTask {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    work: Work,
    progress: zipzi_project::archive::ProjectSaveProgress,
}
pub struct SaveCompletion {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    returned: Option<Completion>,
}
impl SaveTask {
    pub fn progress(&self) -> zipzi_project::archive::ProjectSaveProgress {
        self.progress.clone()
    }
    pub fn run(self) -> SaveCompletion {
        SaveCompletion {
            id: self.id,
            operation: self.operation,
            job: self.job,
            returned: Some(self.work.run()),
        }
    }
}
impl SaveCompletion {
    pub fn job_id(&self) -> JobId {
        self.job
    }
    /// Install save metadata before touching any weak view handle.
    pub fn publish(&mut self, host: &crate::RuntimeHost) {
        if let Some(returned) = self.returned.take() {
            if let Err(returned) = host.write().complete(returned) {
                self.returned = Some(returned);
            }
        }
    }
}

impl Editor {
    pub fn prepare_load(&mut self, request: LoadRequest) -> Result<Option<LoadTask>> {
        ensure!(!self.exiting, "Workspace is closing");
        if let Some(id) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(id, EditingLifecycle::ContextChange)?;
        }
        let host = self.runtime.clone();
        if let LoadRequest::Open(path) = &request {
            let existing = host.read().project_session(host.client_id(), path)?;
            if let Some(session) = existing {
                if let Some(id) = self
                    .documents
                    .iter()
                    .find(|entry| entry.hosted_session_id() == session)
                    .map(|entry| entry.id)
                {
                    self.activate(id)?;
                } else {
                    self.insert(Document::attach_runtime(host.clone(), session)?)?;
                }
                return Ok(None);
            }
        }
        let session = host.write().create_session(
            host.client_id(),
            &Uuid::new_v4().to_string(),
            Default::default(),
        )?;
        let version = host
            .read()
            .session_read(host.client_id(), session)?
            .version();
        let admitted = match request {
            LoadRequest::Open(path) => {
                let io = host.read().project_io().clone();
                host.write()
                    .start_open(host.client_id(), version, &io, &path, Default::default())
            }
            LoadRequest::Model3(path) => host.write().start_import(
                host.client_id(),
                version,
                &path,
                zipzi_contract::kernel_wire::ImportFormat::Model3,
                Default::default(),
            ),
            LoadRequest::Psd(path) => host.write().start_import(
                host.client_id(),
                version,
                &path,
                zipzi_contract::kernel_wire::ImportFormat::Psd,
                Default::default(),
            ),
            LoadRequest::Cubism(path) => host.write().start_import(
                host.client_id(),
                version,
                &path,
                zipzi_contract::kernel_wire::ImportFormat::Cubism,
                Default::default(),
            ),
        };
        let (job, work) = match admitted {
            Ok(value) => value,
            Err(error) => {
                host.write()
                    .close_session(host.client_id(), session, true)?;
                return Err(error.into());
            }
        };
        let id = DocumentId(Uuid::new_v4());
        let operation = OperationId(Uuid::new_v4());
        self.loads.insert(operation, (id, job));
        Ok(Some(LoadTask {
            id,
            operation,
            job,
            work,
            progress: LoadProgress(Arc::new(AtomicU8::new(LoadStage::Queued as u8))),
        }))
    }
    /// Reload through the same SDK transaction; failed decoding preserves the old Session.
    pub fn prepare_reopen(&mut self, id: DocumentId, path: &Path) -> Result<LoadTask> {
        ensure!(!self.exiting, "Workspace is closing");
        self.end_editing(id, EditingLifecycle::ContextChange)?;
        let operation = OperationId(Uuid::new_v4());
        let session = self.entry(id)?.hosted_session_id();
        self.entry_mut(id)?.begin_save(operation, None)?;
        let host = self.runtime.clone();
        let prepared = (|| -> Result<_> {
            let version = host
                .read()
                .session_read(host.client_id(), session)?
                .version();
            let io = host.read().project_io().clone();
            Ok(host
                .write()
                .start_open(host.client_id(), version, &io, path, Default::default())?)
        })();
        let (job, work) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.entry_mut(id)?.restore_session();
                return Err(error);
            }
        };
        self.loads.insert(operation, (id, job));
        self.advance_editing_context();
        self.refresh_viewport_bindings();
        Ok(LoadTask {
            id,
            operation,
            job,
            work,
            progress: LoadProgress(Arc::new(AtomicU8::new(LoadStage::Queued as u8))),
        })
    }
    pub fn finish_load(&mut self, mut completion: LoadCompletion) -> Result<Vec<String>> {
        ensure!(
            self.loads
                .get(&completion.operation)
                .is_some_and(|(id, job)| *id == completion.id && *job == completion.job),
            "Unknown load completion"
        );
        completion.publish(&self.runtime);
        self.finish_load_job(completion.operation, completion.id, completion.job)
    }
    fn finish_load_job(
        &mut self,
        operation: OperationId,
        id: DocumentId,
        job: JobId,
    ) -> Result<Vec<String>> {
        let info = self
            .runtime
            .read()
            .job(self.runtime.client_id(), job)?
            .clone();
        let result = info
            .result
            .ok_or_else(|| anyhow!("Load is still running"))?;
        self.loads.remove(&operation);
        self.runtime
            .write()
            .release_job(self.runtime.client_id(), job)?;
        let reopening = self.documents.iter().any(|entry| entry.id == id);
        if reopening {
            self.entry_mut(id)?.restore_session();
            self.refresh_viewport_bindings();
        }
        let loaded = match result {
            Ok(value) => value,
            Err(error) => {
                if !reopening {
                    let zipzi_sdk::QuerySource::Published(version) = info.source else {
                        unreachable!("load source")
                    };
                    // A failed provisional load has no view. Remove only its
                    // untouched blank; a foreign edit must remain resident.
                    let unchanged = self
                        .runtime
                        .read()
                        .session_read(self.runtime.client_id(), version.session)
                        .is_ok_and(|sdk| sdk.version() == version);
                    if unchanged {
                        let _ = self.runtime.write().close_session(
                            self.runtime.client_id(),
                            version.session,
                            true,
                        );
                    }
                }
                return Err(error.into());
            }
        };
        let (session, mut notices, diagnostics, name) = match loaded {
            JobValue::Opened(receipt) => (
                receipt.publication.version.session,
                vec![],
                receipt.diagnostics,
                None,
            ),
            JobValue::Imported(receipt) => (
                receipt.publication.version.session,
                receipt.warnings,
                receipt.diagnostics,
                receipt.name,
            ),
            _ => return Err(anyhow!("Unexpected load result")),
        };
        notices.extend(
            diagnostics
                .into_iter()
                .map(|d| format!("{}: {}", d.code, d.message)),
        );
        let mut document = Document::attach_runtime(self.runtime.clone(), session)?;
        document.set_import_name(name);
        if let Some(previous) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(previous, EditingLifecycle::ContextChange)?;
        }
        if reopening {
            let mut session = DocumentSession::from_document(document);
            for viewport in self.viewport_ids() {
                session.views.entry(viewport).or_default();
            }
            let workspace = self.entry(id)?.workspace;
            *self.entry_mut(id)? = Entry::new(id, workspace, session);
            self.advance_editing_context();
            self.inspection_generation += 1;
            self.refresh_viewport_bindings();
        } else {
            self.insert_with_id(id, document)?;
        }
        Ok(notices)
    }
    /// Capture the approved destination through the project layer's bounded
    /// whole-archive fingerprint rules. Run on a worker.
    pub fn save_replacement_baseline(
        path: &Path,
    ) -> Result<Option<zipzi_project::archive::ProjectFingerprint>> {
        if !path.try_exists()? {
            return Ok(None);
        }
        Ok(Some(
            zipzi_project::archive::ProjectArchiveStore::new(std::env::temp_dir())
                .fingerprint(path, || true)?,
        ))
    }

    pub fn prepare_save(
        &mut self,
        id: DocumentId,
        path: &Path,
        close_after: bool,
    ) -> Result<SaveTask> {
        self.prepare_save_inner(id, path, close_after, None)
    }

    /// Save after the user explicitly selected an existing destination to replace.
    pub fn prepare_save_replacing(
        &mut self,
        id: DocumentId,
        path: &Path,
        close_after: bool,
        target_fingerprint: &zipzi_project::archive::ProjectFingerprint,
    ) -> Result<SaveTask> {
        self.prepare_save_inner(id, path, close_after, Some(target_fingerprint))
    }

    fn prepare_save_inner(
        &mut self,
        id: DocumentId,
        path: &Path,
        close_after: bool,
        target_fingerprint: Option<&zipzi_project::archive::ProjectFingerprint>,
    ) -> Result<SaveTask> {
        self.end_editing(id, EditingLifecycle::Save)?;
        let operation = OperationId(Uuid::new_v4());
        let close_reason = if self.exiting {
            DeferredClose::Exit
        } else {
            DeferredClose::Document
        };
        let session = self.entry(id)?.hosted_session_id();
        self.entry_mut(id)?
            .begin_save(operation, close_after.then_some(close_reason))?;
        let host = self.runtime.clone();
        let progress = zipzi_project::archive::ProjectSaveProgress::default();
        let prepared = (|| -> Result<(JobId, Work)> {
            let version = host
                .read()
                .session_read(host.client_id(), session)?
                .version();
            let mut io = host.read().project_io().clone();
            io.set_save_progress(Some(progress.clone()));
            Ok(host.write().start_save_as(
                host.client_id(),
                version,
                &io,
                path,
                target_fingerprint,
                Default::default(),
            )?)
        })();
        let (job, work) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.entry_mut(id)?.restore_session();
                return Err(error);
            }
        };
        self.entry_mut(id)?.saving_mut().unwrap().job = Some(job);
        if self.active == Some(id) {
            self.advance_editing_context();
        }
        self.refresh_viewport_bindings();
        Ok(SaveTask {
            id,
            operation,
            job,
            work,
            progress,
        })
    }
    pub fn finish_save(&mut self, completion: &mut SaveCompletion) -> Result<SaveReceipt> {
        ensure!(
            self.entry(completion.id)?
                .saving()
                .is_some_and(
                    |s| s.operation == completion.operation && s.job == Some(completion.job)
                ),
            "Unknown save completion"
        );
        completion.publish(&self.runtime);
        self.finish_save_job(completion.id, completion.job)
    }
    fn finish_save_job(&mut self, id: DocumentId, job: JobId) -> Result<SaveReceipt> {
        let info = self
            .runtime
            .read()
            .job(self.runtime.client_id(), job)?
            .clone();
        let result = info
            .result
            .ok_or_else(|| anyhow!("Save is still running"))?;
        let close = self.entry_mut(id)?.restore_session();
        self.runtime
            .write()
            .release_job(self.runtime.client_id(), job)?;
        if let Some(session) = self.entry_mut(id)?.session_mut() {
            session.document.session.refresh_metadata()?;
            if result.is_ok() {
                session.document.saved_by_runtime();
            }
        }
        if self.active == Some(id) {
            self.advance_editing_context();
        }
        self.refresh_viewport_bindings();
        match result {
            Ok(JobValue::Saved(receipt)) => {
                if close.is_some() {
                    self.close_document(id, false)?;
                }
                Ok(receipt)
            }
            Err(error) => {
                self.cancel_exit();
                Err(error.into())
            }
            _ => Err(anyhow!("Unexpected save result")),
        }
    }
    /// Reconcile IO completed while no window owned this workspace.
    pub fn pending_io_count(&self) -> usize {
        self.loads.len()
            + self
                .documents
                .iter()
                .filter(|entry| entry.saving().is_some_and(|s| !s.external))
                .count()
    }
    pub fn sync_runtime_jobs(&mut self) -> Vec<String> {
        let saves: Vec<_> = self
            .documents
            .iter()
            .filter_map(|entry| {
                entry
                    .saving()
                    .filter(|s| !s.external)
                    .and_then(|s| s.job)
                    .map(|job| (entry.id, job))
            })
            .collect();
        let loads: Vec<_> = self
            .loads
            .iter()
            .map(|(operation, (id, job))| (*operation, *id, *job))
            .collect();
        let mut notices = vec![];
        for (id, job) in saves {
            if self
                .runtime
                .read()
                .job(self.runtime.client_id(), job)
                .is_ok_and(|j| j.state.is_terminal())
            {
                if let Err(error) = self.finish_save_job(id, job) {
                    notices.push(error.to_string());
                }
            }
        }
        for (operation, id, job) in loads {
            if self
                .runtime
                .read()
                .job(self.runtime.client_id(), job)
                .is_ok_and(|j| j.state.is_terminal())
            {
                match self.finish_load_job(operation, id, job) {
                    Ok(warnings) => notices.extend(warnings),
                    Err(error) => notices.push(error.to_string()),
                }
            }
        }
        let pending: std::collections::HashSet<_> = self
            .documents
            .iter()
            .filter_map(|entry| entry.saving().filter(|s| !s.external).and_then(|s| s.job))
            .chain(self.loads.values().map(|(_, job)| *job))
            .collect();
        let jobs = self.runtime.read().client_jobs(self.runtime.client_id());
        if let Ok(jobs) = jobs {
            for job in jobs {
                if job.state.is_terminal() && !pending.contains(&job.id) {
                    if let Some(Err(error)) = job.result {
                        notices.push(error.to_string());
                    }
                    let _ = self
                        .runtime
                        .write()
                        .release_job(self.runtime.client_id(), job.id);
                }
            }
        }
        notices
    }
    /// Detach a document view while keeping its formal session/history discoverable.
    pub fn detach_document(&mut self, id: DocumentId) -> Result<()> {
        self.end_editing(id, EditingLifecycle::Close)?;
        let session = self.entry(id)?.hosted_session_id();
        self.runtime
            .write()
            .detach(self.runtime.client_id(), session)?;
        let index = self.documents.iter().position(|e| e.id == id).unwrap();
        self.documents.remove(index);
        if self.active == Some(id) {
            self.active = self
                .documents
                .get(index.min(self.documents.len().saturating_sub(1)))
                .map(|e| e.id);
            self.inspection_generation += 1;
            self.advance_editing_context();
            self.restore_active_workspace();
            self.refresh_viewport_bindings();
            self.invalidate_animation();
        }
        Ok(())
    }
    pub fn suspend_window(&mut self) -> Result<()> {
        let ids: Vec<_> = self.documents.iter().map(|e| e.id).collect();
        for id in ids {
            self.end_editing(id, EditingLifecycle::Close)?;
            if let Some(session) = self.entry_mut(id)?.session_mut() {
                session.animation.playback.pause();
            }
        }
        self.viewport_interaction = None;
        self.timeline_scrub_owner = None;
        self.viewports = Default::default();
        self.timeline_panels = Default::default();
        for entry in &mut self.documents {
            if let Some(session) = entry.session_mut() {
                session.views.clear();
            }
        }
        Ok(())
    }
    /// Reopen a window from current resident state; never replay an unbounded log.
    pub fn attach_resident_sessions(&mut self) -> Result<Vec<String>> {
        let notices = self.sync_runtime_jobs();
        self.sync_runtime()?;
        let sessions = self.runtime.read().session_keys(self.runtime.client_id())?;
        let loading: std::collections::HashSet<_> = self
            .loads
            .values()
            .filter_map(|(_, job)| {
                self.runtime
                    .read()
                    .job(self.runtime.client_id(), *job)
                    .ok()
                    .map(|job| match job.source {
                        zipzi_sdk::QuerySource::Published(version) => version.session,
                        zipzi_sdk::QuerySource::Draft(source) => source.base.session,
                    })
            })
            .collect();
        for session in sessions {
            if !loading.contains(&session)
                && !self
                    .documents
                    .iter()
                    .any(|entry| entry.hosted_session_id() == session)
            {
                let document = Document::attach_runtime(self.runtime.clone(), session)?;
                self.insert(document)?;
            }
        }
        Ok(notices)
    }
    pub fn sync_runtime(&mut self) -> Result<bool> {
        let mut changed = false;
        for entry in &mut self.documents {
            let saving = self
                .runtime
                .read()
                .saving_job(self.runtime.client_id(), entry.hosted_session_id())?;
            if let Some(job) = saving {
                if entry.session().is_some() {
                    entry.begin_save(OperationId(Uuid::new_v4()), None)?;
                    let saving = entry.saving_mut().unwrap();
                    saving.job = Some(job);
                    saving.external = true;
                    changed = true;
                }
                continue;
            }
            if entry.saving().is_some_and(|s| s.external) {
                entry.restore_session();
                changed = true;
            }
            if let Some(session) = entry.session_mut() {
                if session.document.sync_runtime_events()? {
                    session.invalidate_scene();
                    session.reconcile_animation();
                    changed = true;
                }
            }
        }
        if changed {
            // Runtime publications and save barriers keep the same document/context.
            // The inspector revalidates its pinned object against the refreshed index.
            self.refresh_viewport_bindings();
        }
        Ok(changed)
    }
    pub fn close_disposition(&self, id: DocumentId) -> Result<CloseDisposition> {
        let entry = self.entry(id)?;
        Ok(if entry.saving().is_some() {
            CloseDisposition::Busy
        } else if entry.summary().modified {
            CloseDisposition::NeedsConfirmation
        } else {
            CloseDisposition::Ready
        })
    }
    pub fn close_document(&mut self, id: DocumentId, discard: bool) -> Result<bool> {
        if let Some(s) = self.entry_mut(id)?.session_mut() {
            s.animation.playback.pause();
            s.cancel_gesture();
        }
        let disposition = self.close_disposition(id)?;
        if disposition == CloseDisposition::Busy {
            let reason = if self.exiting {
                DeferredClose::Exit
            } else {
                DeferredClose::Document
            };
            // Exit must not overwrite an independently requested document close.
            self.entry_mut(id)?
                .saving_mut()
                .unwrap()
                .close_after_save
                .get_or_insert(reason);
            return Ok(false);
        }
        ensure!(
            discard || disposition == CloseDisposition::Ready,
            "Document needs a save decision"
        );
        let entry = self.entry(id)?;
        let pristine = entry
            .session()
            .is_some_and(|s| s.document.is_pristine_blank());
        let session = entry.hosted_session_id();
        self.runtime.write().close_session(
            self.runtime.client_id(),
            session,
            discard || pristine,
        )?;
        let index = self.documents.iter().position(|e| e.id == id).unwrap();
        self.documents.remove(index);
        if self.active == Some(id) {
            self.inspection_generation += 1;
            self.advance_editing_context();
            self.active = self
                .documents
                .get(index.min(self.documents.len().saturating_sub(1)))
                .map(|e| e.id);
            self.restore_active_workspace();
            self.refresh_viewport_bindings();
            self.invalidate_animation();
        }
        Ok(true)
    }
    pub fn request_exit(&mut self) -> Result<()> {
        self.attach_resident_sessions()?;
        ensure!(self.loads.is_empty(), "Wait for project loading to finish");
        // Visible local saves already have a completion path that resumes exit.
        // Detached and foreign jobs (including provisional loads) do not.
        ensure!(
            self.runtime.read().outstanding_job_ids().iter().all(|job| {
                self.documents.iter().any(|entry| {
                    entry
                        .saving()
                        .is_some_and(|s| !s.external && s.job == Some(*job))
                })
            }),
            "Wait for background Runtime operations to finish"
        );
        ensure!(
            self.runtime
                .read()
                .session_keys(self.runtime.client_id())?
                .iter()
                .all(|session| self
                    .documents
                    .iter()
                    .any(|entry| entry.hosted_session_id() == *session)),
            "Resolve detached Runtime sessions before quitting"
        );
        if let Some(document) = self.active {
            self.end_editing(document, EditingLifecycle::Close)?;
        }
        self.exiting = true;
        self.refresh_viewport_bindings();
        for entry in &mut self.documents {
            if let Some(session) = entry.session_mut() {
                session.animation.playback.pause();
            }
        }
        Ok(())
    }
    pub fn cancel_exit(&mut self) {
        self.exiting = false;
        self.refresh_viewport_bindings();
        for entry in &mut self.documents {
            if let Some(saving) = entry.saving_mut() {
                if saving.close_after_save == Some(DeferredClose::Exit) {
                    saving.close_after_save = None;
                }
            }
        }
    }
    pub fn exiting(&self) -> bool {
        self.exiting
    }
    pub fn next_close(&self) -> Option<DocumentId> {
        self.documents.first().map(|e| e.id)
    }
}
