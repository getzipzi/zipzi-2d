use super::*;
use editor_core::CommandId;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::Input,
    slider::Slider,
};
impl EditorView {
    pub(super) fn render_tool_panel(
        &mut self,
        kind: docking::PanelKind,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = self.editor.active_id();
        let selected = self.editor.selected_layer();
        let disabled = self.modal || self.editor.is_busy() || selected.is_none();
        let theme = cx.theme();
        let content = div()
            .size_full()
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .text_color(theme.foreground)
            .text_sm();
        match kind {
            docking::PanelKind::Layers => content
                .id("layers")
                .overflow_y_scroll()
                .children(self.editor.layers().into_iter().map(|layer| {
                    let id = layer.id.clone();
                    let target = self.editor.active_id();
                    Button::new(SharedString::from(layer.id.clone()))
                        .label(if layer.name.is_empty() {
                            t!("editor.untitled_layer").into_owned()
                        } else {
                            layer.name
                        })
                        .small()
                        .w_full()
                        .disabled(self.modal || self.editor.is_busy())
                        .when(selected.as_ref().is_some_and(|s| s.id == layer.id), |b| {
                            b.primary()
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if target != this.editor.active_id() {
                                return;
                            }
                            let Some(target) = target else {
                                return;
                            };
                            let result = this
                                .editor
                                .execute_for(target, EditorCommand::Select(id.clone()));
                            this.complete_edit(result, window, cx, true);
                        }))
                }))
                .into_any_element(),
            docking::PanelKind::Properties => content
                .child(t!("editor.name"))
                .child(Input::new(&self.name).disabled(disabled))
                .child(
                    Checkbox::new("visible")
                        .label(SharedString::from(t!("editor.visible")))
                        .checked(selected.as_ref().is_some_and(|s| s.visible))
                        .disabled(disabled)
                        .on_change(cx.listener(move |this, value, window, cx| {
                            if target != this.editor.active_id() {
                                return;
                            }
                            let Some(target) = target else {
                                return;
                            };
                            let result = this
                                .editor
                                .execute_for(target, EditorCommand::SetVisible(*value));
                            this.complete_edit(result, window, cx, true);
                        })),
                )
                .child(t!(
                    "editor.opacity",
                    percent = format!("{:.0}", selected.as_ref().map_or(0., |s| s.opacity * 100.))
                ))
                .child(Slider::new(&self.opacity).disabled(disabled))
                .child(div().text_xs().child(t!("editor.name_help")))
                .child(div().text_xs().child(selected.map_or_else(
                    || t!("editor.no_layer").into_owned(),
                    |s| {
                        t!(
                            "editor.mesh_info",
                            vertices = s.vertices,
                            triangles = s.triangles
                        )
                        .into_owned()
                    },
                )))
                .into_any_element(),
        }
    }
}
impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let status = self
            .edit_error
            .clone()
            .unwrap_or_else(|| self.message.clone());
        let content = self
            .menu_actions(modal, cx)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.filter_repeat(event, window, cx)
            }))
            .child(
                div()
                    .flex()
                    .h(px(40.))
                    .flex_shrink_0()
                    .items_center()
                    .px_3()
                    .gap_2()
                    .child(div().flex_1().child(t!("app.title")))
                    .children(
                        [
                            (CommandId::Vertices, "menu.vertices"),
                            (CommandId::ZoomOut, "menu.zoom_out"),
                            (CommandId::ZoomIn, "menu.zoom_in"),
                            (CommandId::ResetView, "menu.reset_view"),
                        ]
                        .into_iter()
                        .map(|(id, label)| {
                            Button::new(label)
                                .small()
                                .label(t!(label).into_owned())
                                .tooltip_with_action(
                                    t!(label).into_owned(),
                                    &menus::RunCommand { id },
                                    Some("KasaneEditor KasaneCanvas"),
                                )
                                .disabled(
                                    modal
                                        || !self.editor.command_available(self.editor.command(id)),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_command(id, window, cx)
                                }))
                        }),
                    ),
            )
            .children(
                self.dock
                    .as_ref()
                    .map(|dock| div().flex_1().min_h_0().child(dock.area.clone())),
            )
            .child(
                div()
                    .h(px(28.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .text_xs()
                    .child(status),
            );
        content.into_any_element()
    }
}
