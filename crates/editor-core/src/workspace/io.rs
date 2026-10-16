//! Load/save ownership envelopes, path identity and deferred close decisions.
use super::*;

/// Non-cloneable ownership envelopes prevent duplicate completion delivery.
pub struct LoadTask {
    id: DocumentId,
    operation: OperationId,
    request: LoadRequest,
}
pub struct LoadCompletion {
    id: DocumentId,
    operation: OperationId,
    result: Result<(Document, Vec<String>)>,
}
impl LoadTask {
    pub fn run(self) -> LoadCompletion {
        let Self {
            id,
            operation,
            request,
        } = self;
        // GPUI's background executor uses libdispatch workers with small stacks.
        // MOC3 validation and decoding can exceed that stack for real models.
        // Keep the whole load off the executor worker to avoid the observed
        // stack overflow during import.
        let result = std::thread::Builder::new()
            .name("kasane-document-load".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || request.load())
            .map_err(anyhow::Error::from)
            .and_then(|worker| {
                worker
                    .join()
                    .map_err(|_| anyhow!("Document load worker panicked"))?
            });
        LoadCompletion {
            id,
            operation,
            result,
        }
    }
}
pub struct SaveTask {
    id: DocumentId,
    operation: OperationId,
    session: DocumentSession,
    path: PathBuf,
}
pub struct SaveCompletion {
    id: DocumentId,
    operation: OperationId,
    returned: Option<(DocumentSession, Result<SaveReceipt>)>,
}
impl SaveTask {
    /// Both success and SDK errors return the same session, including its history.
    pub fn run(mut self) -> SaveCompletion {
        let result = self.session.save(&self.path);
        SaveCompletion {
            id: self.id,
            operation: self.operation,
            returned: Some((self.session, result)),
        }
    }
}

