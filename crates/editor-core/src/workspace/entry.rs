//! A document is either editable here or owned by an outstanding save task.
use super::*;

pub(super) struct Entry {
    pub(super) id: DocumentId,
    pub(super) workspace: WorkspaceKind,
    state: EntryState,
}
enum EntryState {
    Ready(DocumentSession),
    Saving(SavingState),
}
pub(super) struct SavingState {
    pub(super) operation: OperationId,
    pub(super) path: PathBuf,
    pub(super) close_after_save: Option<DeferredClose>,
    frozen: FrozenDocument,
}
struct FrozenDocument {
    summary: DocumentSummary,
    layers: Vec<Layer>,
    selected: Option<Layer>,
    mesh_tree: Vec<TreeNode>,
    deformer_tree: Vec<TreeNode>,
    views: [ViewState; 2],
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
            state: EntryState::Ready(session),
        }
    }
    pub(super) fn restore_session(&mut self, session: DocumentSession) -> Option<DeferredClose> {
        let close = self.saving().and_then(|saving| saving.close_after_save);
        self.state = EntryState::Ready(session);
        close
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
            EntryState::Saving(saving) => Some(saving),
            EntryState::Ready(_) => None,
        }
    }
    pub(super) fn saving_mut(&mut self) -> Option<&mut SavingState> {
        match &mut self.state {
            EntryState::Saving(saving) => Some(saving),
            EntryState::Ready(_) => None,
        }
    }
    pub(super) fn begin_save(
        &mut self,
        operation: OperationId,
        path: PathBuf,
        close_after_save: Option<DeferredClose>,
    ) -> Result<DocumentSession> {
        let session = self
            .session()
            .ok_or_else(|| anyhow!("Document is saving"))?;
        let frozen = FrozenDocument {
            summary: DocumentSummary::read(self.id, session),
            layers: session.document().layers(),
            selected: session.document().selected_layer(),
            mesh_tree: session.document().mesh_tree(),
            deformer_tree: session.document().deformer_tree(),
            views: WorkspaceKind::ALL.map(|kind| session.view_for(kind)),
        };
        let state = std::mem::replace(
            &mut self.state,
            EntryState::Saving(SavingState {
                operation,
                path,
                close_after_save,
                frozen,
            }),
        );
        let EntryState::Ready(session) = state else {
            unreachable!("session checked before save transition")
        };
        Ok(session)
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
    pub(super) fn mesh_tree(&self) -> Vec<TreeNode> {
        match &self.state {
            EntryState::Ready(session) => session.document().mesh_tree(),
            EntryState::Saving(saving) => saving.frozen.mesh_tree.clone(),
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
    pub(super) fn view(&self, workspace: WorkspaceKind) -> ViewState {
        match &self.state {
            EntryState::Ready(session) => session.view(),
            EntryState::Saving(saving) => saving.frozen.views[workspace.index()],
        }
    }
}
