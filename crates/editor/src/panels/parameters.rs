use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, ParameterValue};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
};
use gpui_kit::{prelude::*, *};

struct ParameterRow {
    parameter: ParameterValue,
    slider: Entity<SliderState>,
    dragging: bool,
}

fn format_value(value: f32, places: usize) -> String {
    if value == 0. {
        return "0".into();
    }
    if places > 6 {
        return value.to_string();
    }
    let mut text = format!("{value:.places$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

pub struct ParametersPanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    document_id: Option<DocumentId>,
    rows: Vec<ParameterRow>,
    subscriptions: Vec<Subscription>,
    _observe: Subscription,
}

impl ParametersPanel {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus: cx.focus_handle(),
            _observe: cx.observe_in(
                &owner.upgrade().unwrap(),
                window,
                |this: &mut Self, _, window, cx| {
                    this.sync_with_editor(window, cx);
                    cx.notify();
                },
            ),
            owner,
            document_id: None,
            rows: Vec::new(),
            subscriptions: Vec::new(),
        })
    }

    fn sync_with_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let (document_id, parameters) = {
            let owner = owner.read(cx);
            if owner.editor.is_busy() && self.document_id == owner.editor.active_id() {
                return;
            }
            (
                owner.editor.active_id(),
                owner
                    .editor
                    .document()
                    .map_or_else(Vec::new, |doc| doc.parameters()),
            )
        };
        let structure_changed = self.document_id != document_id
            || self.rows.len() != parameters.len()
            || self.rows.iter().zip(&parameters).any(|(row, parameter)| {
                row.parameter.id != parameter.id
                    || row.parameter.minimum != parameter.minimum
                    || row.parameter.maximum != parameter.maximum
                    || row.parameter.decimal_places != parameter.decimal_places
            });

        if structure_changed {
            self.document_id = document_id;
            self.subscriptions.clear();
            self.rows = parameters
                .into_iter()
                .map(|parameter| {
                    let slider = cx.new(|_| {
                        let span = parameter.maximum - parameter.minimum;
                        SliderState::new()
                            .min(parameter.minimum)
                            .max(parameter.maximum)
                            .step(
                                10_f32
                                    .powi(-(parameter.decimal_places as i32))
                                    .min(span)
                                    .max(span / 1000.),
                            )
                            .default_value(parameter.value)
                    });
                    ParameterRow {
                        parameter,
                        slider,
                        dragging: false,
                    }
                })
                .collect();
            for row in &self.rows {
                let parameter_id = row.parameter.id.clone();
                let slider = row.slider.clone();
                let subscription = cx.subscribe_in(
                    &slider,
                    window,
                    move |this: &mut Self, _, event: &SliderEvent, window, cx| {
                        let Some(index) = this
                            .rows
                            .iter()
                            .position(|row| row.parameter.id == parameter_id)
                        else {
                            return;
                        };
                        let SliderValue::Single(value) = (match event {
                            SliderEvent::Change(value) | SliderEvent::Release(value) => value,
                        }) else {
                            return;
                        };
                        let Some(owner) = this.owner.upgrade() else {
                            return;
                        };
                        let Some(document_id) = this.document_id else {
                            return;
                        };
                        if matches!(event, SliderEvent::Release(_)) {
                            this.rows[index].dragging = false;
                        }
                        let dialog_active = window.has_active_dialog(cx);
                        let editable = {
                            let owner = owner.read(cx);
                            !owner.modal
                                && !dialog_active
                                && !owner.editor.is_busy()
                                && owner.editor.active_id() == Some(document_id)
                        };
                        if !editable {
                            return;
                        }
                        if matches!(event, SliderEvent::Change(_)) {
                            this.rows[index].dragging = true;
                        }
                        owner.update(cx, |owner, cx| {
                            let result = owner.editor.execute_for(
                                document_id,
                                EditorCommand::SetPreviewParameter {
                                    parameter_id: parameter_id.clone(),
                                    value: *value,
                                },
                            );
                            owner.complete_preview_edit(result, cx);
                        });
                        cx.notify();
                    },
                );
                self.subscriptions.push(subscription);
            }
        } else {
            for (row, parameter) in self.rows.iter_mut().zip(parameters) {
                if !row.dragging {
                    let current = match row.slider.read(cx).value() {
                        SliderValue::Single(value) => value,
                        _ => parameter.default_value,
                    };
                    if current != parameter.value {
                        row.slider.update(cx, |slider, cx| {
                            slider.set_value(parameter.value, window, cx);
                        });
                    }
                }
                row.parameter = parameter;
            }
        }
    }

    fn reset_parameter(
        &mut self,
        document_id: DocumentId,
        parameter_id: &str,
        default_value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        owner.update(cx, |owner, cx| {
            if owner.modal
                || owner.editor.is_busy()
                || owner.editor.active_id() != Some(document_id)
            {
                return;
            }
            let result = owner.editor.execute_for(
                document_id,
                EditorCommand::SetPreviewParameter {
                    parameter_id: parameter_id.to_owned(),
                    value: default_value,
                },
            );
            owner.complete_edit(result, cx);
        });
    }
}

