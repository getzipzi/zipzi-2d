mod linked;
mod track;
use linked::linked_pad;
use track::parameter_track;

use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, ParameterValue, ParameterVisual};
use gpui_kit::assets::IconName;
use gpui_kit::base::{
    ElementExt as _, Slider as BaseSlider, SliderIndicator, SliderThumb, SliderTrack,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    input::{Input, InputEvent, InputState},
    slider::{SliderEvent, SliderState, SliderValue},
};
use gpui_kit::{prelude::*, *};
use std::collections::HashMap;

struct ParameterRow {
    parameter: ParameterValue,
    visual: ParameterVisual,
    slider: Entity<SliderState>,
    input: Entity<InputState>,
    input_dirty: bool,
    view: Entity<ParameterRowView>,
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
    visual_revision: Option<u64>,
    visual_selection: Option<String>,
    linked: Option<(String, String)>,
    linked_bounds: Bounds<Pixels>,
    document_links: HashMap<DocumentId, (String, String)>,
    rows: Vec<ParameterRow>,
    scroll: UniformListScrollHandle,
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
            visual_revision: None,
            visual_selection: None,
            linked: None,
            linked_bounds: Bounds::default(),
            document_links: HashMap::new(),
            rows: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            subscriptions: Vec::new(),
        })
    }

    pub(crate) fn benchmark_slider_bounds(&self, cx: &App) -> Option<Bounds<Pixels>> {
        self.rows.iter().find_map(|row| {
            let bounds = row.slider.read(cx).bounds();
            (row.parameter.minimum < row.parameter.maximum && bounds.size.width > px(0.))
                .then_some(bounds)
        })
    }

    fn sync_with_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let (document_id, parameters, revision, selection) = {
            let owner = owner.read(cx);
            if owner.editor.is_busy() && self.document_id == owner.editor.active_id() {
                return;
            }
            let doc = owner.editor.document();
            (
                owner.editor.active_id(),
                doc.map_or_else(Vec::new, |doc| doc.parameters()),
                doc.map(|doc| doc.parameter_model_revision()),
                doc.and_then(|doc| doc.selected_id().map(str::to_owned)),
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
        let visuals_changed = structure_changed
            || self.visual_revision != revision
            || self.visual_selection != selection;
        let visuals = visuals_changed.then(|| {
            owner
                .read(cx)
                .editor
                .document()
                .map_or_else(HashMap::new, |doc| doc.parameter_visuals())
        });
        self.visual_revision = revision;
        self.visual_selection = selection;

        if structure_changed {
            if let Some(id) = self.document_id {
                if let Some(link) = &self.linked {
                    self.document_links.insert(id, link.clone());
                } else {
                    self.document_links.remove(&id);
                }
            }
            let open_ids: Vec<_> = owner
                .read(cx)
                .editor
                .documents()
                .iter()
                .map(|d| d.id)
                .collect();
            self.document_links.retain(|id, _| open_ids.contains(id));
            self.document_id = document_id;
            self.linked = document_id
                .and_then(|id| self.document_links.get(&id).cloned())
                .filter(|(a, b)| {
                    parameters.iter().any(|p| &p.id == a) && parameters.iter().any(|p| &p.id == b)
                });
            self.linked_bounds = Bounds::default();
            self.scroll = UniformListScrollHandle::new();
            self.subscriptions.clear();
            let panel = cx.weak_entity();
            self.rows = parameters
                .into_iter()
                .map(|parameter| {
                    let visual = visuals
                        .as_ref()
                        .and_then(|visuals| visuals.get(&parameter.id))
                        .cloned()
                        .unwrap_or_else(|| ParameterVisual {
                            kind: Default::default(),
                            repeat: false,
                            keys: Vec::new(),
                        });
                    let slider = cx.new(|_| {
                        let span = parameter.maximum - parameter.minimum;
                        SliderState::new()
                            .min(parameter.minimum)
                            .max(parameter.maximum)
                            .step(
                                10_f32
                                    .powi(-(parameter.decimal_places as i32))
                                    .min(span)
                                    .max(span / 1000.)
                                    .max(f32::EPSILON),
                            )
                            .default_value(parameter.value)
                    });
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .default_value(format_value(parameter.value, parameter.decimal_places))
                    });
                    let view = cx.new(|cx| ParameterRowView {
                        panel: panel.clone(),
                        document_id,
                        parameter: parameter.clone(),
                        visual: visual.clone(),
                        slider: slider.clone(),
                        input: input.clone(),
                        disabled: false,
                        link_available: false,
                        link_active: false,
                        _observe: cx.observe(&slider, |_, _, cx| cx.notify()),
                    });
                    ParameterRow {
                        view,
                        parameter,
                        visual,
                        slider,
                        input,
                        input_dirty: false,
                    }
                })
                .collect();
            for row in &self.rows {
                let parameter_id = row.parameter.id.clone();
                let slider = row.slider.clone();
                let subscription = cx.subscribe_in(
                    &slider,
                    window,
                    move |this: &mut Self, source, event: &SliderEvent, window, cx| {
                        let Some(document_id) = document_id else {
                            return;
                        };
                        if this.document_id != Some(document_id)
                            || !this.rows.iter().any(|row| {
                                row.parameter.id == parameter_id && &row.slider == source
                            })
                        {
                            return;
                        }
                        let SliderValue::Single(value) = (match event {
                            SliderEvent::Change(value) | SliderEvent::Release(value) => value,
                        }) else {
                            return;
                        };
                        let Some(row) = this.rows.iter().find(|r| r.parameter.id == parameter_id)
                        else {
                            return;
                        };
                        // Kit rounds around zero and can overshoot a non-aligned range endpoint.
                        let value = value.clamp(row.parameter.minimum, row.parameter.maximum);
                        this.apply_values(
                            document_id,
                            &[(parameter_id.clone(), value)],
                            window,
                            cx,
                        );
                    },
                );
                self.subscriptions.push(subscription);
                let input = row.input.clone();
                let parameter_id = row.parameter.id.clone();
                let input_subscription =
                    cx.subscribe_in(
                        &input,
                        window,
                        move |this: &mut Self, source, event: &InputEvent, window, cx| {
                            let Some(document_id) = document_id else {
                                return;
                            };
                            if this.document_id != Some(document_id) {
                                return;
                            }
                            let Some(row) = this.rows.iter_mut().find(|row| {
                                row.parameter.id == parameter_id && &row.input == source
                            }) else {
                                return;
                            };
                            match event {
                                InputEvent::Change => row.input_dirty = true,
                                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                                    this.commit_input(document_id, &parameter_id, window, cx)
                                }
                                InputEvent::Focus => {}
                            }
                        },
                    );
                self.subscriptions.push(input_subscription);
            }
        } else {
            for (row, parameter) in self.rows.iter_mut().zip(parameters) {
                {
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
                if !row.input_dirty && !row.input.read(cx).focus_handle(cx).is_focused(window) {
                    let desired = format_value(parameter.value, parameter.decimal_places);
                    if row.input.read(cx).value().as_ref() != desired {
                        row.input.update(cx, |input, cx| {
                            input.set_value(desired, window, cx);
                        });
                    }
                }
                if let Some(visual) = visuals.as_ref().and_then(|v| v.get(&parameter.id)) {
                    if row.visual != *visual {
                        row.visual = visual.clone();
                        row.view.update(cx, |view, cx| {
                            view.visual = visual.clone();
                            cx.notify();
                        });
                    }
                }
                row.view.update(cx, |view, cx| {
                    if view.parameter != parameter {
                        view.parameter = parameter.clone();
                        cx.notify();
                    }
                });
                row.parameter = parameter;
            }
        }
    }

    fn cancel_input(
        &mut self,
        document_id: DocumentId,
        parameter_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) {
            return;
        }
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|r| r.parameter.id == parameter_id)
        {
            row.input_dirty = false;
            row.input.update(cx, |input, cx| {
                input.set_value(
                    format_value(row.parameter.value, row.parameter.decimal_places),
                    window,
                    cx,
                )
            });
        }
    }

    fn commit_input(
        &mut self,
        document_id: DocumentId,
        parameter_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) {
            return;
        }
        let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.parameter.id == parameter_id)
        else {
            return;
        };
        if row.input.read(cx).value().as_ref()
            == format_value(row.parameter.value, row.parameter.decimal_places)
        {
            row.input_dirty = false;
            return;
        }
        let value = row.input.read(cx).value().trim().parse::<f32>().ok();
        let valid = value.filter(|value| {
            value.is_finite() && (row.parameter.minimum..=row.parameter.maximum).contains(value)
        });
        let original = row.parameter.value;
        let places = row.parameter.decimal_places;
        let input = row.input.clone();
        if let Some(value) = valid.filter(|value| *value != original) {
            self.apply_values(document_id, &[(parameter_id.to_owned(), value)], window, cx);
        }
        let displayed = self
            .rows
            .iter()
            .find(|row| row.parameter.id == parameter_id)
            .map_or(original, |row| row.parameter.value);
        input.update(cx, |input, cx| {
            input.set_value(format_value(displayed, places), window, cx);
        });
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|r| r.parameter.id == parameter_id)
        {
            row.input_dirty = false;
        }
    }

    fn apply_values(
        &mut self,
        document_id: DocumentId,
        values: &[(String, f32)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let changed = values.iter().any(|(id, value)| {
            self.rows
                .iter()
                .any(|r| &r.parameter.id == id && r.parameter.value != *value)
        });
        let applied = owner.update(cx, |owner, cx| {
            if owner.modal
                || owner.editor.is_busy()
                || owner.editor.active_id() != Some(document_id)
            {
                return false;
            }
            if !changed {
                return true;
            }
            let result = owner.editor.execute_for(
                document_id,
                EditorCommand::SetPreviewParameters {
                    values: values.to_vec(),
                },
            );
            let applied = result.is_ok();
            owner.complete_preview_edit(result, cx);
            applied
        });
        if applied {
            for (id, value) in values {
                if let Some(row) = self.rows.iter_mut().find(|row| row.parameter.id == *id) {
                    row.parameter.value = *value;
                    let view = row.view.clone();
                    let applied_value = *value;
                    cx.defer(move |cx| {
                        view.update(cx, |view, cx| {
                            view.parameter.value = applied_value;
                            cx.notify();
                        })
                    });
                    if !row.input_dirty {
                        row.input.update(cx, |input, cx| {
                            input.set_value(
                                format_value(*value, row.parameter.decimal_places),
                                window,
                                cx,
                            )
                        });
                    }
                    row.slider.update(cx, |slider, cx| {
                        slider.set_value(*value, window, cx);
                    });
                }
            }
            if self.linked.is_some() && changed {
                cx.notify();
            }
        }
    }

    fn toggle_link(&mut self, document_id: DocumentId, upper: &str, cx: &mut Context<Self>) {
        if self.document_id != Some(document_id) {
            return;
        }
        let Some(index) = self.rows.iter().position(|row| row.parameter.id == upper) else {
            return;
        };
        let Some(lower) = self.rows.get(index + 1).map(|row| row.parameter.id.clone()) else {
            return;
        };
        if self
            .linked
            .as_ref()
            .is_some_and(|(a, b)| a == upper && b == &lower)
        {
            self.linked = None;
        } else {
            self.linked = Some((upper.to_owned(), lower));
        }
        cx.notify();
    }

    fn move_linked(&mut self, point: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((x_id, y_id)) = self.linked.clone() else {
            return;
        };
        let bounds = self.linked_bounds;
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return;
        }
        let value_for = |id: &str, fraction: f32, rows: &[ParameterRow]| {
            rows.iter().find(|row| row.parameter.id == id).map(|row| {
                let p = &row.parameter;
                let step = 10_f32
                    .powi(-(p.decimal_places as i32))
                    .max((p.maximum - p.minimum) / 1000.);
                if fraction <= 0. {
                    return p.minimum;
                }
                if fraction >= 1. {
                    return p.maximum;
                }
                let raw = p.minimum + (p.maximum - p.minimum) * fraction;
                (raw / step)
                    .round()
                    .mul_add(step, 0.)
                    .clamp(p.minimum, p.maximum)
            })
        };
        let x_fraction = (point.x - bounds.left()).as_f32() / bounds.size.width.as_f32();
        let y_fraction = (bounds.bottom() - point.y).as_f32() / bounds.size.height.as_f32();
        let (Some(x), Some(y)) = (
            value_for(&x_id, x_fraction, &self.rows),
            value_for(&y_id, y_fraction, &self.rows),
        ) else {
            return;
        };
        let changed = self.rows.iter().any(|row| {
            (row.parameter.id == x_id && row.parameter.value != x)
                || (row.parameter.id == y_id && row.parameter.value != y)
        });
        if changed {
            if let Some(id) = self.document_id {
                self.apply_values(id, &[(x_id, x), (y_id, y)], window, cx);
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
        self.apply_values(
            document_id,
            &[(parameter_id.to_owned(), default_value)],
            window,
            cx,
        );
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
        // Disabled state changes invalidate the row cache; ordinary drags do not.
        for row in &self.rows {
            if row.view.read(cx).disabled != disabled {
                row.view.update(cx, |view, cx| {
                    view.disabled = disabled;
                    cx.notify();
                });
            }
        }
        let row_count = self.rows.len();
        let panel = cx.entity();
        for (index, row) in self.rows.iter().enumerate() {
            let available = index + 1 < row_count;
            let active = self
                .linked
                .as_ref()
                .is_some_and(|(id, _)| id == &row.parameter.id);
            if row.view.read(cx).link_available != available
                || row.view.read(cx).link_active != active
            {
                row.view.update(cx, |view, cx| {
                    view.link_available = available;
                    view.link_active = active;
                    cx.notify();
                });
            }
        }
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
            .when_some(self.linked.clone(), |this, (x, y)| {
                let x_row = self.rows.iter().find(|row| row.parameter.id == x);
                let y_row = self.rows.iter().find(|row| row.parameter.id == y);
                this.when_some(x_row.zip(y_row), |this, (x_row, y_row)| {
                    this.child(linked_pad(
                        &x_row.parameter,
                        &y_row.parameter,
                        &x_row.visual,
                        &y_row.visual,
                        &panel,
                        self.document_id.unwrap(),
                        disabled,
                        theme,
                    ))
                })
            })
            .child(if row_count == 0 {
                div()
                    .p_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("editor.no_parameters"))
                    .into_any_element()
            } else {
                // A drag repaints this panel; keep work proportional to visible rows.
                uniform_list("parameters-scroll-area", row_count, move |range, _, cx| {
                    panel.update(cx, |this, _| {
                        range
                            .map(|index| {
                                this.rows[index]
                                    .view
                                    .clone()
                                    .cached(StyleRefinement::default().w_full().h(rems(3.5)))
                            })
                            .collect()
                    })
                })
                .flex_1()
                .min_h_0()
                .track_scroll(&self.scroll)
                .into_any_element()
            })
            .into_any_element()
    }
}

