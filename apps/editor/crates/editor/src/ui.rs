pub(crate) mod about;
pub(crate) mod color_picker;
pub(crate) mod compact;
pub(crate) mod export;
pub(crate) mod form_dialog;
pub(crate) mod mesh_generation;
pub(crate) mod mesh_generation_preview;
mod mesh_preview;
pub(crate) mod object_identity;
pub(crate) mod property_details;
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
        let open: std::collections::HashSet<_> = self
            .editor
            .documents()
            .into_iter()
            .map(|doc| doc.id)
            .collect();
        self.property_forms
            .entries
            .retain(|target, _| open.contains(&target.document));
        self.sync_workspace_layout(window, cx);
        if let Some(dock) = &self.dock {
            let visible =
                window.is_visible() && dock.displayed(editor_core::PanelKind::Properties, cx);
            let pending = if !visible && dock.properties.read(cx).is_visible() {
                let composing = dock
                    .properties
                    .update(cx, |panel, cx| panel.composing_input(window, cx));
                dock.properties
                    .read(cx)
                    .focused_input_snapshot(window, cx)
                    .map(|snapshot| (snapshot, composing))
            } else {
                None
            };
            let cleanup = dock
                .properties
                .update(cx, |panel, cx| panel.set_visible(visible, cx));
            if let Some((target, id, token)) = cleanup {
                token.cancel(&mut self.editor);
                if let Some(form) = self.property_forms.entries.get_mut(&target) {
                    form.reload_field(id);
                }
            } else if let Some(((target, id, raw), composing)) = pending {
                if let Some(form) = self.property_forms.entries.get_mut(&target) {
                    if composing {
                        form.reload_field(id);
                    } else if let Some(spec) = target
                        .definition(&self.editor)
                        .fields
                        .iter()
                        .map(|field| &field.binding)
                        .find(|s| s.key == id)
                    {
                        if form.fields[&id].raw != raw {
                            form.edit(spec, raw);
                        }
                    }
                }
                if !composing {
                    let _ = self.property_forms.commit(&mut self.editor, &target, id);
                }
            }
        }
        self.sync_timeline_visibility(window, cx);
        if self.sync_viewport_visibility(window, cx) {
            self.redraw(cx);
        }
        if let Some(viewport) = self.focused_viewport_id(window, cx) {
            self.focused_viewport = Some(viewport);
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
        if modal && self.suspend_input(editor_core::EditingLifecycle::BlockingUi, window, cx) {
            self.redraw(cx);
        }
        self.schedule_animation_frame(window, cx);
        self.refresh_menus(modal, window, cx);
        let theme = cx.theme();

        let workspace = self.dock.as_ref().map(|dock| {
            div()
                .flex_1()
                .min_h_0()
                .child(dock.area.clone())
                .into_any_element()
        });
        #[cfg(any(test, feature = "test-support"))]
        let workspace = if let Some(scene) = &self.viewport_scene {
            Some(
                div()
                    .id("viewport-test-scene")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .children(scene.panels.iter().enumerate().map(|(index, panel)| {
                        div()
                            .id(("viewport-test-panel", index))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(panel.clone())
                    }))
                    .into_any_element(),
            )
        } else {
            workspace
        };
        self.action_handlers(modal, cx)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_size(compact::rem(compact::BODY))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.route_key(event, window, cx)
            }))
            .child(workspace::render_title_bar(self, modal, window, cx))
            .children(workspace)
            .child(self.status_bar.clone())
            .child(self.pointer_observer(window, cx))
            .into_any_element()
    }
}
