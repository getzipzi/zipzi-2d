//! Desktop lifetime is independent of window entities and background callers.
use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zipzi_editor_core::RuntimeHost;

#[derive(Clone)]
pub(super) struct DesktopHost {
    pub(super) runtime: RuntimeHost,
    pub(super) protocol: Rc<RefCell<zipzi_runtime::protocol::Protocol>>,
    resident: bool,
    parked: Rc<RefCell<Option<Editor>>>,
    pub(super) pending_quit: Rc<Cell<bool>>,
    outcomes: Rc<RefCell<std::collections::VecDeque<zipzi_editor_core::RuntimeJobInfo>>>,
    notices: Rc<RefCell<Vec<String>>>,
}
impl Global for DesktopHost {}
impl DesktopHost {
    pub(super) fn new(resident: bool) -> Self {
        let runtime = RuntimeHost::new();
        let editor = Editor::with_runtime(runtime.clone());
        Self {
            runtime,
            protocol: Rc::new(RefCell::new(
                zipzi_runtime::protocol::Protocol::default()
                    .with_window_support()
                    .with_mesh_generator(zipzi_editor_core::adaptive_mesh::remote::generate),
            )),
            resident,
            parked: Rc::new(RefCell::new(Some(editor))),
            pending_quit: Default::default(),
            outcomes: Default::default(),
            notices: Default::default(),
        }
    }
    pub(super) fn keep_running_without_windows(&self) -> bool {
        self.resident
            || self.protocol.borrow().has_connected_clients()
            || !self.runtime.read().pending_jobs().is_empty()
    }
    pub(super) fn release_disconnected_clients(&self) {
        self.protocol
            .borrow_mut()
            .release_disconnected_clients(&mut self.runtime.write());
    }
    pub(super) fn take_editor(&self) -> (Editor, Vec<String>) {
        let mut editor = self
            .parked
            .borrow_mut()
            .take()
            .unwrap_or_else(|| Editor::with_runtime(self.runtime.clone()));
        let mut notices = std::mem::take(&mut *self.notices.borrow_mut());
        notices.extend(
            editor
                .attach_resident_sessions()
                .unwrap_or_else(|e| vec![e.to_string()]),
        );
        (editor, notices)
    }
    pub(super) fn park(&self, mut editor: Editor) {
        if let Some(document) = editor.document() {
            let _ = self
                .runtime
                .write()
                .set_editor_context(document.runtime_session_id(), None);
        }
        let _ = editor.suspend_window();
        if let Some(mut dev) = crate::dev_session::DevSession::from_env() {
            let _ = dev.sync(&editor);
        }
        *self.parked.borrow_mut() = Some(editor);
    }
    /// Complete presentation bookkeeping in a parked workspace, after Runtime
    /// already returned SDK ownership. This never opens a window.
    pub(super) fn completed(&self) {
        if let Ok(jobs) = self.runtime.read().list_jobs(self.runtime.client_id()) {
            let mut outcomes = self.outcomes.borrow_mut();
            for job in jobs {
                if job.state.is_terminal() && !outcomes.iter().any(|old| old.id == job.id) {
                    outcomes.push_back(job);
                }
            }
            while outcomes.len() > 32 {
                outcomes.pop_front();
            }
        }
        if let Some(editor) = self.parked.borrow_mut().as_mut() {
            self.notices.borrow_mut().extend(editor.sync_runtime_jobs());
            if let Some(mut dev) = crate::dev_session::DevSession::from_env() {
                let _ = dev.sync(editor);
            }
        }
    }
    #[cfg(test)]
    pub(super) fn outcomes(&self) -> Vec<zipzi_editor_core::RuntimeJobInfo> {
        self.outcomes.borrow().iter().cloned().collect()
    }
    pub(super) fn new_document(&self) {
        if let Some(editor) = self.parked.borrow_mut().as_mut() {
            if let Err(error) = editor.new_document() {
                self.notices.borrow_mut().push(error.to_string());
            }
        }
    }
}
impl Drop for EditorView {
    fn drop(&mut self) {
        if let Some(host) = &self.host {
            let editor =
                std::mem::replace(&mut self.editor, Editor::with_runtime(host.runtime.clone()));
            host.park(editor);
        }
    }
}

pub(super) fn open_window(
    cx: &mut App,
    activate: bool,
) -> Option<(AnyWindowHandle, Entity<EditorView>)> {
    if !cx.windows().is_empty() {
        if activate {
            cx.activate(true);
        }
        return None;
    }
    let opened = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1100.), px(740.)),
                cx,
            ))),
            window_min_size: Some(size(px(800.), px(480.))),
            ..workspace::title_bar::window_options()
        },
        cx,
        |window, cx| cx.new(|cx| EditorView::new(window, cx)),
    )
    .expect("open Editor window");
    if activate {
        cx.activate(true);
    }
    Some(opened)
}