// Each row owns its slider dependency so changing one thumb does not rebuild
// the other visible sliders (including their labels, tooltips and hover rings).
struct ParameterRowView {
    panel: WeakEntity<ParametersPanel>,
    document_id: Option<DocumentId>,
    parameter: ParameterValue,
    visual: ParameterVisual,
    slider: Entity<SliderState>,
    input: Entity<InputState>,
    disabled: bool,
    link_available: bool,
    link_active: bool,
    _observe: Subscription,
}

impl Render for ParameterRowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let parameter = &self.parameter;
        let slider = &self.slider;
        let disabled = self.disabled;
        let document_id = self.document_id;
        let theme = cx.theme();
        let value = match slider.read(cx).value() {
            SliderValue::Single(value) => value,
            _ => parameter.value,
        };
        let minimum_text = format_value(parameter.minimum, parameter.decimal_places);
        let maximum_text = format_value(parameter.maximum, parameter.decimal_places);
        let parameter_id = parameter.id.clone();
        let default_value = parameter.default_value;
        let link_parameter_id = parameter_id.clone();
        let apply_parameter_id = parameter_id.clone();
        div()
            .id(SharedString::from(format!("parameter-row-{parameter_id}")))
            .capture_action(
                cx.listener(|this, action: &crate::menus::RunCommand, window, cx| {
                    use editor_core::CommandId;
                    if this.input.read(cx).focus_handle(cx).is_focused(window)
                        && matches!(
                            action.id,
                            CommandId::CommitProperty | CommandId::CancelProperty
                        )
                    {
                        if let Some(id) = this.document_id {
                            let _ = this.panel.update(cx, |panel, cx| {
                                if action.id == CommandId::CancelProperty {
                                    panel.cancel_input(id, &this.parameter.id, window, cx);
                                } else {
                                    panel.commit_input(id, &this.parameter.id, window, cx);
                                }
                            });
                        }
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::input::Escape, window, cx| {
                    if this.input.read(cx).focus_handle(cx).is_focused(window) {
                        if let Some(id) = this.document_id {
                            let _ = this.panel.update(cx, |panel, cx| {
                                panel.cancel_input(id, &this.parameter.id, window, cx)
                            });
                        }
                        cx.stop_propagation();
                    }
                }),
            )
            .flex()
            .flex_col()
            .w_full()
            .h(rems(3.5))
            .min_w_0()
            .gap_0p5()
            .px_3()
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
                        Input::new(&self.input)
                            .id(SharedString::from(format!(
                                "parameter-value-{parameter_id}"
                            )))
                            .aria_label(parameter.name.clone())
                            .small()
                            .w(px(65.))
                            .disabled(disabled),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "parameter-apply-{parameter_id}"
                        )))
                        .xsmall()
                        .ghost()
                        .icon(IconName::Check)
                        .tooltip(t!("editor.parameter_apply"))
                        .disabled(disabled)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let _ = this.panel.update(cx, |panel, cx| {
                                    if let Some(id) = document_id {
                                        panel.commit_input(id, &apply_parameter_id, window, cx);
                                    }
                                });
                            },
                        )),
                    )
                    .when(self.link_available, |this| {
                        this.child(
                            Button::new(SharedString::from(format!(
                                "parameter-link-{parameter_id}"
                            )))
                            .xsmall()
                            .ghost()
                            .icon(if self.link_active {
                                IconName::Unlink
                            } else {
                                IconName::Link2
                            })
                            .tooltip(if self.link_active {
                                t!("editor.parameter_unlink")
                            } else {
                                t!("editor.parameter_link")
                            })
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let _ = this.panel.update(cx, |panel, cx| {
                                        if let Some(id) = document_id {
                                            panel.toggle_link(id, &link_parameter_id, cx);
                                        }
                                    });
                                },
                            )),
                        )
                    })
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
                                let _ = this.panel.update(cx, |panel, cx| {
                                    panel.reset_parameter(
                                        document_id,
                                        &parameter_id,
                                        default_value,
                                        window,
                                        cx,
                                    );
                                });
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
                    .child(div().flex().items_center().flex_1().min_w_0().h_6().child(
                        parameter_track(
                            slider,
                            parameter,
                            &self.visual,
                            value,
                            disabled,
                            theme,
                            self.panel.clone(),
                            document_id,
                        ),
                    ))
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
    }
}
