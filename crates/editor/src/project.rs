//! Native file dialogs and SDK project lifecycle.
use super::*;
use gpui_kit::component::WindowExt;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportKind {
    Model3,
    Psd,
}

impl Editor {
    pub(super) fn save(&mut self, save_as: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Err(error) = self.document.finish_edit() {
            self.edit_error = Some(error.to_string());
            cx.notify();
            return;
        }
        if !save_as {
            if let Some(path) = self.document.path().map(PathBuf::from) {
                self.save_to(path, cx);
                return;
            }
        }
        let directory = self
            .document
            .path()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        // The SDK publishes a directory containing the manifest and its assets.
        let suggested = format!(
            "{}.kasane",
            self.document.import_name().unwrap_or("Untitled")
        );
        let picker = cx.prompt_for_new_path(&directory, Some(&suggested));
        self.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(Ok(Some(path))) => this.save_to(path, cx),
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.edit_error = Some(error.to_string()),
                    Err(error) => this.edit_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.document.save(&path) {
            Ok(receipt) => {
                self.edit_error = None;
                let mut notices = receipt.warnings;
                notices.extend(receipt.history_warnings);
                if !receipt.durable {
                    notices
                        .push("The project was published, but durability was not confirmed".into());
                }
                self.message = if notices.is_empty() {
                    format!("Saved {}", receipt.manifest.display())
                } else {
                    format!("Saved · {}", notices.join("; "))
                };
            }
            Err(error) => self.edit_error = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(super) fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Err(error) = self.document.finish_edit() {
            self.edit_error = Some(error.to_string());
            cx.notify();
            return;
        }
        let confirmation = self.document.modified().then(|| {
            window.prompt(
                PromptLevel::Warning,
                "Open another project and discard unsaved changes?",
                Some("Cancel and save the current project to keep your changes."),
                &["Cancel", "Discard and Open"],
                cx,
            )
        });
        self.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            if let Some(confirmation) = confirmation {
                if confirmation.await.ok() != Some(1) {
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        cx.notify();
                    });
                    return;
                }
            }
            let picker = match this.update(cx, |_, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: true,
                    multiple: false,
                    prompt: Some("Open".into()),
                })
            }) {
                Ok(picker) => picker,
                Err(_) => return,
            };
            let path = match picker.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                other => {
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        this.edit_error = Some(format!("Could not select a project: {other:?}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let Some(path) = path else {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { Document::open(&path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(document) => {
                        this.document = document;
                        this.view = ViewState::default();
                        this.frame = None;
                        this.edit_error = None;
                        this.message = "Project opened".into();
                        this.sync_controls(window, cx);
                        this.redraw(cx);
                    }
                    Err(error) => this.edit_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn import(&mut self, kind: ImportKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Err(error) = self.document.finish_edit() {
            self.edit_error = Some(error.to_string());
            cx.notify();
            return;
        }
        let confirmation = self.document.modified().then(|| {
            window.prompt(
                PromptLevel::Warning,
                "Import another project and discard unsaved changes?",
                Some("Cancel and save the current project to keep your changes."),
                &["Cancel", "Discard and Import"],
                cx,
            )
        });
        self.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            if let Some(confirmation) = confirmation {
                if confirmation.await.ok() != Some(1) {
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        cx.notify();
                    });
                    return;
                }
            }
            let picker = match this.update(cx, |_, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Import".into()),
                })
            }) {
                Ok(picker) => picker,
                Err(_) => return,
            };
            let source = match picker.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                other => {
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        this.edit_error =
                            Some(format!("Could not select an import file: {other:?}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let Some(source) = source else {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
                return;
            };

            let destination = if kind == ImportKind::Psd {
                let name = source
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Untitled");
                let suggested = format!("{name}.kasane");
                let directory = source.parent().unwrap_or_else(|| std::path::Path::new("/"));
                let picker = match this.update(cx, |_, cx| {
                    cx.prompt_for_new_path(directory, Some(&suggested))
                }) {
                    Ok(picker) => picker,
                    Err(_) => return,
                };
                match picker.await {
                    Ok(Ok(path)) => path,
                    other => {
                        let _ = this.update(cx, |this, cx| {
                            this.busy = false;
                            this.edit_error =
                                Some(format!("Could not select a project location: {other:?}"));
                            cx.notify();
                        });
                        return;
                    }
                }
            } else {
                None
            };
            if kind == ImportKind::Psd && destination.is_none() {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
                return;
            }

            let result = cx
                .background_executor()
                .spawn(async move {
                    match kind {
                        ImportKind::Model3 => Document::import_model3(&source),
                        ImportKind::Psd => {
                            Document::import_psd(&source, destination.as_deref().unwrap())
                        }
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((document, warnings)) => {
                        this.document = document;
                        this.view = ViewState::default();
                        this.frame = None;
                        this.edit_error = None;
                        this.message = if warnings.is_empty() {
                            "Import complete".into()
                        } else {
                            let details = warnings.join("\n");
                            window.open_dialog(cx, move |dialog, _, _| {
                                dialog
                                    .title("Import completed with warnings")
                                    .child(details.clone())
                            });
                            format!("Import complete · {} warning(s)", warnings.len())
                        };
                        this.sync_controls(window, cx);
                        this.redraw(cx);
                    }
                    Err(error) => this.edit_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let confirmation = window.prompt(
            PromptLevel::Warning,
            "Close and discard unsaved changes?",
            Some("Cancel and save the current project to keep your changes."),
            &["Cancel", "Discard and Close"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let discard = confirmation.await.ok() == Some(1);
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if discard {
                    window.remove_window();
                }
                cx.notify();
            });
        })
        .detach();
    }
}
