//! Native dialogs and executor adaptation. Lifecycle decisions stay in zipzi-editor-core.
use super::*;
use gpui_kit::component::WindowExt;
use std::path::{Path, PathBuf};
use zipzi_editor_core::DocumentId;

/// SDK paths stay explicit; only the Editor normalizes a missing extension.
fn native_save_path(mut path: PathBuf) -> Result<PathBuf, ()> {
    match path.extension() {
        None => {
            path.set_extension("zipzi");
            Ok(path)
        }
        Some(extension) if extension.eq_ignore_ascii_case("zipzi") => Ok(path),
        _ => Err(()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportKind {
    Model3,
    Psd,
    Cubism,
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
        if let Some(previous) = self.editor.active_id() {
            if !self.prepare_property_inputs(previous, window, cx) {
                return;
            }
        }
        window.focus(&self.focus, cx);
        match self.editor.activate(id) {
            Ok(()) => self.refresh_document(window, cx),
            Err(error) => {
                self.set_edit_error(error.to_string(), cx);
            }
        }
    }
    pub(super) fn refresh_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focused_viewport = None;
        self.input_router.menu_target.set(None);
        self.sync_workspace_layout(window, cx);
        if let Some(dock) = &self.dock {
            for canvas in dock.viewport_surfaces() {
                let token = canvas.update(cx, |canvas, _| canvas.cancel_pointer());
                if let Some(token) = token {
                    self.editor.cancel_viewport_interaction(token);
                }
            }
        }
        self.animation_anchor = None;
        let active = self.editor.active_id();
        self.animation_content = None;
        self.mesh_thumbnails
            .update(cx, |cache, cx| cache.sync_document(active, window, cx));
        self.viewport_frames.retain(|_, state| {
            state
                .modeling
                .as_ref()
                .is_some_and(|frame| Some(frame.canvas.version.binding.document) == active)
                || state
                    .animation
                    .as_ref()
                    .is_some_and(|frame| Some(frame.state.identity.document) == active)
        });
        if let Some(frozen) = active.and_then(|id| self.frozen_frames.get(&id)) {
            for (&id, state) in frozen {
                self.viewport_frames
                    .entry(id)
                    .or_insert_with(|| state.clone());
            }
        }
        for state in self.viewport_frames.values_mut() {
            state.error = None;
        }
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
        if !self.prepare_property_inputs(id, window, cx) {
            return;
        }
        let Some(summary) = self.editor.documents().into_iter().find(|d| d.id == id) else {
            return;
        };
        if !save_as {
            if let Some(path) = summary.path {
                self.save_to(id, path, close_after, window, cx);
                return;
            }
        }
        window.focus(&self.focus, cx);
        let directory = summary
            .path
            .as_deref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .or_else(|| zipzi_editor_core::platform_paths::user_home().ok())
            .unwrap_or_else(|| std::env::current_dir().expect("file dialog initial directory"));
        let suggested = format!(
            "{}.zipzi",
            summary
                .title
                .unwrap_or_else(|| t!("project.untitled").into_owned())
        );
        let picker = cx.prompt_for_new_path_with_extension(&directory, Some(&suggested), "zipzi");
        self.editor.pause_animation();
        self.animation_anchor = None;
        self.modal = true;
        self.redraw(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(path))) => {
                    cx.background_executor()
                        .spawn(async move {
                            // The native save panel has already confirmed Replace.
                            // Capture its target baseline without blocking the UI;
                            // the worker checks it again before publishing.
                            let normalized = native_save_path(path.clone())
                                .map_err(|_| anyhow::anyhow!(t!("project.extension_error")))?;
                            let changed = normalized != path;
                            let hash = Editor::save_replacement_baseline(&normalized)?;
                            Ok::<_, anyhow::Error>(Some((normalized, hash, changed)))
                        })
                        .await
                }
                Ok(Ok(None)) => Ok(None),
                other => Err(anyhow::anyhow!("{other:?}")),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.modal = false;
                if this.window_exit_interrupted() {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(Some((path, hash, changed))) => {
                        let same_project = this
                            .editor
                            .documents()
                            .iter()
                            .find(|d| d.id == id)
                            .and_then(|d| d.path.as_ref())
                            .is_some_and(|current| {
                                std::fs::canonicalize(&path).as_ref().ok() == Some(current)
                            });
                        if same_project {
                            this.save_to(id, path, close_after, window, cx);
                        } else if changed && hash.is_some() {
                            this.confirm_normalized_save(
                                id,
                                path,
                                close_after,
                                hash.unwrap(),
                                window,
                                cx,
                            );
                        } else {
                            this.save_to_with_baseline(
                                id,
                                path,
                                close_after,
                                hash.as_ref(),
                                window,
                                cx,
                            );
                        }
                    }
                    Ok(None) => {
                        this.editor.cancel_exit();
                        cx.notify();
                    }
                    Err(error) => {
                        this.editor.cancel_exit();
                        this.set_edit_error(error.to_string(), cx);
                    }
                }
            });
        })
        .detach();
    }
    fn confirm_normalized_save(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        close_after: bool,
        baseline: zipzi_sdk::ProjectFingerprint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let confirmation = window.prompt(
            PromptLevel::Warning,
            &t!("project.replace", path = path.display()),
            None,
            &[
                PromptButton::Cancel(t!("project.cancel").into()),
                PromptButton::Ok(t!("project.replace_action").into()),
            ],
            cx,
        );
        self.modal = true;
        cx.spawn_in(window, async move |this, cx| {
            let choice = confirmation.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.modal = false;
                if choice == Some(1) {
                    this.save_to_with_baseline(id, path, close_after, Some(&baseline), window, cx);
                } else {
                    this.editor.cancel_exit();
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn save_to(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_to_with_baseline(id, path, close_after, None, window, cx);
    }

    fn save_to_with_baseline(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        close_after: bool,
        target_fingerprint: Option<&zipzi_sdk::ProjectFingerprint>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prepared = match target_fingerprint {
            Some(hash) => self
                .editor
                .prepare_save_replacing(id, &path, close_after, hash),
            None => self.editor.prepare_save(id, &path, close_after),
        };
        let task = match prepared {
            Ok(task) => task,
            Err(error) => {
                self.editor.cancel_exit();
                self.set_operation_error(Some(id), error.to_string(), cx);
                return;
            }
        };
        if self.editor.active_id() == Some(id) {
            self.frozen_frames.insert(id, self.viewport_frames.clone());
        }
        cx.notify();
        let activity = self.begin_save_status(path.clone(), task.progress(), cx);
        let runtime = self.editor.runtime_host();
        let host = self.host.clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut completion = cx
                .background_executor()
                .spawn(async move { task.run() })
                .await;
            completion.publish(&runtime);
            if let Some(host) = &host {
                host.completed();
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_load_status(activity, cx);
                this.frozen_frames.remove(&id);
                if !this
                    .editor
                    .documents()
                    .iter()
                    .any(|document| document.id == id)
                {
                    let notices = this.editor.sync_runtime_jobs();
                    if !notices.is_empty() {
                        this.set_edit_error(notices.join("; "), cx);
                    }
                    this.continue_exit(window, cx);
                    cx.notify();
                    return;
                }
                let active_before = this.editor.active_id();
                match this.editor.finish_save(&mut completion) {
                    Ok(receipt) => {
                        let mut notices = receipt.warnings;
                        notices.extend(receipt.history_warnings);
                        if !receipt.durable {
                            notices.push(t!("project.durability_warning").into_owned());
                        }
                        if !notices.is_empty() {
                            this.record_operation_warnings(
                                Some(id),
                                &t!("status.save_warnings"),
                                Some(path.clone()),
                                notices.clone(),
                                cx,
                            );
                        }
                        let msg = if notices.is_empty() {
                            t!("project.saved", path = receipt.path.display()).into_owned()
                        } else {
                            format!(
                                "{} · {}",
                                t!("project.saved", path = receipt.path.display()),
                                t!("status.warning_summary", count = notices.len())
                            )
                        };
                        this.set_status_message(msg, cx);
                        this.record_recent_file(path.clone(), cx);
                    }
                    Err(error) => {
                        let conflict = matches!(error.downcast_ref::<zipzi_runtime::RuntimeError>(), Some(zipzi_runtime::RuntimeError::Sdk(error)) if error.code.as_ref() == "PROJECT_CONFLICT");
                        this.set_operation_error(Some(id), error.to_string(), cx);
                        if conflict { this.resolve_save_conflict(id, path.clone(), window, cx); }
                    }
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
    fn resolve_save_conflict(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let confirmation = window.prompt(
            PromptLevel::Warning,
            &t!("project.conflict"),
            Some(&t!("project.conflict_help")),
            &[
                PromptButton::Cancel(t!("project.cancel").into()),
                PromptButton::Ok(t!("editor.save_as").into()),
                PromptButton::Ok(t!("project.reopen").into()),
            ],
            cx,
        );
        self.modal = true;
        cx.spawn_in(window, async move |this, cx| {
            let choice = confirmation.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.modal = false;
                match choice {
                    Some(1) => this.save_document(id, true, false, window, cx),
                    Some(2) => this.confirm_reopen(id, path, window, cx),
                    _ => cx.notify(),
                }
            });
        })
        .detach();
    }
    fn confirm_reopen(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.close_disposition(id).ok() == Some(CloseDisposition::NeedsConfirmation) {
            let confirmation = window.prompt(
                PromptLevel::Warning,
                &t!("project.discard_open"),
                Some(&t!("project.discard_help")),
                &[
                    PromptButton::Cancel(t!("project.cancel").into()),
                    PromptButton::Ok(t!("project.discard_and_open").into()),
                ],
                cx,
            );
            self.modal = true;
            cx.spawn_in(window, async move |this, cx| {
                let choice = confirmation.await.ok();
                let _ = this.update_in(cx, |this, window, cx| {
                    this.modal = false;
                    if choice == Some(1) {
                        this.reopen_project(id, path, window, cx);
                    } else {
                        cx.notify();
                    }
                });
            })
            .detach();
        } else {
            self.reopen_project(id, path, window, cx);
        }
    }
    fn reopen_project(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.editor.prepare_reopen(id, &path) {
            Ok(task) => {
                self.property_forms
                    .entries
                    .retain(|target, _| target.document != id);
                self.run_load(task, path, false, None, window, cx);
            }
            Err(error) => self.set_operation_error(Some(id), error.to_string(), cx),
        }
    }
    pub(super) fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_load(None, window, cx);
    }
    pub(super) fn import(&mut self, kind: ImportKind, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_load(Some(kind), window, cx);
    }
    pub(super) fn open_recent(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.enqueue_projects(vec![path.to_path_buf()], window, cx);
    }
    pub(super) fn enqueue_projects(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.project_queue.lock().unwrap().enqueue(paths);
        if let Err(error) = result {
            self.set_edit_error(error, cx);
        }
        self.drain_project_queue(window, cx);
    }
    pub(super) fn drain_project_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.project_queue.lock().unwrap().is_empty() {
            self.dismiss_welcome(window, cx);
        }
        if self.modal
            || self.editor.exiting()
            || window.has_active_dialog(cx)
            || self.editor.pending_io_count() != 0
        {
            return;
        }
        if self.project_queue.lock().unwrap().is_empty() {
            return;
        }
        if let Some(id) = self.editor.active_id().filter(|_| !self.editor.is_busy()) {
            if !self.prepare_property_inputs(id, window, cx) {
                return;
            }
        }
        let path = self.project_queue.lock().unwrap().pop();
        if let Some(path) = path {
            if !path.exists() {
                self.remove_recent_file(&path, cx);
                self.set_load_error(
                    &path,
                    t!("project.file_not_found", path = path.display()).into_owned(),
                    cx,
                );
                return;
            }
            self.execute_load(LoadRequest::Open(path), None, window, cx);
        }
    }
    pub(super) fn record_recent_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let mut settings = cx.global::<settings::Settings>().clone();
        let error = settings.push_recent_file(path).err().map(|e| e.to_string());
        cx.set_global(settings);
        self.menu_state = Default::default();
        if let Some(e) = error {
            self.set_edit_error(e, cx);
        } else {
            cx.notify();
        }
    }
    pub(super) fn clear_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal || window.has_active_dialog(cx) {
            return;
        }
        let mut settings = cx.global::<settings::Settings>().clone();
        let error = settings.clear_recent_files().err().map(|e| e.to_string());
        cx.set_global(settings);
        self.menu_state = Default::default();
        if let Some(e) = error {
            self.set_edit_error(e, cx);
        } else {
            cx.notify();
        }
    }
    pub(super) fn remove_recent_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        let mut settings = cx.global::<settings::Settings>().clone();
        let error = settings
            .remove_recent_file(path)
            .err()
            .map(|e| e.to_string());
        cx.set_global(settings);
        self.menu_state = Default::default();
        if let Some(e) = error {
            self.set_edit_error(e, cx);
        } else {
            cx.notify();
        }
    }
    fn pick_load(&mut self, kind: Option<ImportKind>, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal || self.editor.exiting() {
            return;
        }
        if let Some(id) = self.editor.active_id().filter(|_| !self.editor.is_busy()) {
            if !self.prepare_property_inputs(id, window, cx) {
                return;
            }
        }
        window.focus(&self.focus, cx);
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: kind.is_none(),
            prompt: Some(if kind.is_some() {
                t!("project.import").into()
            } else {
                t!("project.open").into()
            }),
        };
        let picker = if kind.is_none() {
            cx.prompt_for_paths_with_extension(options, "zipzi")
        } else {
            cx.prompt_for_paths(options)
        };
        self.editor.pause_animation();
        self.animation_anchor = None;
        self.modal = true;
        self.redraw(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let source = match result {
                Ok(Ok(Some(paths))) => {
                    if kind.is_none() {
                        let _ = this.update_in(cx, |this, window, cx| {
                            this.modal = false;
                            this.enqueue_projects(paths, window, cx);
                        });
                        return;
                    }
                    paths.into_iter().next()
                }
                other => {
                    let _ = this.update(cx, |this, cx| {
                        this.modal = false;
                        if !matches!(other, Ok(Ok(None))) {
                            this.set_edit_error(format!("{other:?}"), cx);
                        } else {
                            cx.notify();
                        }
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
                LoadRequest::Psd(source)
            } else if kind == Some(ImportKind::Cubism) {
                LoadRequest::Cubism(source)
            } else if kind == Some(ImportKind::Model3) {
                LoadRequest::Model3(source)
            } else {
                LoadRequest::Open(source)
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.modal = false;
                this.execute_load(request, kind, window, cx);
            });
        })
        .detach();
    }
    fn execute_load(
        &mut self,
        request: LoadRequest,
        kind: Option<ImportKind>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (source, importing) = match &request {
            LoadRequest::Open(path) => (path.clone(), false),
            LoadRequest::Model3(path) | LoadRequest::Psd(path) | LoadRequest::Cubism(path) => {
                (path.clone(), true)
            }
        };
        let task = match self.editor.prepare_load(request) {
            Ok(None) => {
                if let Some(path) = self
                    .editor
                    .document()
                    .and_then(|d| d.path())
                    .map(PathBuf::from)
                {
                    self.record_recent_file(path, cx);
                }
                self.refresh_document(window, cx);
                return;
            }
            Ok(Some(task)) => {
                self.redraw(cx);
                task
            }
            Err(error) => {
                self.set_load_error(&source, error.to_string(), cx);
                return;
            }
        };
        self.run_load(task, source, importing, kind, window, cx);
    }
    fn run_load(
        &mut self,
        task: zipzi_editor_core::LoadTask,
        source: PathBuf,
        importing: bool,
        kind: Option<ImportKind>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let activity = self.begin_load_status(source.clone(), importing, task.progress(), cx);
        let runtime = self.editor.runtime_host();
        let host = self.host.clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut completion = cx
                .background_executor()
                .spawn(async move { task.run() })
                .await;
            completion.publish(&runtime);
            if let Some(host) = &host {
                host.completed();
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_load_status(activity, cx);
                match this.editor.finish_load(completion) {
                    Ok(warnings) => {
                        // Commit/blur the previous property input before displaying the new tab.
                        window.focus(&this.focus, cx);
                        let count = this.record_load_warnings(&source, importing, warnings, cx);
                        let msg = if count > 0 {
                            t!("project.load_warning_count", count = count).into_owned()
                        } else if kind.is_none() {
                            t!("project.opened").into_owned()
                        } else {
                            t!("project.import_complete").into_owned()
                        };
                        this.set_status_message(msg, cx);
                        if let Some(path) = this
                            .editor
                            .document()
                            .and_then(|d| d.path())
                            .map(PathBuf::from)
                        {
                            this.record_recent_file(path, cx);
                        }
                        this.refresh_document(window, cx);
                    }
                    Err(error) => this.set_load_error(&source, error.to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn request_window_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.dismiss_welcome(window, cx);
        if self.modal || window.has_active_dialog(cx) {
            return false;
        }
        if cx.windows().len() == 1
            && !self
                .host
                .as_ref()
                .is_some_and(host::DesktopHost::keep_running_without_windows)
            && self.editor.pending_io_count() == 0
        {
            // Keep the window alive while the ordinary Quit path resolves all
            // resident documents, including tabs that were detached earlier.
            if let Some(host) = &self.host {
                host.release_disconnected_clients();
            }
            self.begin_exit(true, window, cx);
            return false;
        }
        if let Some(id) = self.editor.active_id() {
            if !self.prepare_property_inputs(id, window, cx) {
                return false;
            }
        }
        if let Err(error) = self.editor.suspend_window() {
            self.set_edit_error(error.to_string(), cx);
            return false;
        }
        true
    }
    pub(super) fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.request_window_close(window, cx) {
            window.remove_window();
        }
    }
    pub(super) fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal {
            return;
        }
        if let Some(id) = self.editor.active_id() {
            if !self.prepare_property_inputs(id, window, cx) {
                return;
            }
            if let Err(error) = self.editor.detach_document(id) {
                self.set_edit_error(error.to_string(), cx);
                return;
            }
            self.refresh_document(window, cx);
        }
    }
    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_exit(false, window, cx);
    }
    fn begin_exit(&mut self, on_window_close: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal {
            return;
        }
        if let Some(id) = self.editor.active_id() {
            if !self.prepare_property_inputs(id, window, cx) {
                return;
            }
        }
        self.exit_on_window_close = on_window_close;
        match self.editor.request_exit() {
            Ok(()) => self.continue_exit(window, cx),
            Err(error) => {
                self.exit_on_window_close = false;
                self.set_edit_error(error.to_string(), cx);
            }
        }
    }
    fn window_exit_interrupted(&mut self) -> bool {
        if !self.editor.exiting() {
            self.exit_on_window_close = false;
        }
        if !self.exit_on_window_close {
            return false;
        }
        if let Some(host) = &self.host {
            if host.keep_running_without_windows() {
                self.editor.cancel_exit();
                self.exit_on_window_close = false;
                return true;
            }
            host.release_disconnected_clients();
        }
        false
    }
    fn continue_exit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.exiting() || self.modal || self.window_exit_interrupted() {
            return;
        }
        if let Some(id) = self.editor.next_close() {
            self.close_document(id, window, cx);
        } else {
            cx.quit();
        }
    }
    pub(super) fn close_document(
        &mut self,
        id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.prepare_property_inputs(id, window, cx) {
            return;
        }
        let disposition = self.editor.prepare_close(id);
        match disposition {
            Ok(CloseDisposition::Busy) => {
                if let Err(error) = self.editor.close_document(id, false) {
                    self.set_edit_error(error.to_string(), cx);
                    self.editor.cancel_exit();
                }
            }
            Ok(CloseDisposition::Ready) => {
                let was_active = self.editor.active_id() == Some(id);
                match self.editor.close_document(id, false) {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        self.set_edit_error(error.to_string(), cx);
                        self.editor.cancel_exit();
                        return;
                    }
                }
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
                self.editor.pause_animation();
                self.animation_anchor = None;
                self.modal = true;
                cx.notify();
                cx.spawn_in(window, async move |this, cx| {
                    let choice = confirmation.await.ok();
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.modal = false;
                        if this.window_exit_interrupted() {
                            cx.notify();
                            return;
                        }
                        match choice {
                            Some(1) => {
                                this.property_forms
                                    .entries
                                    .retain(|target, _| target.document != id);
                                let was_active = this.editor.active_id() == Some(id);
                                if let Err(error) = this.editor.close_document(id, true) {
                                    this.set_edit_error(error.to_string(), cx);
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
                self.set_edit_error(error.to_string(), cx);
                self.editor.cancel_exit();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::native_save_path;
    #[test]
    fn save_names_normalize_once_and_reject_other_extensions() {
        assert_eq!(native_save_path("角色".into()), Ok("角色.zipzi".into()));
        assert_eq!(
            native_save_path("角色.ZIPZI".into()),
            Ok("角色.ZIPZI".into())
        );
        assert_eq!(
            native_save_path("one.zipzi.zipzi".into()),
            Ok("one.zipzi.zipzi".into())
        );
        assert!(native_save_path("one.png".into()).is_err());
    }
}
