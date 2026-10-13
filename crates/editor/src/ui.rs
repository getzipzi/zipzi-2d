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
        if let Some(session) = &mut self.dev_session {
            if let Err(error) = session.sync(&self.editor) {
                self.edit_error = Some(error.to_string());
            }
        }
        if preview::device_changed(window, self.gpu_device.as_ref()) {
            self.connect_preview(window, cx);
        }
        let modal = self.modal || window.has_active_dialog(cx);
        if modal && self.editor.animation().is_some_and(|a| a.playing) {
            self.editor.pause_animation();
            self.redraw(cx);
        }
        if modal && self.editor.gesture_active() {
            self.editor.cancel_gesture();
            self.redraw(cx);
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
            .text_sm()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.filter_repeat(event, window, cx)
            }))
            // 1. Global Document TabBar
            .child(workspace::render_tab_bar(self, modal, window, cx))
            // 3. Central Dock Area
            .children(
                self.dock
                    .as_ref()
                    .map(|dock| div().flex_1().min_h_0().child(dock.area.clone())),
            )
            // 4. Figma-style Status Bar
            .when(
                self.editor.workspace() == editor_core::WorkspaceKind::Animation,
                |e| e.child(self.playback_bar.clone()),
            )
            .child(self.status_bar.clone())
            .into_any_element()
    }
}
