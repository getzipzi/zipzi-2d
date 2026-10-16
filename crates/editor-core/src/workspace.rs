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
mod navigation;
mod presentation;
use entry::{DeferredClose, Entry};
pub use io::{LoadCompletion, LoadTask, SaveCompletion, SaveTask};
pub use lifecycle::{EditingContextToken, EditingLifecycle};

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
    pub(crate) timeline_panels: crate::TimelinePanels,
    pub(crate) timeline_scrub_owner: Option<crate::TimelineBinding>,
    pub(crate) viewport_interaction: Option<crate::ViewportInteractionToken>,
    pub(crate) next_viewport_interaction: u64,
    workspace_revision: u64,
    pub(crate) context_generation: u64,
    pub(crate) editing_epoch: u64,
    documents: Vec<Entry>,
    active: Option<DocumentId>,
    loads: HashMap<OperationId, (DocumentId, Option<PathBuf>)>,
    exiting: bool,
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
            timeline_panels: Default::default(),
            timeline_scrub_owner: None,
            viewport_interaction: None,
            next_viewport_interaction: 0,
            workspace_revision: 0,
            context_generation: 0,
            editing_epoch: 0,
            documents: Vec::new(),
            active: None,
            loads: HashMap::new(),
            exiting: false,
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
        for viewport in self.viewport_ids() {
            session.views.entry(viewport).or_default();
        }
        self.documents.push(Entry::new(id, self.workspace, session));
        self.advance_editing_context();
        self.active = Some(id);
        self.refresh_viewport_bindings();
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
    pub fn editing_context(&self) -> Option<crate::EditingContext> {
        Some(self.entry(self.active?).ok()?.context)
    }
    pub fn set_editing_context(&mut self, context: crate::EditingContext) -> Result<()> {
        let id = self.active.ok_or_else(|| anyhow!("No active document"))?;
        if self.entry(id)?.context == context {
            return Ok(());
        }
        ensure!(!self.exiting(), "Workspace is closing");
        self.end_editing(id, EditingLifecycle::ContextChange)?;
        self.entry_mut(id)?.context = context;
        self.invalidate_primary_presentation();
        Ok(())
    }
    pub fn workspace_revision(&self) -> u64 {
        self.workspace_revision
    }
    /// Finish property edits and roll back unfinished canvas gestures before
    /// changing modes. View revisions also reject frames from the old mode.
    pub fn set_workspace(&mut self, workspace: WorkspaceKind) -> Result<()> {
        if self.workspace == workspace {
            return if self.active.is_some() {
                self.set_editing_context(workspace.into())
            } else {
                Ok(())
            };
        }
        ensure!(!self.exiting(), "Workspace is closing");
        if let Some(id) = self.active.filter(|_| !self.is_busy()) {
            self.end_editing(id, EditingLifecycle::ContextChange)?;
        }
        if let Some(id) = self.active {
            let entry = self.entry_mut(id)?;
            entry.workspace = workspace;
            entry.context = workspace.into();
        }
        self.update_workspace(workspace);
        Ok(())
    }
    fn update_workspace(&mut self, workspace: WorkspaceKind) {
        if self.workspace == workspace {
            return;
        }
        self.workspace = workspace;
        self.advance_editing_context();
        self.workspace_revision += 1;
        self.refresh_viewport_bindings();
        self.invalidate_primary_presentation();
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
        self.active_id()
            .zip(self.primary_viewport_id())
            .and_then(|(document, viewport)| self.viewport_state(document, viewport))
            .map_or_else(ViewState::default, |state| state.view)
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
        ensure!(
            self.entry(id)?.context == crate::EditingContext::Modeling
                || matches!(
                    command,
                    EditorCommand::Undo
                        | EditorCommand::Redo
                        | EditorCommand::FinishEdit
                        | EditorCommand::CancelEdit
                ),
            "Modeling commands are unavailable in Animation"
        );
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
        self.advance_editing_context();
        self.active = Some(id);
        self.restore_active_workspace();
        self.refresh_viewport_bindings();
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
    pub fn prepare_close(&mut self, id: DocumentId) -> Result<CloseDisposition> {
        self.end_editing(id, EditingLifecycle::Close)?;
        self.close_disposition(id)
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
                s.modeling.tools.deformer_visibility
            })
    }
    pub fn deformer_grid(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .is_some_and(|s| s.modeling.tools.deformer_grid)
    }
    pub fn warp_fine(&self) -> bool {
        self.active
            .and_then(|id| self.entry(id).ok())
            .and_then(|e| e.session())
            .is_some_and(|s| s.modeling.tools.warp_fine)
    }
    pub fn canvas_input(&mut self, input: crate::CanvasInput) -> Result<bool> {
        if self.editing_context() != Some(crate::EditingContext::Modeling) {
            return Ok(false);
        }
        let binding = self
            .primary_viewport_binding()
            .ok_or_else(|| anyhow!("No available Modeling viewport"))?;
        self.canvas_input_for(binding, input)
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
            .is_some_and(|s| s.modeling.tools.dragging())
    }
    pub fn overlay(&self) -> crate::OverlaySnapshot {
        self.primary_viewport_binding()
            .map_or_else(Default::default, |binding| {
                self.overlay_for_viewport(binding)
            })
    }
    pub fn evaluation_count(&self) -> u64 {
        self.documents
            .iter()
            .filter_map(|e| e.session())
            .map(|s| s.evaluations)
            .sum()
    }
}
