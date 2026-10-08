//! Parameter-model guards stay here, outside the reusable UI dialog component.
use super::*;
use crate::ui::form_dialog::FormDialogView;

#[derive(Clone, PartialEq, Eq)]
struct SourceState {
    document: Option<DocumentId>,
    revision: Option<u64>,
    available: bool,
}

pub(super) struct ParameterDialogSession {
    pub owner: WeakEntity<EditorView>,
    pub document: DocumentId,
    pub version: u64,
    observed: Option<SourceState>,
}

impl ParameterDialogSession {
    pub fn new(owner: WeakEntity<EditorView>, document: DocumentId, cx: &App) -> Self {
        let version = owner
            .upgrade()
            .unwrap()
            .read(cx)
            .editor
            .document()
            .unwrap()
            .parameter_model_revision();
        let mut session = Self {
            owner,
            document,
            version,
            observed: None,
        };
        session.source_changed(cx);
        session
    }

    pub fn source_changed(&mut self, cx: &App) -> bool {
        let current = self.owner.upgrade().map(|owner| {
            let state = owner.read(cx);
            SourceState {
                document: state.editor.active_id(),
                revision: state
                    .editor
                    .document()
                    .map(|d| d.parameter_model_revision()),
                available: !state.modal && !state.editor.is_busy() && !state.editor.exiting(),
            }
        });
        if self.observed == current {
            return false;
        }
        self.observed = current;
        true
    }

    pub fn validate(&self, cx: &App) -> anyhow::Result<()> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("parameter.document_changed")))?;
        let state = owner.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(self.document),
            t!("parameter.document_changed")
        );
        anyhow::ensure!(
            !state.modal && !state.editor.is_busy() && !state.editor.exiting(),
            t!("parameter.unavailable")
        );
        state
            .editor
            .document()
            .unwrap()
            .check_parameter_version(self.version)
    }

    pub fn needs_reload(&self, cx: &App) -> bool {
        self.owner.upgrade().is_some_and(|owner| {
            let state = owner.read(cx);
            state.editor.active_id() == Some(self.document)
                && state
                    .editor
                    .document()
                    .is_some_and(|d| d.parameter_model_revision() != self.version)
        })
    }

    pub fn diagnostics(
        &self,
        command: &EditorCommand,
        cx: &App,
    ) -> Vec<editor_core::ParameterExportDiagnostic> {
        self.owner
            .upgrade()
            .and_then(|owner| {
                owner
                    .read(cx)
                    .editor
                    .document()
                    .map(|d| d.parameter_export_diagnostics(command))
            })
            .unwrap_or_default()
    }

    pub fn execute(&self, command: EditorCommand, cx: &mut App) -> anyhow::Result<()> {
        self.validate(cx)?;
        self.owner.upgrade().unwrap().update(cx, |state, cx| {
            state.editor.execute_for(self.document, command)?;
            state.edit_error = None;
            state.redraw(cx);
            Ok(())
        })
    }

    pub fn reload<V: FormDialogView>(
        &self,
        action: ParameterAction,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        let owner = self.owner.clone();
        let document = self.document;
        let view = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            let Some(view) = view.upgrade() else { return };
            let dialog = view.read(cx).dialog();
            if dialog.closed() || !dialog.focus.contains_focused(window, cx) {
                return;
            }
            let Some(owner) = owner.upgrade() else { return };
            if owner.read(cx).editor.active_id() != Some(document) {
                return;
            }
            let panel = owner.read(cx).dock.as_ref().map(|d| d.parameters.clone());
            if let Some(panel) = panel {
                view.update(cx, |view, _| view.dialog_mut().close());
                window.close_dialog(cx);
                panel.update(cx, |p, cx| p.open_management(action, window, cx));
            }
        });
    }
}

impl ParametersPanel {
    pub(super) fn prepare_parameter_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<DocumentId> {
        if window.has_active_dialog(cx) {
            return None;
        }
        let owner = self.owner.upgrade()?;
        let state = owner.read(cx);
        let command = state
            .editor
            .command(editor_core::CommandId::CreateParameter);
        if state.modal || !state.editor.command_available(command) {
            return None;
        }
        let document = command.document?;
        self.finish_slider_drag(cx);
        let ok = owner.update(cx, |state, cx| {
            let result = state
                .editor
                .execute_for(document, EditorCommand::FinishEdit);
            let ok = result.is_ok();
            state.complete_edit(result, cx);
            ok
        });
        if !ok {
            return None;
        }
        window.focus(&self.focus, cx);
        Some(document)
    }
}
