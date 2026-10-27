//! Runtime-owned IO; the view keeps frozen presentation state only.
use super::*;
use zipzi_runtime::{Completion, JobId, JobValue, Work};

pub struct LoadTask {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    work: Work,
}
pub struct LoadCompletion {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    returned: Option<Completion>,
    prepared: Option<Result<crate::document::PreparedDocument>>,
}
impl LoadTask {
    pub fn run(self) -> LoadCompletion {
        let Self {
            id,
            operation,
            job,
            work,
        } = self;
        // Real model decoders need an explicit stack, independent of GPUI's pool.
        let returned = work.run_on_thread("zipzi-document-load", 8 * 1024 * 1024);
        let prepared = returned.returned_session().map(|session| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Document::prepare_runtime_view(session)
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
            let session = host
                .read()
                .job_info(host.client_id(), self.job)
                .ok()
                .map(|job| job.session);
            let accepted = host.write().complete(returned);
            if accepted {
                if let (Some(session), Some(prepared)) = (session, self.prepared.take()) {
                    host.prepare_view(session, prepared);
                }
            }
        }
    }
}
pub struct SaveTask {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    work: Work,
}
pub struct SaveCompletion {
    id: DocumentId,
    operation: OperationId,
    job: JobId,
    returned: Option<Completion>,
}
impl SaveTask {
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
    /// Returning SDK ownership and storing the outcome never depend on a view.
    pub fn publish(&mut self, host: &crate::RuntimeHost) {
        if let Some(returned) = self.returned.take() {
            host.write().complete(returned);
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
        let job = match request {
            LoadRequest::Open(path) => {
                let opened = host.write().request_open(host.client_id(), &path)?;
                match opened {
                    zipzi_runtime::OpenRequest::Existing(session) => {
                        if let Some(id) = self
                            .documents
                            .iter()
                            .find(|e| e.hosted_session_id() == session)
                            .map(|e| e.id)
                        {
                            self.activate(id)?;
                        } else {
                            let document = Document::attach_runtime(host.clone(), session)?;
                            self.insert(document)?;
                        }
                        return Ok(None);
                    }
                    zipzi_runtime::OpenRequest::Pending(job) => job,
                }
            }
            LoadRequest::Model3(path) => host
                .write()
                .request_import(host.client_id(), zipzi_runtime::ImportSource::Model3(path))?,
            LoadRequest::Psd(path) => host
                .write()
                .request_import(host.client_id(), zipzi_runtime::ImportSource::Psd(path))?,
            LoadRequest::Cubism(path) => host
                .write()
                .request_import(host.client_id(), zipzi_runtime::ImportSource::Cubism(path))?,
        };
        let work = host.dispatch_job(job)?;
        let id = DocumentId(Uuid::new_v4());
        let operation = OperationId(Uuid::new_v4());
        self.loads.insert(operation, (id, job));
        Ok(Some(LoadTask {
            id,
            operation,
            job,
            work,
        }))
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
            .job_info(self.runtime.client_id(), job)?;
        let result = info
            .result
            .ok_or_else(|| anyhow!("Load is still running"))?;
        self.loads.remove(&operation);
        self.runtime
            .write()
            .release_job(self.runtime.client_id(), job)?;
        let (session, notices, name) = match result? {
            JobValue::Opened(session) => (session, vec![], None),
            JobValue::Imported {
                session,
                notices,
                name,
            } => (session, notices, name),
            _ => return Err(anyhow!("Unexpected load result")),
        };
        let mut document = Document::attach_runtime(self.runtime.clone(), session)?;
        document.set_import_name(name);
        if let Some(previous) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(previous, EditingLifecycle::ContextChange)?;
        }
        self.insert_with_id(id, document)?;
        Ok(notices)
    }
    /// Capture the existing destination after a save dialog confirmed replacement.
    /// Run on a worker: the manifest read may block on filesystem IO.
    pub fn save_replacement_baseline(path: &Path) -> Result<Option<String>> {
        let manifest = zipzi_project::project_manifest(path);
        match std::fs::read(manifest) {
            Ok(bytes) => Ok(Some(zipzi_project::content_sha256(&bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
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
        target_sha256: &str,
    ) -> Result<SaveTask> {
        self.prepare_save_inner(id, path, close_after, Some(target_sha256))
    }

    fn prepare_save_inner(
        &mut self,
        id: DocumentId,
        path: &Path,
        close_after: bool,
        target_sha256: Option<&str>,
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
        let prepared = (|| -> Result<(JobId, Work)> {
            let job = match target_sha256 {
                Some(hash) => {
                    host.write()
                        .request_save_replacing(host.client_id(), session, path, hash)?
                }
                None => host.write().request_save(host.client_id(), session, path)?,
            };
            Ok((job, host.dispatch_job(job)?))
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
            .job_info(self.runtime.client_id(), job)?;
        let result = info
            .result
            .ok_or_else(|| anyhow!("Save is still running"))?;
        let session_id = self.entry(id)?.hosted_session_id();
        let faulted = matches!(
            self.runtime.read().session_info(session_id)?.state,
            zipzi_runtime::SessionState::Faulted(_)
        );
        if faulted {
            self.entry_mut(id)?.mark_faulted();
            self.runtime
                .write()
                .release_job(self.runtime.client_id(), job)?;
            self.cancel_exit();
            return Err(result
                .err()
                .map(anyhow::Error::from)
                .unwrap_or_else(|| anyhow!("Session faulted")));
        }
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
                .job_info(self.runtime.client_id(), job)
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
                .job_info(self.runtime.client_id(), job)
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
        let jobs = self.runtime.read().list_jobs(self.runtime.client_id());
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
    /// Closing a tab detaches its view. Formal session/history remain discoverable.
    pub fn detach_document(&mut self, id: DocumentId) -> Result<()> {
        self.end_editing(id, EditingLifecycle::Close)?;
        let session = self.entry(id)?.hosted_session_id();
        self.runtime
            .write()
            .detach_editor_view(self.runtime.client_id(), session)?;
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
        let sessions = self.runtime.read().list_sessions();
        for info in sessions {
            if !self
                .documents
                .iter()
                .any(|e| e.hosted_session_id() == info.id)
                && info.state == zipzi_runtime::SessionState::Ready
            {
                self.runtime
                    .write()
                    .attach_session(self.runtime.client_id(), info.id)?;
                let document = Document::attach_runtime(self.runtime.clone(), info.id)?;
                self.insert(document)?;
            }
        }
        Ok(notices)
    }
    pub fn sync_runtime(&mut self) -> Result<bool> {
        let mut changed = false;
        let mut fault = None;
        for entry in &mut self.documents {
            let info = self
                .runtime
                .read()
                .session_info(entry.hosted_session_id())?;
            if matches!(info.state, zipzi_runtime::SessionState::Faulted(_)) {
                if entry.session().is_some() {
                    entry.begin_save(OperationId(Uuid::new_v4()), None)?;
                }
                if entry.saving().is_some() {
                    entry.mark_faulted();
                    fault = info.fault.clone();
                    changed = true;
                }
                continue;
            }
            if let zipzi_runtime::SessionState::Busy(job) = info.state {
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
            self.inspection_generation += 1;
            self.refresh_viewport_bindings();
        }
        if let Some(error) = fault {
            self.cancel_exit();
            return Err(error.into());
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
            self.runtime.read().pending_jobs().iter().all(|job| {
                self.documents.iter().any(|entry| {
                    entry
                        .saving()
                        .is_some_and(|s| !s.external && s.job == Some(job.id))
                })
            }),
            "Wait for background Runtime operations to finish"
        );
        ensure!(
            self.runtime.read().list_sessions().iter().all(|info| self
                .documents
                .iter()
                .any(|entry| entry.hosted_session_id() == info.id)),
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
