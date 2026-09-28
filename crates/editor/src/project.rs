//! Native dialogs and executor adaptation. Lifecycle decisions stay in editor-core.
use super::*;
use editor_core::DocumentId;
use gpui_kit::component::WindowExt;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportKind {
    Model3,
    Psd,
}

impl EditorView {
    pub(super) fn activate_document(
        &mut self,
        id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal {
            return;
        }
        // Blur the old property input before changing its document target.
        window.focus(&self.focus, cx);
        match self.editor.activate(id) {
            Ok(()) => self.refresh_document(window, cx),
            Err(error) => {
                self.edit_error = Some(error.to_string());
                cx.notify();
            }
        }
    }
    pub(super) fn refresh_document(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let active = self.editor.active_id();
        // A repeated tab click or save completion may reuse the same scene.
        // Keep that document's last image while its replacement is pending.
        self.frame = self
            .frame
            .take()
            .filter(|frame| Some(frame.canvas.version.document) == active)
            .or_else(|| active.and_then(|id| self.frozen_frames.get(&id).cloned()));
        self.render_error = None;
        self.redraw(cx);
    }
    pub(super) fn save(&mut self, save_as: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal || self.editor.is_busy() {
            return;
        }
        if let Some(id) = self.editor.active_id() {
            self.save_document(id, save_as, false, window, cx);
        }
    }
    fn save_document(
        &mut self,
        id: DocumentId,
        save_as: bool,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(summary) = self.editor.documents().into_iter().find(|d| d.id == id) else {
            return;
        };
        if !save_as {
            if let Some(path) = summary.path {
                self.save_to(id, path, close_after, window, cx);
                return;
            }
        }
        if let Err(error) = self.editor.execute_for(id, EditorCommand::FinishEdit) {
            self.edit_error = Some(error.to_string());
            self.editor.cancel_exit();
            cx.notify();
            return;
        }
        window.focus(&self.focus, cx);
        let directory = summary
            .path
            .as_deref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        let suggested = format!(
            "{}.kasane",
            summary
                .title
                .unwrap_or_else(|| t!("project.untitled").into_owned())
        );
        let picker = cx.prompt_for_new_path(&directory, Some(&suggested));
        self.modal = true;
        self.redraw(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.modal = false;
                match result {
                    Ok(Ok(Some(path))) => this.save_to(id, path, close_after, window, cx),
                    other => {
                        this.editor.cancel_exit();
                        if !matches!(other, Ok(Ok(None))) {
                            this.edit_error = Some(format!("{other:?}"));
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn save_to(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = match self.editor.prepare_save(id, &path, close_after) {
            Ok(task) => task,
            Err(error) => {
                self.editor.cancel_exit();
                self.edit_error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        if self.editor.active_id() == Some(id) {
            if let Some(frame) = &self.frame {
                self.frozen_frames.insert(id, frame.clone());
            }
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let mut completion = cx
                .background_executor()
                .spawn(async move { task.run() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.frozen_frames.remove(&id);
                let active_before = this.editor.active_id();
                match this.editor.finish_save(&mut completion) {
                    Ok(receipt) => {
                        this.edit_error = None;
                        let mut notices = receipt.warnings;
                        notices.extend(receipt.history_warnings);
                        if !receipt.durable {
                            notices.push(t!("project.durability_warning").into_owned());
                        }
                        this.message = if notices.is_empty() {
                            t!("project.saved", path = receipt.manifest.display()).into_owned()
                        } else {
                            t!("project.saved_with_warnings", warnings = notices.join("; "))
                                .into_owned()
                        };
                    }
                    Err(error) => this.edit_error = Some(error.to_string()),
                }
                // A background save must not blur or rewrite another document's input.
                if active_before == Some(id) || active_before != this.editor.active_id() {
                    this.refresh_document(window, cx);
                }
                this.continue_exit(window, cx);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_load(None, window, cx);
    }
    pub(super) fn import(&mut self, kind: ImportKind, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_load(Some(kind), window, cx);
    }
    fn pick_load(&mut self, kind: Option<ImportKind>, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal || self.editor.exiting() {
            return;
        }
        window.focus(&self.focus, cx);
        if self.editor.active_id().is_some() && !self.editor.is_busy() {
            if let Err(error) = self.editor.execute(EditorCommand::FinishEdit) {
                self.edit_error = Some(error.to_string());
                cx.notify();
                return;
            }
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: kind.is_none(),
            multiple: false,
            prompt: Some(if kind.is_some() {
                t!("project.import").into()
            } else {
                t!("project.open").into()
            }),
        });
        self.modal = true;
        self.redraw(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let source = match result {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                other => {
                    let _ = this.update(cx, |this, cx| {
                        this.modal = false;
                        if !matches!(other, Ok(Ok(None))) {
                            this.edit_error = Some(format!("{other:?}"));
                        }
                        cx.notify();
                    });
                    None
                }
            };
            let Some(source) = source else {
                let _ = this.update(cx, |this, cx| {
                    this.modal = false;
                    cx.notify();
                });
                return;
            };
            let request = if kind == Some(ImportKind::Psd) {
                let suggested = format!(
                    "{}.kasane",
                    source.file_stem().unwrap_or_default().to_string_lossy()
                );
                let picker = match this.update(cx, |_, cx| {
                    cx.prompt_for_new_path(
                        source.parent().unwrap_or(std::path::Path::new("/")),
                        Some(&suggested),
                    )
                }) {
                    Ok(picker) => picker,
                    Err(_) => return,
                };
                match picker.await {
                    Ok(Ok(Some(destination))) => LoadRequest::Psd {
                        source,
                        destination,
                    },
                    other => {
                        let _ = this.update(cx, |this, cx| {
                            this.modal = false;
                            if !matches!(other, Ok(Ok(None))) {
                                this.edit_error = Some(format!("{other:?}"));
                            }
                            cx.notify();
                        });
                        return;
                    }
                }
            } else if kind == Some(ImportKind::Model3) {
                LoadRequest::Model3(source)
            } else {
                LoadRequest::Open(source)
            };
            let task = match this.update_in(cx, |this, window, cx| {
                this.modal = false;
                let task = this.editor.prepare_load(request);
                match task {
                    Ok(None) => {
                        this.refresh_document(window, cx);
                        None
                    }
                    Ok(task) => {
                        this.redraw(cx);
                        task
                    }
                    Err(error) => {
                        this.edit_error = Some(error.to_string());
                        cx.notify();
                        None
                    }
                }
            }) {
                Ok(Some(task)) => task,
                _ => return,
            };
            let completion = cx
                .background_executor()
                .spawn(async move { task.run() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                match this.editor.finish_load(completion) {
                    Ok(warnings) => {
                        // Commit/blur the previous property input before displaying the new tab.
                        window.focus(&this.focus, cx);
                        this.edit_error = None;
                        this.message = if kind.is_none() {
                            t!("project.opened").into_owned()
                        } else if warnings.is_empty() {
                            t!("project.import_complete").into_owned()
                        } else {
                            let details = warnings.join("\n");
                            window.open_dialog(cx, move |dialog, _, _| {
                                dialog
                                    .title(t!("project.import_warnings_title"))
                                    .child(details.clone())
                            });
                            t!("project.import_warning_count", count = warnings.len()).into_owned()
                        };
                        this.refresh_document(window, cx);
                    }
                    Err(error) => this.edit_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal {
            return;
        }
        if let Some(id) = self.editor.active_id() {
            self.close_document(id, window, cx);
        }
    }
    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal {
            return;
        }
        match self.editor.request_exit() {
            Ok(()) => self.continue_exit(window, cx),
            Err(error) => {
                self.edit_error = Some(error.to_string());
                cx.notify();
            }
        }
    }
    fn continue_exit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.exiting() || self.modal {
            return;
        }
        if let Some(id) = self.editor.next_close() {
            self.close_document(id, window, cx);
        } else {
            window.remove_window();
        }
    }
    pub(super) fn close_document(
        &mut self,
        id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.editor.prepare_close(id) {
            Ok(CloseDisposition::Busy) => {
                let _ = self.editor.close_document(id, false);
            }
            Ok(CloseDisposition::Ready) => {
                let was_active = self.editor.active_id() == Some(id);
                let _ = self.editor.close_document(id, false);
                if was_active {
                    self.refresh_document(window, cx);
                }
                self.continue_exit(window, cx);
            }
            Ok(CloseDisposition::NeedsConfirmation) => {
                let title = self
                    .editor
                    .documents()
                    .into_iter()
                    .find(|d| d.id == id)
                    .and_then(|d| d.title)
                    .unwrap_or_else(|| t!("project.untitled").into_owned());
                window.focus(&self.focus, cx);
                let confirmation = window.prompt(
                    PromptLevel::Warning,
                    &t!("project.save_close", title = title),
                    None,
                    &[
                        PromptButton::Cancel(t!("project.cancel").into()),
                        PromptButton::Other(t!("project.discard_and_close").into()),
                        PromptButton::Other(t!("menu.save").into()),
                    ],
                    cx,
                );
                self.modal = true;
                cx.notify();
                cx.spawn_in(window, async move |this, cx| {
                    let choice = confirmation.await.ok();
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.modal = false;
                        match choice {
                            Some(1) => {
                                let was_active = this.editor.active_id() == Some(id);
                                if let Err(error) = this.editor.close_document(id, true) {
                                    this.edit_error = Some(error.to_string());
                                    this.editor.cancel_exit();
                                }
                                if was_active {
                                    this.refresh_document(window, cx);
                                }
                                this.continue_exit(window, cx);
                            }
                            Some(2) => this.save_document(id, false, true, window, cx),
                            _ => this.editor.cancel_exit(),
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
            Err(error) => {
                self.edit_error = Some(error.to_string());
                self.editor.cancel_exit();
                cx.notify();
            }
        }
    }
}
