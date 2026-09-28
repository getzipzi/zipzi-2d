use super::*;
use gpui_kit::component::{ActiveTheme, WindowExt};

impl EditorView {
    pub fn has_focused_input(&self, window: &Window, cx: &App) -> bool {
        self.dock
            .as_ref()
            .is_some_and(|dock| dock.has_focused_input(window, cx))
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
        if modal && self.editor.gesture_active() {
            self.editor.cancel_gesture();
            self.redraw(cx);
        }
        self.refresh_menus(modal, window, cx);
        let theme = cx.theme();

        self.menu_actions(modal, cx)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "space"
                    && event.keystroke.modifiers == Modifiers::default()
                    && !event.is_held
                    && !this.modal
                    && !window.has_active_dialog(cx)
                    && !this.has_focused_input(window, cx)
                {
                    if let Some(dock) = &this.dock {
                        dock.canvas
                            .update(cx, |canvas, cx| canvas.set_temporary_hand(true, window, cx));
                        cx.stop_propagation();
                        return;
                    }
                }
                this.filter_repeat(event, window, cx)
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, window, cx| {
                if event.keystroke.key == "space" {
                    if let Some(dock) = &this.dock {
                        dock.canvas.update(cx, |canvas, cx| {
                            canvas.set_temporary_hand(false, window, cx)
                        });
                    }
                }
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
            .child(self.status_bar.clone())
            .into_any_element()
    }
}
