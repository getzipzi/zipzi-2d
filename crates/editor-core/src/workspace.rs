//! Window workspace. Documents, tasks and previews are addressed by runtime identity.
use crate::{
    CloseDisposition, Document, DocumentSession, EditorCommand, EditorPreview, Layer, LoadRequest,
    TreeNode, ViewState,
};
use anyhow::{Result, anyhow, ensure};
use kasane_sdk::SaveReceipt;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(Uuid);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OperationId(Uuid);

#[derive(Clone)]
pub struct DocumentSummary {
    pub id: DocumentId,
    pub path: Option<PathBuf>,
    pub title: Option<String>,
    pub modified: bool,
}
impl DocumentSummary {
    fn read(id: DocumentId, session: &DocumentSession) -> Self {
        let document = session.document();
        Self {
            id,
            path: document.path().map(Path::to_path_buf),
            title: document
                .path()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .map(|s| s.to_string_lossy().into_owned())
                .or_else(|| document.import_name().map(str::to_owned)),
            modified: document.modified(),
        }
    }
}
struct Entry {
    id: DocumentId,
    session: Option<DocumentSession>,
    frozen: DocumentSummary,
    frozen_layers: Vec<Layer>,
    frozen_selected: Option<Layer>,
    frozen_mesh_tree: Vec<TreeNode>,
    frozen_deformer_tree: Vec<TreeNode>,
    view: ViewState,
    saving: Option<(OperationId, PathBuf)>,
    close_after_save: Option<DeferredClose>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum DeferredClose {
    Document,
    Exit,
}
impl Entry {
    fn summary(&self) -> DocumentSummary {
        self.session.as_ref().map_or_else(
            || self.frozen.clone(),
            |s| DocumentSummary::read(self.id, s),
        )
    }
}

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

pub struct Editor {
    documents: Vec<Entry>,
    active: Option<DocumentId>,
    loads: HashMap<OperationId, (DocumentId, Option<PathBuf>)>,
    revision: u64,
    displayed_request: Option<(DocumentId, u64)>,
    presentation_floor: u64,
    exiting: bool,
    metrics: crate::CanvasMetrics,
    requested_scene: Option<(DocumentId, u64, u64)>,
}
impl Editor {
    pub fn new() -> Result<Self> {
        Ok(Self::from_document(Document::new_blank()?))
    }
    pub fn empty() -> Self {
        Self {
            documents: Vec::new(),
            active: None,
            loads: HashMap::new(),
            revision: 0,
            displayed_request: None,
            presentation_floor: 0,
            exiting: false,
            metrics: crate::CanvasMetrics::default(),
            requested_scene: None,
        }
    }
    pub fn new_document(&mut self) -> Result<DocumentId> {
        ensure!(!self.exiting, "Workspace is closing");
        if self.active.is_some() && !self.is_busy() {
            self.execute(EditorCommand::FinishEdit)?;
        }
        Ok(self.insert(Document::new_blank()?))
    }
    pub fn from_document(document: Document) -> Self {
        let mut editor = Self::empty();
        editor.insert(document);
        editor
    }
    fn insert(&mut self, document: Document) -> DocumentId {
        self.insert_with_id(DocumentId(Uuid::new_v4()), document)
    }
    fn insert_with_id(&mut self, id: DocumentId, document: Document) -> DocumentId {
        let session = DocumentSession::from_document(document);
        self.documents.push(Entry {
            id,
            frozen: DocumentSummary::read(id, &session),
            frozen_layers: Vec::new(),
            frozen_selected: None,
            frozen_mesh_tree: Vec::new(),
            frozen_deformer_tree: Vec::new(),
            view: session.view(),
            session: Some(session),
            saving: None,
            close_after_save: None,
        });
        self.active = Some(id);
        self.displayed_request = None;
        self.requested_scene = None;
        id
    }
    fn entry(&self, id: DocumentId) -> Result<&Entry> {
        self.documents
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("Document is no longer open"))
    }
    fn entry_mut(&mut self, id: DocumentId) -> Result<&mut Entry> {
        self.documents
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("Document is no longer open"))
    }
    pub fn active_id(&self) -> Option<DocumentId> {
        self.active
    }
    pub fn documents(&self) -> Vec<DocumentSummary> {
        self.documents.iter().map(Entry::summary).collect()
    }
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }
    pub fn summary(&self) -> Option<DocumentSummary> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map(Entry::summary)
    }
    pub fn document(&self) -> Option<&Document> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .map(DocumentSession::document)
    }
    pub fn layers(&self) -> Vec<Layer> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(Vec::new, |entry| {
                entry
                    .session
                    .as_ref()
                    .map_or_else(|| entry.frozen_layers.clone(), |s| s.document().layers())
            })
    }
    /// Changes only with document edits, not selection or viewport movement.
    pub fn tree_revision(&self) -> Option<u64> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|entry| entry.session.as_ref())
            .map(|session| session.document().parameter_model_revision())
    }

    pub fn mesh_tree(&self) -> Vec<TreeNode> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(Vec::new, |entry| {
                entry.session.as_ref().map_or_else(
                    || entry.frozen_mesh_tree.clone(),
                    |session| session.document().mesh_tree(),
                )
            })
    }
    pub fn deformer_tree(&self) -> Vec<TreeNode> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(Vec::new, |entry| {
                entry.session.as_ref().map_or_else(
                    || entry.frozen_deformer_tree.clone(),
                    |session| session.document().deformer_tree(),
                )
            })
    }
    pub fn selected_layer_id(&self) -> Option<&str> {
        let entry = self.entry(self.active?).ok()?;
        match &entry.session {
            Some(session) => session.document().selected_id(),
            None => entry
                .frozen_selected
                .as_ref()
                .map(|layer| layer.id.as_str()),
        }
    }

    pub fn selected_layer(&self) -> Option<Layer> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|entry| {
                entry.session.as_ref().map_or_else(
                    || entry.frozen_selected.clone(),
                    |s| s.document().selected_layer(),
                )
            })
    }
    pub fn evaluated_frame(&self) -> Option<std::sync::Arc<kasane_core::DrawableFrame>> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .and_then(DocumentSession::evaluated_frame)
    }
    pub fn view(&self) -> ViewState {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(ViewState::default, |e| {
                e.session.as_ref().map_or(e.view, DocumentSession::view)
            })
    }
    pub fn is_busy(&self) -> bool {
        self.active
            .is_some_and(|id| self.entry(id).is_ok_and(|e| e.saving.is_some()))
    }
    pub fn can_undo(&self) -> bool {
        self.document()
            .is_some_and(|d| d.history_lengths().0 > 0 || d.has_pending_edit())
    }
    pub fn can_redo(&self) -> bool {
        self.document()
            .is_some_and(|d| d.history_lengths().1 > 0 && !d.has_pending_edit())
    }
    pub fn execute(&mut self, command: EditorCommand) -> Result<()> {
        self.execute_for(
            self.active.ok_or_else(|| anyhow!("No active document"))?,
            command,
        )
    }
    pub fn execute_for(&mut self, id: DocumentId, command: EditorCommand) -> Result<()> {
        if matches!(
            &command,
            EditorCommand::CreateParameter(_)
                | EditorCommand::GenerateMeshGrid { .. }
                | EditorCommand::ApplyMeshGeneration(_)
                | EditorCommand::CreateParameterChecked { .. }
                | EditorCommand::UpdateParameter { .. }
                | EditorCommand::DeleteParameter { .. }
                | EditorCommand::CreateParameterGroup { .. }
                | EditorCommand::UpdateParameterGroup { .. }
                | EditorCommand::DeleteParameterGroup { .. }
                | EditorCommand::SetObjectsLocked { .. }
                | EditorCommand::SelectCanvasObjects(_)
                | EditorCommand::ReorderTree { .. }
                | EditorCommand::MoveOrganization { .. }
                | EditorCommand::MoveParameter { .. }
                | EditorCommand::ReorderParameterGroup { .. }
                | EditorCommand::MoveParameterGroupToRoot(_)
        ) {
            ensure!(
                self.active == Some(id) && !self.exiting(),
                "Document has changed"
            );
        }
        let result = self
            .entry_mut(id)?
            .session
            .as_mut()
            .ok_or_else(|| anyhow!("Document is saving"))?
            .execute(command);
        result
    }

    pub fn activate(&mut self, id: DocumentId) -> Result<()> {
        self.entry(id)?;
        if self.active == Some(id) {
            return Ok(());
        }
        if let Some(previous) = self.active {
            if self.entry(previous)?.session.is_some() {
                self.execute_for(previous, EditorCommand::FinishEdit)?;
            }
        }
        self.active = Some(id);
        self.displayed_request = None;
        self.requested_scene = None;
        Ok(())
    }
    pub fn adjacent(&self, forward: bool) -> Option<DocumentId> {
        let index = self
            .documents
            .iter()
            .position(|e| Some(e.id) == self.active)?;
        let count = self.documents.len();
        Some(self.documents[(index + if forward { 1 } else { count - 1 }) % count].id)
    }
    /// A duplicate open activates its existing tab without rereading disk.
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
        if self.active.is_some() && !self.is_busy() {
            self.execute(EditorCommand::FinishEdit)?;
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
        if self.active.is_some() && !self.is_busy() {
            self.execute(EditorCommand::FinishEdit)?;
        }
        self.insert_with_id(completion.id, document);
        Ok(notices)
    }
    fn find_path(&self, identity: &Path, except: Option<DocumentId>) -> Result<Option<DocumentId>> {
        for entry in &self.documents {
            if Some(entry.id) == except {
                continue;
            }
            if entry.saving.as_ref().is_some_and(|(_, p)| p == identity) {
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
        self.execute_for(id, EditorCommand::FinishEdit)?;
        let operation = OperationId(Uuid::new_v4());
        let close_reason = if self.exiting {
            DeferredClose::Exit
        } else {
            DeferredClose::Document
        };
        let entry = self.entry_mut(id)?;
        entry.frozen = entry.summary();
        entry.view = entry.session.as_ref().unwrap().view();
        entry.frozen_layers = entry.session.as_ref().unwrap().document().layers();
        entry.frozen_selected = entry.session.as_ref().unwrap().document().selected_layer();
        entry.frozen_mesh_tree = entry.session.as_ref().unwrap().document().mesh_tree();
        entry.frozen_deformer_tree = entry.session.as_ref().unwrap().document().deformer_tree();
        let session = entry.session.take().unwrap();
        entry.saving = Some((operation, identity));
        entry.close_after_save = close_after.then_some(close_reason);
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
                .saving
                .as_ref()
                .is_some_and(|(id, _)| *id == completion.operation),
            "Unknown save completion"
        );
        let (mut session, result) = completion
            .returned
            .take()
            .ok_or_else(|| anyhow!("Save completion was already delivered"))?;
        // Metrics/device state may change on the UI while the session is saving.
        // Also replace any pending preview rejected while it was on the worker.
        session.viewport_revision += 1;
        entry.session = Some(session);
        entry.frozen_layers.clear();
        entry.frozen_selected = None;
        entry.frozen_mesh_tree.clear();
        entry.frozen_deformer_tree.clear();
        entry.saving = None;
        let close = std::mem::take(&mut entry.close_after_save);
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
        Ok(if entry.saving.is_some() {
            CloseDisposition::Busy
        } else if entry.summary().modified {
            CloseDisposition::NeedsConfirmation
        } else {
            CloseDisposition::Ready
        })
    }
    pub fn close_document(&mut self, id: DocumentId, discard: bool) -> Result<bool> {
        if let Some(s) = &mut self.entry_mut(id)?.session {
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
            self.entry_mut(id)?.close_after_save.get_or_insert(reason);
            return Ok(false);
        }
        ensure!(
            discard || disposition == CloseDisposition::Ready,
            "Document needs a save decision"
        );
        let index = self.documents.iter().position(|e| e.id == id).unwrap();
        self.documents.remove(index);
        if self.active == Some(id) {
            self.active = self
                .documents
                .get(index.min(self.documents.len().saturating_sub(1)))
                .map(|e| e.id);
            self.displayed_request = None;
        }
        Ok(true)
    }
    pub fn request_exit(&mut self) -> Result<()> {
        ensure!(self.loads.is_empty(), "Wait for project loading to finish");
        self.exiting = true;
        Ok(())
    }
    pub fn cancel_exit(&mut self) {
        self.exiting = false;
        for entry in &mut self.documents {
            if entry.close_after_save == Some(DeferredClose::Exit) {
                entry.close_after_save = None;
            }
        }
    }
    pub fn exiting(&self) -> bool {
        self.exiting
    }
    pub fn next_close(&self) -> Option<DocumentId> {
        self.documents.first().map(|e| e.id)
    }
    pub fn zoom_at(&mut self, delta: f32, focus: Option<crate::CanvasPoint>) {
        let metrics = self.metrics;
        if let Some(id) = self.active
            && let Ok(entry) = self.entry_mut(id)
            && let Some(session) = &mut entry.session
        {
            session.zoom_at(delta, focus, metrics);
        }
    }
    pub fn zoom_by(&mut self, delta: f32) {
        self.zoom_at(delta, None);
    }
    /// Set an absolute zoom while keeping the model point beneath `focus` fixed.
    pub fn set_zoom_at(&mut self, zoom: f32, focus: Option<crate::CanvasPoint>) {
        let metrics = self.metrics;
        if let Some(id) = self.active
            && let Ok(entry) = self.entry_mut(id)
            && let Some(session) = &mut entry.session
        {
            session.set_zoom_at(zoom, focus, metrics);
        }
    }
    pub fn reset_view(&mut self) {
        if let Some(id) = self.active {
            if let Ok(entry) = self.entry_mut(id) {
                if let Some(session) = &mut entry.session {
                    session.reset_view();
                }
            }
        }
    }
    pub fn invalidate_preview(&mut self) {
        self.requested_scene = None;
        self.displayed_request = None;
    }
    pub fn prepare_close(&mut self, id: DocumentId) -> Result<CloseDisposition> {
        if let Some(s) = &mut self.entry_mut(id)?.session {
            s.execute(EditorCommand::FinishEdit)?;
        }
        self.close_disposition(id)
    }
    pub fn set_canvas_metrics(&mut self, metrics: crate::CanvasMetrics) {
        if self.metrics == metrics {
            return;
        }
        self.metrics = metrics;
        for entry in &mut self.documents {
            if let Some(session) = &mut entry.session {
                session.cancel_gesture();
                session.viewport_revision += 1;
            }
        }
        self.displayed_request = None;
        self.requested_scene = None;
    }
    pub fn canvas_metrics(&self) -> crate::CanvasMetrics {
        self.metrics
    }
    pub fn pan_by(&mut self, delta: crate::CanvasPoint) {
        if let Some(id) = self.active {
            if let Ok(entry) = self.entry_mut(id) {
                if let Some(s) = &mut entry.session {
                    s.pan_by(delta);
                }
            }
        }
    }
    pub fn active_tool(&self) -> crate::ToolId {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .map_or(crate::ToolId::Select, |s| s.tools.active)
    }
    pub fn deformer_visibility(&self) -> crate::DeformerVisibility {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .map_or(crate::DeformerVisibility::default(), |s| {
                s.tools.deformer_visibility
            })
    }
    pub fn deformer_grid(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .is_some_and(|s| s.tools.deformer_grid)
    }
    pub fn warp_fine(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .is_some_and(|s| s.tools.warp_fine)
    }
    pub fn pick_candidates(
        &self,
        point: crate::CanvasPoint,
        image: Option<crate::FrameVersion>,
    ) -> Vec<crate::PickCandidate> {
        if !self.has_presented_frame(image) {
            return Vec::new();
        }
        let Some(s) = self
            .active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
        else {
            return Vec::new();
        };
        let Some(frame) = &s.displayed else {
            return Vec::new();
        };
        frame.geometry.candidates_with_locks(
            point,
            frame.transform,
            &s.document().context().selections,
            s.tools.active,
            s.tools.deformer_visibility,
            &s.document().object_state_index(),
        )
    }
    pub fn canvas_input(&mut self, input: crate::CanvasInput) -> Result<bool> {
        let metrics = self.metrics;
        let id = self.active.ok_or_else(|| anyhow!("No active document"))?;
        self.entry_mut(id)?
            .session
            .as_mut()
            .ok_or_else(|| anyhow!("Document is saving"))?
            .input(input, metrics)
    }
    pub fn cancel_gesture(&mut self) {
        if let Some(id) = self.active {
            if let Ok(e) = self.entry_mut(id) {
                if let Some(s) = &mut e.session {
                    s.cancel_gesture();
                }
            }
        }
    }
    pub fn gesture_active(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .is_some_and(|s| s.tools.dragging())
    }
    pub fn overlay(&self) -> crate::OverlaySnapshot {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .map_or_else(crate::OverlaySnapshot::default, |s| s.overlay())
    }
    pub fn evaluation_count(&self) -> u64 {
        self.documents
            .iter()
            .filter_map(|e| e.session.as_ref())
            .map(|s| s.evaluations)
            .sum()
    }
    /// Expose geometry only when it is paired with the image actually owned by the UI.
    pub fn has_presented_frame(&self, frame: Option<crate::FrameVersion>) -> bool {
        let Some(frame) = frame else {
            return false;
        };
        self.active
            .filter(|id| *id == frame.document)
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session.as_ref())
            .is_some_and(|s| {
                s.displayed
                    .as_ref()
                    .is_some_and(|shown| shown.version == frame)
            })
    }
    pub fn overlay_for_frame(&self, frame: Option<crate::FrameVersion>) -> crate::OverlaySnapshot {
        if self.has_presented_frame(frame) {
            self.overlay()
        } else {
            crate::OverlaySnapshot::default()
        }
    }
    pub fn preview_if_needed(&mut self) -> Result<Option<EditorPreview>> {
        let Some(id) = self.active else {
            return Ok(None);
        };
        if !self.metrics.visible() || self.is_busy() {
            return Ok(None);
        }
        let s = self.entry(id)?.session.as_ref().unwrap();
        if self.requested_scene == Some((id, s.scene_revision, s.viewport_revision)) {
            return Ok(None);
        }
        self.preview().map(Some)
    }
    pub fn preview(&mut self) -> Result<EditorPreview> {
        let id = self.active.ok_or_else(|| anyhow!("No active document"))?;
        let metrics = self.metrics;
        self.revision += 1;
        let revision = self.revision;
        let session = self
            .entry_mut(id)?
            .session
            .as_mut()
            .ok_or_else(|| anyhow!("Document is saving"))?;
        let (scene, geometry) = session.scene()?;
        let view = session.view();
        let versions = (id, session.scene_revision, session.viewport_revision);
        let canvas = crate::ViewportTransform::new(scene.viewport_frame.canvas, metrics, view).map(
            |transform| crate::CanvasFrame {
                version: crate::FrameVersion {
                    document: id,
                    scene: versions.1,
                    viewport: versions.2,
                    request: revision,
                },
                transform,
                geometry,
            },
        );
        self.requested_scene = Some(versions);
        if self.displayed_request.is_none() {
            self.presentation_floor = revision;
        }
        self.displayed_request = Some((id, revision));
        Ok(EditorPreview {
            revision,
            scene,
            view,
            canvas,
        })
    }
    pub fn accepts_preview(&self, revision: u64) -> bool {
        self.active.is_some_and(|id| {
            self.displayed_request == Some((id, revision))
                && self
                    .entry(id)
                    .ok()
                    .and_then(|e| e.session.as_ref())
                    .is_some_and(|s| {
                        self.requested_scene == Some((id, s.scene_revision, s.viewport_revision))
                    })
        })
    }
    pub fn present_frame(&mut self, frame: crate::CanvasFrame) -> bool {
        if self.active != Some(frame.version.document)
            || frame.version.request < self.presentation_floor
            || !self.displayed_request.is_some_and(|(id, latest)| {
                id == frame.version.document && frame.version.request <= latest
            })
            || frame.transform.metrics != self.metrics
        {
            return false;
        }
        let Ok(entry) = self.entry_mut(frame.version.document) else {
            return false;
        };
        let Some(s) = &mut entry.session else {
            return false;
        };
        // Completed views and control-edit poses may lag the pointer. Their
        // images retain matching geometry; advance monotonically within the
        // same drag, but reject poses from before it or after it is cancelled.
        if frame.version.scene > s.scene_revision
            || (s.scene_revision != frame.version.scene
                && !s.tools.accepts_drag_frame(frame.version))
            || s.viewport_revision < frame.version.viewport
            || s.displayed
                .as_ref()
                .is_some_and(|shown| shown.version.request >= frame.version.request)
        {
            return false;
        }
        s.displayed = Some(frame);
        true
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
