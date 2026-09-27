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
            self.document
                .import_name()
                .map(str::to_owned)
                .unwrap_or_else(|| t!("project.untitled").into_owned())
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
                    notices.push(t!("project.durability_warning").into_owned());
                }
                self.message = if notices.is_empty() {
                    t!("project.saved", path = receipt.manifest.display()).into_owned()
                } else {
                    t!("project.saved_with_warnings", warnings = notices.join("; ")).into_owned()
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
                &t!("project.discard_open"),
                Some(&t!("project.discard_help")),
                &[
                    PromptButton::Cancel(t!("project.cancel").into()),
                    PromptButton::Other(t!("project.discard_and_open").into()),
                ],
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
                    prompt: Some(t!("project.open").into()),
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
                        this.edit_error = Some(
                            t!("project.select_project_error", error = format!("{other:?}"))
                                .into_owned(),
                        );
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
                        this.message = t!("project.opened").into_owned();
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
                &t!("project.discard_import"),
                Some(&t!("project.discard_help")),
                &[
                    PromptButton::Cancel(t!("project.cancel").into()),
                    PromptButton::Other(t!("project.discard_and_import").into()),
                ],
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
                    prompt: Some(t!("project.import").into()),
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
                        this.edit_error = Some(
                            t!("project.select_import_error", error = format!("{other:?}"))
                                .into_owned(),
                        );
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
                    .map(str::to_owned)
                    .unwrap_or_else(|| t!("project.untitled").into_owned());
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
                            this.edit_error = Some(
                                t!(
                                    "project.select_location_error",
                                    error = format!("{other:?}")
                                )
                                .into_owned(),
                            );
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
            &t!("project.discard_close"),
            Some(&t!("project.discard_help")),
            &[
                PromptButton::Cancel(t!("project.cancel").into()),
                PromptButton::Other(t!("project.discard_and_close").into()),
            ],
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