impl EventEmitter<PanelEvent> for ParametersPanel {}
impl Focusable for ParametersPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for ParametersPanel {
    fn panel_name(&self) -> &'static str {
        "Parameters"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for ParametersPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.parameters")
    }
}

impl Render for ParametersPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let dialog_active = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let disabled = state.modal || dialog_active || state.editor.is_busy();
        let document_id = self.document_id;
        let theme = cx.theme();

        div()
            .id("parameters-panel")
            .test_support()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .track_focus(&self.focus)
            .child(if self.rows.is_empty() {
                div()
                    .p_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("editor.no_parameters"))
                    .into_any_element()
            } else {
                div()
                    .id("parameters-scroll-area")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .px_2()
                            .py_1()
                            .children(self.rows.iter().map(|row| {
                                let parameter = &row.parameter;
                                let value = match row.slider.read(cx).value() {
                                    SliderValue::Single(value) => value,
                                    _ => parameter.value,
                                };
                                let value_text = format_value(value, parameter.decimal_places);
                                let minimum_text =
                                    format_value(parameter.minimum, parameter.decimal_places);
                                let maximum_text =
                                    format_value(parameter.maximum, parameter.decimal_places);
                                let parameter_id = parameter.id.clone();
                                let default_value = parameter.default_value;
                                div()
                                    .id(SharedString::from(format!("parameter-row-{parameter_id}")))
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .px_1()
                                    .py_1()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_xs()
                                                    .text_ellipsis()
                                                    .overflow_hidden()
                                                    .child(parameter.name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_family(".AppleSystemUIFontMonospaced")
                                                    .text_color(theme.muted_foreground)
                                                    .child(value_text),
                                            )
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "parameter-reset-{parameter_id}"
                                                )))
                                                .xsmall()
                                                .ghost()
                                                .icon(IconName::RotateCcw)
                                                .tooltip(t!("editor.parameter_reset"))
                                                .disabled(disabled || value == default_value)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        let Some(document_id) = document_id else {
                                                            return;
                                                        };
                                                        this.reset_parameter(
                                                            document_id,
                                                            &parameter_id,
                                                            default_value,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .w(px(42.))
                                                    .flex_shrink_0()
                                                    .text_xs()
                                                    .text_ellipsis()
                                                    .overflow_hidden()
                                                    .text_color(theme.muted_foreground)
                                                    .child(minimum_text),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .h_6()
                                                    .child(
                                                        Slider::new(&row.slider).disabled(disabled),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .w(px(42.))
                                                    .flex_shrink_0()
                                                    .text_right()
                                                    .text_xs()
                                                    .text_ellipsis()
                                                    .overflow_hidden()
                                                    .text_color(theme.muted_foreground)
                                                    .child(maximum_text),
                                            ),
                                    )
                            })),
                    )
                    .into_any_element()
            })
            .into_any_element()
    }
}
