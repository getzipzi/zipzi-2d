//! A document is either editable here or owned by an outstanding save task.
use super::*;

pub(super) struct Entry {
    pub(super) id: DocumentId,
    pub(super) workspace: WorkspaceKind,
    pub(super) context: crate::EditingContext,
    state: EntryState,
}
enum EntryState {
    Ready(DocumentSession),
    Saving(SavingState),
}
pub(super) struct SavingState {
    pub(super) operation: OperationId,
    pub(super) job: Option<zipzi_runtime::JobId>,
    pub(super) external: bool,
    session: Option<DocumentSession>,
    faulted: bool,
    pub(super) close_after_save: Option<DeferredClose>,
    frozen: FrozenDocument,
}
struct FrozenDocument {
    summary: DocumentSummary,
    layers: Vec<Layer>,
    selected: Option<Layer>,
    part_tree: Vec<TreeNode>,
    deformer_tree: Vec<TreeNode>,
    views: HashMap<crate::ViewportId, crate::ViewportState>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DeferredClose {
    Document,
    Exit,
}
impl Entry {
    pub(super) fn new(id: DocumentId, workspace: WorkspaceKind, session: DocumentSession) -> Self {
        Self {
            id,
            workspace,
            context: workspace.into(),
            state: EntryState::Ready(session),
        }
    }
    pub(super) fn restore_session(&mut self) -> Option<DeferredClose> {
        let saving = match &mut self.state {
            EntryState::Saving(saving) => saving,
            _ => return None,
        };
        let close = saving.close_after_save;
        let mut session = saving.session.take().unwrap();
        for (&id, &state) in &saving.frozen.views {
            session.views.entry(id).or_insert(state);
        }
        for state in session.views.values_mut() {
            state.revision += 1;
        }
        self.state = EntryState::Ready(session);
        close
    }
    pub(super) fn hosted_session_id(&self) -> zipzi_runtime::SessionId {
        match &self.state {
            EntryState::Ready(s) => s.document.session.id,
            EntryState::Saving(s) => s.session.as_ref().unwrap().document.session.id,
        }
    }
    pub(super) fn mark_faulted(&mut self) {
        if let EntryState::Saving(saving) = &mut self.state {
            saving.faulted = true;
            saving.job = None;
        }
    }
    pub(super) fn session(&self) -> Option<&DocumentSession> {
        match &self.state {
            EntryState::Ready(session) => Some(session),
            EntryState::Saving(_) => None,
        }
    }
    pub(super) fn session_mut(&mut self) -> Option<&mut DocumentSession> {
        match &mut self.state {
            EntryState::Ready(session) => Some(session),
            EntryState::Saving(_) => None,
        }
    }
    pub(super) fn saving(&self) -> Option<&SavingState> {
        match &self.state {
            EntryState::Saving(saving) if !saving.faulted => Some(saving),
            EntryState::Saving(_) => None,
            EntryState::Ready(_) => None,
        }
    }
    pub(super) fn saving_mut(&mut self) -> Option<&mut SavingState> {
        match &mut self.state {
            EntryState::Saving(saving) if !saving.faulted => Some(saving),
            EntryState::Saving(_) => None,
            EntryState::Ready(_) => None,
        }
    }
    pub(super) fn begin_save(
        &mut self,
        operation: OperationId,
        close_after_save: Option<DeferredClose>,
    ) -> Result<()> {
        let session = self
            .session()
            .ok_or_else(|| anyhow!("Document is saving"))?;
        let frozen = FrozenDocument {
            summary: DocumentSummary::read(self.id, session),
            layers: session.document().layers(),
            selected: session.document().selected_layer(),
            part_tree: session.document().part_tree(),
            deformer_tree: session.document().deformer_tree(),
            views: session.views.clone(),
        };
        let state = std::mem::replace(
            &mut self.state,
            EntryState::Saving(SavingState {
                operation,
                job: None,
                external: false,
                session: None,
                faulted: false,
                close_after_save,
                frozen,
            }),
        );
        let EntryState::Ready(session) = state else {
            unreachable!("session checked before save transition")
        };
        self.saving_mut().unwrap().session = Some(session);
        Ok(())
    }
    pub(super) fn summary(&self) -> DocumentSummary {
        match &self.state {
            EntryState::Ready(session) => DocumentSummary::read(self.id, session),
            EntryState::Saving(saving) => saving.frozen.summary.clone(),
        }
    }
    pub(super) fn layers(&self) -> Vec<Layer> {
        match &self.state {
            EntryState::Ready(session) => session.document().layers(),
            EntryState::Saving(saving) => saving.frozen.layers.clone(),
        }
    }
    pub(super) fn part_tree(&self) -> Vec<TreeNode> {
        match &self.state {
            EntryState::Ready(session) => session.document().part_tree(),
            EntryState::Saving(saving) => saving.frozen.part_tree.clone(),
        }
    }
    pub(super) fn deformer_tree(&self) -> Vec<TreeNode> {
        match &self.state {
            EntryState::Ready(session) => session.document().deformer_tree(),
            EntryState::Saving(saving) => saving.frozen.deformer_tree.clone(),
        }
    }
    pub(super) fn selected_layer_id(&self) -> Option<&str> {
        match &self.state {
            EntryState::Ready(session) => session.document().selected_id(),
            EntryState::Saving(saving) => saving
                .frozen
                .selected
                .as_ref()
                .map(|layer| layer.id.as_str()),
        }
    }
    pub(super) fn selected_layer(&self) -> Option<Layer> {
        match &self.state {
            EntryState::Ready(session) => session.document().selected_layer(),
            EntryState::Saving(saving) => saving.frozen.selected.clone(),
        }
    }
    pub(super) fn viewport_state(
        &self,
        viewport: crate::ViewportId,
    ) -> Option<crate::ViewportState> {
        match &self.state {
            EntryState::Ready(session) => session.views.get(&viewport).copied(),
            EntryState::Saving(saving) => saving.frozen.views.get(&viewport).copied(),
        }
    }
    pub(super) fn ensure_viewport_state(&mut self, viewport: crate::ViewportId) {
        match &mut self.state {
            EntryState::Ready(session) => {
                session.views.entry(viewport).or_default();
            }
            EntryState::Saving(saving) => {
                saving.frozen.views.entry(viewport).or_default();
            }
        }
    }
}
