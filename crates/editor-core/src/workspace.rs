//! Window workspace. Documents, tasks and previews are addressed by runtime identity.
use crate::{
    CloseDisposition, Document, DocumentSession, EditorCommand, EditorPreview, Layer, LoadRequest,
    TreeNode, ViewState, WorkspaceKind,
};
use anyhow::{Result, anyhow, ensure};
use kasane_sdk::SaveReceipt;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;
mod entry;
mod io;
mod lifecycle;
use entry::{DeferredClose, Entry};
pub use io::{LoadCompletion, LoadTask, SaveCompletion, SaveTask};
pub use lifecycle::EditingLifecycle;

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
pub struct Editor {
    workspace: WorkspaceKind,
    pub(crate) viewports: crate::ViewportRegistry,
    pub(crate) viewport_interaction: Option<crate::ViewportInteractionToken>,
    pub(crate) next_viewport_interaction: u64,
    workspace_revision: u64,
    pub(crate) context_generation: u64,
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
    pub(crate) fn active_session(&self) -> Option<&DocumentSession> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|entry| entry.session())
    }
    pub(crate) fn active_session_mut(&mut self) -> Option<&mut DocumentSession> {
        let id = self.active?;
        self.entry_mut(id).ok()?.session_mut()
    }
    pub fn new() -> Result<Self> {
        Ok(Self::from_document(Document::new_blank()?))
    }
    pub fn empty() -> Self {
        Self {
            workspace: WorkspaceKind::default(),
            viewports: Default::default(),
            viewport_interaction: None,
            next_viewport_interaction: 0,
            workspace_revision: 0,
            context_generation: 0,
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
        if let Some(id) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(id, EditingLifecycle::ContextChange)?;
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
        self.pause_animation();
        let mut session = DocumentSession::from_document(document);
        session.set_workspace(self.workspace);
        self.documents.push(Entry::new(id, self.workspace, session));
        self.context_generation += 1;
        self.active = Some(id);
        self.refresh_viewport_bindings();
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
    pub fn workspace(&self) -> WorkspaceKind {
        self.workspace
    }
    pub fn workspace_revision(&self) -> u64 {
        self.workspace_revision
    }
    /// Finish property edits and roll back unfinished canvas gestures before
    /// changing modes. View revisions also reject frames from the old mode.
    pub fn set_workspace(&mut self, workspace: WorkspaceKind) -> Result<()> {
        if self.workspace == workspace {
            return Ok(());
        }
        ensure!(!self.exiting(), "Workspace is closing");
        if let Some(id) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(id, EditingLifecycle::ContextChange)?;
        }
        if let Some(id) = self.active {
            let entry = self.entry_mut(id)?;
            entry.workspace = workspace;
            if let Some(session) = entry.session_mut() {
                session.set_workspace(workspace);
            }
        }
        self.update_workspace(workspace);
        Ok(())
    }
    fn update_workspace(&mut self, workspace: WorkspaceKind) {
        if self.workspace == workspace {
            return;
        }
        self.workspace = workspace;
        self.context_generation += 1;
        self.workspace_revision += 1;
        self.refresh_viewport_bindings();
        self.invalidate_preview();
        self.presentation_floor = self.revision + 1;
    }
    fn restore_active_workspace(&mut self) {
        if let Some(entry) = self.active.and_then(|id| self.entry(id).ok()) {
            self.update_workspace(entry.workspace);
        }
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
            .and_then(|e| e.session())
            .map(DocumentSession::document)
    }
    pub fn layers(&self) -> Vec<Layer> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(Vec::new, Entry::layers)
    }
    /// Changes only with document edits, not selection or viewport movement.
    pub fn tree_revision(&self) -> Option<u64> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|entry| entry.session())
            .map(|session| session.document().parameter_model_revision())
    }

    pub fn mesh_tree(&self) -> Vec<TreeNode> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(Vec::new, Entry::mesh_tree)
    }
    pub fn deformer_tree(&self) -> Vec<TreeNode> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(Vec::new, Entry::deformer_tree)
    }
    pub fn selected_layer_id(&self) -> Option<&str> {
        self.entry(self.active?).ok()?.selected_layer_id()
    }
    pub fn selected_layer(&self) -> Option<Layer> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(Entry::selected_layer)
    }
    pub fn evaluated_frame(&self) -> Option<std::sync::Arc<kasane_core::DrawableFrame>> {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .and_then(DocumentSession::evaluated_frame)
    }
    pub fn view(&self) -> ViewState {
        self.active
            .and_then(|id| self.entry(id).ok())
            .map_or_else(ViewState::default, |entry| entry.view(self.workspace))
    }
    pub fn is_busy(&self) -> bool {
        self.active
            .is_some_and(|id| self.entry(id).is_ok_and(|e| e.saving().is_some()))
    }
    pub fn can_undo(&self) -> bool {
        self.timeline_edit().is_some()
            || self
                .document()
                .is_some_and(|d| d.history_lengths().0 > 0 || d.has_pending_edit())
    }
    pub fn can_redo(&self) -> bool {
        self.timeline_edit().is_none()
            && self
                .document()
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
            .session_mut()
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
            self.end_editing(previous, EditingLifecycle::ContextChange)?;
        }
        self.context_generation += 1;
        self.active = Some(id);
        self.restore_active_workspace();
        self.refresh_viewport_bindings();
        self.displayed_request = None;
        self.requested_scene = None;
        // The UI releases the inactive document's GPU image. Reissue a paused
        // target on return and reject results from its previous activation.
        self.invalidate_animation();
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
    pub fn zoom_at(&mut self, delta: f32, focus: Option<crate::CanvasPoint>) {
        let metrics = self.metrics;
        if let Some(id) = self.active
            && let Ok(entry) = self.entry_mut(id)
            && let Some(session) = entry.session_mut()
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
            && let Some(session) = entry.session_mut()
        {
            session.set_zoom_at(zoom, focus, metrics);
        }
    }
    pub fn reset_view(&mut self) {
        if let Some(id) = self.active {
            if let Ok(entry) = self.entry_mut(id) {
                if let Some(session) = entry.session_mut() {
                    session.reset_view();
                }
            }
        }
    }
    pub fn invalidate_preview(&mut self) {
        self.requested_scene = None;
        self.displayed_request = None;
    }
    pub fn fit_view(&mut self) {
        let metrics = self.metrics;
        if let Some(session) = self.active_session_mut() {
            session.fit_view(metrics);
        }
    }
    pub fn prepare_close(&mut self, id: DocumentId) -> Result<CloseDisposition> {
        self.end_editing(id, EditingLifecycle::Close)?;
        self.close_disposition(id)
    }
    pub fn set_canvas_metrics(&mut self, metrics: crate::CanvasMetrics) {
        if self.metrics == metrics {
            return;
        }
        if let Some(token) = self
            .viewport_interaction
            .filter(|token| token.kind == crate::ViewportInteractionKind::Modeling)
        {
            self.cancel_viewport_interaction(token);
        }
        self.metrics = metrics;
        for entry in &mut self.documents {
            if let Some(session) = entry.session_mut() {
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
                if let Some(s) = entry.session_mut() {
                    s.pan_by(delta);
                }
            }
        }
    }
    pub fn active_tool(&self) -> crate::ToolId {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .map_or(crate::ToolId::Select, DocumentSession::active_tool)
    }
    pub fn deformer_visibility(&self) -> crate::DeformerVisibility {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .map_or(crate::DeformerVisibility::default(), |s| {
                s.tools.deformer_visibility
            })
    }
    pub fn deformer_grid(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .is_some_and(|s| s.tools.deformer_grid)
    }
    pub fn warp_fine(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .is_some_and(|s| s.tools.warp_fine)
    }
    pub fn pick_candidates(
        &self,
        point: crate::CanvasPoint,
        image: Option<crate::FrameVersion>,
    ) -> Vec<crate::PickCandidate> {
        if self.workspace != WorkspaceKind::Modeling {
            return Vec::new();
        }
        if !self.has_presented_frame(image) {
            return Vec::new();
        }
        let Some(s) = self
            .active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
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
            .session_mut()
            .ok_or_else(|| anyhow!("Document is saving"))?
            .input(input, metrics)
    }
    pub fn cancel_gesture(&mut self) {
        if self
            .viewport_interaction
            .is_some_and(|token| token.kind == crate::ViewportInteractionKind::Modeling)
        {
            self.viewport_interaction = None;
        }
        if let Some(id) = self.active {
            if let Ok(e) = self.entry_mut(id) {
                if let Some(s) = e.session_mut() {
                    s.cancel_gesture();
                }
            }
        }
    }
    pub fn gesture_active(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .is_some_and(|s| s.tools.dragging())
    }
    pub fn overlay(&self) -> crate::OverlaySnapshot {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .map_or_else(crate::OverlaySnapshot::default, |s| s.overlay())
    }
    pub fn evaluation_count(&self) -> u64 {
        self.documents
            .iter()
            .filter_map(|e| e.session())
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
            .and_then(|e| e.session())
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
        // Runtime frames are supplied by the animation executor in phase two.
        // Never expose the Modeling pose or editor visibility as an animation frame.
        if self.workspace != WorkspaceKind::Modeling {
            return Ok(None);
        }
        let Some(id) = self.active else {
            return Ok(None);
        };
        if !self.metrics.visible() || self.is_busy() {
            return Ok(None);
        }
        let s = self.entry(id)?.session().unwrap();
        if self.requested_scene == Some((id, s.scene_revision, s.viewport_revision)) {
            return Ok(None);
        }
        self.preview().map(Some)
    }
    pub fn preview(&mut self) -> Result<EditorPreview> {
        ensure!(
            self.workspace == WorkspaceKind::Modeling,
            "Animation preview is not connected yet"
        );
        let id = self.active.ok_or_else(|| anyhow!("No active document"))?;
        let metrics = self.metrics;
        self.revision += 1;
        let revision = self.revision;
        let session = self
            .entry_mut(id)?
            .session_mut()
            .ok_or_else(|| anyhow!("Document is saving"))?;
        session.fit_default_view(metrics);
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
        self.workspace == WorkspaceKind::Modeling
            && self.active.is_some_and(|id| {
                self.displayed_request == Some((id, revision))
                    && self
                        .entry(id)
                        .ok()
                        .and_then(|e| e.session())
                        .is_some_and(|s| {
                            self.requested_scene
                                == Some((id, s.scene_revision, s.viewport_revision))
                        })
            })
    }
    pub fn present_frame(&mut self, frame: crate::CanvasFrame) -> bool {
        if self.workspace != WorkspaceKind::Modeling
            || self.active != Some(frame.version.document)
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
        let Some(s) = entry.session_mut() else {
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
