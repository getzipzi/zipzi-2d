//! Per-format settings followed by the native save panel.
use super::*;
use editor_core::{Command, CommandId, DocumentId, TransferFormat, TransferSnapshot};
use std::path::PathBuf;
impl EditorView {
    pub(super) fn open_export(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = command.document() else { return };
        if !self.prepare_property_inputs(id, window, cx) {
            return;
        }
        if let Err(error) = self
            .editor
            .end_editing(id, editor_core::EditingLifecycle::BlockingUi)
        {
            self.set_edit_error(error.to_string(), cx);
            return;
        }
        self.animation_anchor = None;
        let format = if command.id == CommandId::ExportRuntime {
            TransferFormat::Runtime
        } else {
            TransferFormat::Cubism
        };
        let owner = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            if let Err(error) =
                crate::ui::export::ExportForm::open(owner.clone(), id, format, window, cx)
            {
                if let Some(owner) = owner.upgrade() {
                    owner.update(cx, |view, cx| view.set_edit_error(error.to_string(), cx));
                }
            }
        });
    }
    pub(crate) fn pick_export(
        &mut self,
        id: DocumentId,
        format: TransferFormat,
        snapshot: TransferSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(summary) = self.editor.documents().into_iter().find(|d| d.id == id) else {
            return;
        };
        let directory = summary
            .path
            .as_deref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        let suggested = summary
            .title
            .unwrap_or_else(|| t!("project.untitled").into_owned())
            .trim_end_matches(".kasane")
            .to_string();
        let picker = cx.prompt_for_new_path(&directory, Some(&suggested));
        self.modal = true;
        self.redraw(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let destination = match result {
                Ok(Ok(Some(path))) => path,
                other => {
                    let _ = this.update(cx, |view, cx| {
                        view.modal = false;
                        if !matches!(other, Ok(Ok(None))) {
                            view.set_edit_error(format!("{other:?}"), cx);
                        } else {
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            let _ = this.update(cx, |view, cx| {
                view.modal = false;
                view.set_status_message(t!("transfer.exporting").into_owned(), cx);
            });
            let result = cx
                .background_executor()
                .spawn(async move {
                    std::thread::Builder::new()
                        .name("kasane-export".into())
                        .stack_size(8 * 1024 * 1024)
                        .spawn(move || snapshot.publish(format, &destination))
                        .map_err(anyhow::Error::from)
                        .and_then(|worker| {
                            worker
                                .join()
                                .map_err(|_| anyhow::anyhow!("Export worker panicked"))?
                                .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))
                        })
                })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(publication) => {
                    if publication.warnings.is_empty() {
                        view.set_status_message(t!("transfer.complete").into_owned(), cx);
                    } else {
                        view.set_status_message(publication.warnings.join("\n"), cx);
                    }
                }
                Err(error) => view.set_edit_error(error.to_string(), cx),
            });
        })
        .detach();
    }
}
