pub(crate) mod compact;
pub(crate) mod form_dialog;
pub(crate) mod mesh_generation;
mod mesh_preview;
pub(crate) mod preview_viewport;
pub(crate) mod slide_number_input;

use super::*;
use gpui_kit::component::{ActiveTheme, WindowExt};

impl EditorView {
    pub fn has_focused_input(&self, window: &mut Window, cx: &mut App) -> bool {
        window.has_focused_input(cx)
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_viewports();
        if self.sync_viewport_visibility(window, cx) {
            self.redraw(cx);
        }
        if let Some(session) = &mut self.dev_session {
            if let Err(error) = session.sync(&self.editor) {
                self.edit_error = Some(error.to_string());
            }
        }
        if preview::device_changed(window, self.gpu_device.as_ref()) {
            self.connect_preview(window, cx);
        }
        let modal = self.modal || window.has_active_dialog(cx);
        if modal
            && (self.editor.animation().is_some_and(|a| a.playing)
                || self.editor.timeline_edit().is_some()
                || self.editor.gesture_active()
                || self.editor.viewport_interaction_active())
        {
            if let Some(document) = self.editor.active_id() {
                if let Err(error) = self
                    .editor
                    .end_editing(document, editor_core::EditingLifecycle::BlockingUi)
                {
                    self.edit_error = Some(error.to_string());
                }
                self.redraw(cx);
            }
        }
        self.schedule_animation_frame(window, cx);
        self.refresh_menus(modal, window, cx);
        let theme = cx.theme();

        self.action_handlers(modal, cx)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_size(compact::rem(compact::BODY))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.filter_repeat(event, window, cx)
            }))
            .child(workspace::render_title_bar(self, modal, window, cx))
            .children(
                self.dock
                    .as_ref()
                    .map(|dock| div().flex_1().min_h_0().child(dock.area.clone())),
            )
            .child(self.status_bar.clone())
            .into_any_element()
    }
}
