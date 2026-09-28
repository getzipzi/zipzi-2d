use super::*;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::Input,
    slider::Slider,
};

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let target = preview::target_config(window);
        let target_changed = target != self.preview_target;
        self.preview_target = target;
        if preview::device_changed(window, self.gpu_device.as_ref()) {
            self.connect_preview(window, cx);
        } else if target_changed {
            self.redraw(cx);
        }
        let theme = cx.theme();
        let selected = self.document.selected_layer();
        let disabled = self.busy || selected.is_none();
        let title = self
            .document
            .path()
            .and_then(|path| path.parent())
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .or_else(|| self.document.import_name().map(str::to_owned))
            .unwrap_or_else(|| t!("editor.untitled_project").into_owned());
        let status = self.edit_error.clone().unwrap_or_else(|| {
            if self.busy {
                t!("editor.processing").into_owned()
            } else {
                self.message.clone()
            }
        });
        let mut canvas = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(theme.muted)
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus, cx);
                }),
            );
        if let Some(error) = &self.render_error {
            canvas = canvas.child(div().text_color(theme.danger).child(error.clone()));
        } else if let Some(frame) = &self.frame {
            canvas = canvas.child(preview::element(frame.clone()));
        } else {
            canvas = canvas.child(t!("editor.starting_preview"));
        }
        let layers = self.document.layers();
        let content = self
            .menu_actions(cx)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && !this.busy
                    && this.name.focus_handle(cx).is_focused(window)
                {
                    this.document.cancel_edit();
                    this.complete_edit(Ok(()), window, cx, true);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .h(px(preview::DOCUMENT_BAR_HEIGHT))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(div().flex_1().truncate().child(format!(
                        "{}{}",
                        title,
                        if self.document.modified() {
                            t!("editor.unsaved")
                        } else {
                            "".into()
                        }
                    )))
                    .child(
                        Button::new("zoom-out")
                            .label("−")
                            .small()
                            .disabled(self.view.zoom <= 0.25)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.zoom_by(-0.25, cx);
                            })),
                    )
                    .child(
                        div()
                            .w_12()
                            .text_center()
                            .child(format!("{:.0}%", self.view.zoom * 100.)),
                    )
                    .child(
                        Button::new("zoom-in")
                            .label("＋")
                            .small()
                            .disabled(self.view.zoom >= 3.)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.zoom_by(0.25, cx);
                            })),
                    )
                    .child(
                        Button::new("reset-view")
                            .label(t!("menu.reset_view"))
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_view(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("layers")
                            .w(px(preview::LAYERS_WIDTH))
                            .flex_shrink_0()
                            .h_full()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .overflow_y_scroll()
                            .border_r_1()
                            .border_color(theme.border)
                            .bg(theme.sidebar)
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(t!("editor.layers")),
                            )
                            .children(layers.into_iter().map(|layer| {
                                let selected =
                                    self.document.selected_id() == Some(layer.id.as_str());
                                let id = layer.id.clone();
                                Button::new(SharedString::from(layer.id))
                                    .label(if layer.name.is_empty() {
                                        t!("editor.untitled_layer")
                                    } else {
                                        layer.name.into()
                                    })
                                    .small()
                                    .w_full()
                                    .disabled(self.busy)
                                    .when(selected, |button| button.primary())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let result = this.document.select(&id);
                                        this.complete_edit(result, window, cx, true);
                                    }))
                            })),
                    )
                    .child(canvas)
                    .child(
                        div()
                            .w(px(preview::PROPERTIES_WIDTH))
                            .flex_shrink_0()
                            .h_full()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .border_l_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(t!("editor.properties")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(t!("editor.name"))
                                    .child(Input::new(&self.name).disabled(disabled)),
                            )
                            .child(
                                Checkbox::new("visible")
                                    .label(SharedString::from(t!("editor.visible")))
                                    .checked(selected.as_ref().is_some_and(|layer| layer.visible))
                                    .disabled(disabled)
                                    .on_change(cx.listener(|this, checked, window, cx| {
                                        let result = this.document.set_visible(*checked);
                                        this.complete_edit(result, window, cx, true);
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(t!(
                                        "editor.opacity",
                                        percent = format!(
                                            "{:.0}",
                                            selected
                                                .as_ref()
                                                .map_or(0., |layer| layer.opacity * 100.)
                                        )
                                    ))
                                    .child(Slider::new(&self.opacity).disabled(disabled)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(t!("editor.name_help")),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                selected.as_ref().map_or_else(
                                    || t!("editor.no_layer"),
                                    |layer| {
                                        t!(
                                            "editor.mesh_info",
                                            vertices = layer.vertices,
                                            triangles = layer.triangles
                                        )
                                    },
                                ),
                            )),
                    ),
            )
            .child(
                div()
                    .h(px(preview::STATUS_BAR_HEIGHT))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(if self.edit_error.is_some() {
                        theme.danger
                    } else {
                        theme.muted_foreground
                    })
                    .child(div().truncate().child(status)),
            );

        #[cfg(all(debug_assertions, feature = "debug-tools"))]
        {
            div()
                .relative()
                .size_full()
                .child(content)
                .child(gpui_fps::fps_monitor(window, cx))
                .into_any_element()
        }
        #[cfg(not(all(debug_assertions, feature = "debug-tools")))]
        {
            content.into_any_element()
        }
    }
}