impl Editor {
    pub fn prepare_load(&mut self, request: LoadRequest) -> Result<Option<LoadTask>> {
        ensure!(!self.exiting, "Workspace is closing");
        let identity = match &request {
            LoadRequest::Open(path) => Some(project_identity(path)?),
            _ => None,
        };
        if matches!(request, LoadRequest::Open(_)) {
            if let Some(identity) = &identity {
                if let Some(id) = self.find_path(identity, None)? {
                    self.activate(id)?;
                    return Ok(None);
                }
            }
        }
        if let Some(identity) = &identity {
            ensure!(
                !self
                    .loads
                    .values()
                    .any(|(_, p)| p.as_ref() == Some(identity)),
                "Project is already loading"
            );
        }
        if let Some(id) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(id, EditingLifecycle::ContextChange)?;
        }
        let id = DocumentId(Uuid::new_v4());
        let operation = OperationId(Uuid::new_v4());
        self.loads.insert(operation, (id, identity));
        Ok(Some(LoadTask {
            id,
            operation,
            request,
        }))
    }
    pub fn finish_load(&mut self, completion: LoadCompletion) -> Result<Vec<String>> {
        ensure!(
            self.loads
                .get(&completion.operation)
                .is_some_and(|(id, _)| *id == completion.id),
            "Unknown load completion"
        );
        self.loads.remove(&completion.operation);
        let (document, notices) = completion.result?;
        if let Some(path) = document.path() {
            if let Some(id) = self.find_path(&project_identity(path)?, None)? {
                self.activate(id)?;
                return Ok(notices);
            }
        }
        // A user may have edited the old tab while loading.
        if let Some(id) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(id, EditingLifecycle::ContextChange)?;
        }
        self.insert_with_id(completion.id, document);
        Ok(notices)
    }
    fn find_path(&self, identity: &Path, except: Option<DocumentId>) -> Result<Option<DocumentId>> {
        for entry in &self.documents {
            if Some(entry.id) == except {
                continue;
            }
            if entry.saving().is_some_and(|saving| saving.path == identity) {
                return Ok(Some(entry.id));
            }
            if let Some(path) = entry.summary().path {
                if project_identity(&path)? == identity {
                    return Ok(Some(entry.id));
                }
            }
        }
        Ok(None)
    }
    fn ensure_available_path(&self, id: Option<DocumentId>, path: &Path) -> Result<PathBuf> {
        let identity = project_identity(path)?;
        ensure!(
            self.find_path(&identity, id)?.is_none(),
            "Project belongs to another open document"
        );
        ensure!(
            !self
                .loads
                .values()
                .any(|(_, p)| p.as_ref() == Some(&identity)),
            "Project is loading"
        );
        Ok(identity)
    }
    pub fn prepare_save(
        &mut self,
        id: DocumentId,
        path: &Path,
        close_after: bool,
    ) -> Result<SaveTask> {
        let identity = self.ensure_available_path(Some(id), path)?;
        self.end_editing(id, EditingLifecycle::Save)?;
        let operation = OperationId(Uuid::new_v4());
        let close_reason = if self.exiting {
            DeferredClose::Exit
        } else {
            DeferredClose::Document
        };
        let entry = self.entry_mut(id)?;
        let session = entry.begin_save(operation, identity, close_after.then_some(close_reason))?;
        if self.active == Some(id) {
            self.context_generation += 1;
        }
        self.refresh_viewport_bindings();
        Ok(SaveTask {
            id,
            operation,
            session,
            path: path.to_owned(),
        })
    }
    /// Rejected delivery leaves the envelope intact so its owner can recover it.
    pub fn finish_save(&mut self, completion: &mut SaveCompletion) -> Result<SaveReceipt> {
        let entry = self.entry_mut(completion.id)?;
        ensure!(
            entry
                .saving()
                .is_some_and(|saving| saving.operation == completion.operation),
            "Unknown save completion"
        );
        let (mut session, result) = completion
            .returned
            .take()
            .ok_or_else(|| anyhow!("Save completion was already delivered"))?;
        // Metrics/device state may change on the UI while the session is saving.
        // Also replace any pending preview rejected while it was on the worker.
        session.viewport_revision += 1;
        session.set_workspace(entry.workspace);
        let close = entry.restore_session(session);
        if self.active == Some(completion.id) {
            self.context_generation += 1;
        }
        self.refresh_viewport_bindings();
        match result {
            Ok(receipt) => {
                if close.is_some() {
                    self.close_document(completion.id, false)?;
                }
                Ok(receipt)
            }
            Err(error) => {
                self.cancel_exit();
                Err(error)
            }
        }
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
            s.animation.pause();
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
        let index = self.documents.iter().position(|e| e.id == id).unwrap();
        self.documents.remove(index);
        if self.active == Some(id) {
            self.context_generation += 1;
            self.active = self
                .documents
                .get(index.min(self.documents.len().saturating_sub(1)))
                .map(|e| e.id);
            self.restore_active_workspace();
            self.refresh_viewport_bindings();
            self.invalidate_animation();
            self.displayed_request = None;
        }
        Ok(true)
    }
    pub fn request_exit(&mut self) -> Result<()> {
        ensure!(self.loads.is_empty(), "Wait for project loading to finish");
        if let Some(document) = self.active {
            self.end_editing(document, EditingLifecycle::Close)?;
        }
        self.exiting = true;
        self.refresh_viewport_bindings();
        for entry in &mut self.documents {
            if let Some(session) = entry.session_mut() {
                session.animation.pause();
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

/// SDK directory/manifest aliases share an identity. Resolve existing ancestors so
/// new Save As paths and symlinked parent directories are normalized as well.
fn project_identity(path: &Path) -> Result<PathBuf> {
    let manifest = kasane_project::project_manifest(path);
    let absolute = if manifest.is_absolute() {
        manifest
    } else {
        std::env::current_dir()?.join(manifest)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                resolved.pop();
            }
            other => {
                resolved.push(other.as_os_str());
                match std::fs::symlink_metadata(&resolved) {
                    Ok(_) => resolved = resolved.canonicalize()?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok(resolved)
}
