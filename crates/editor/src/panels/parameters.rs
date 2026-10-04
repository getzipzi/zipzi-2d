mod create;
mod track;
use track::parameter_track;

use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, ParameterValue, ParameterVisual};
use gpui_kit::base::{NumberInput, Slider as BaseSlider, SliderIndicator};
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::Button,
    dock::{BasePanel, Panel, PanelEvent},
    input::{Input, InputEvent, InputState},
    slider::{SliderEvent, SliderState, SliderValue},
    tooltip::Tooltip,
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

fn format_value(value: f32) -> String {
    // Truncate the shortest decimal representation, avoiding binary float
    // artifacts (for example, 0.7 must stay 0.7). Preview values keep their precision.
    let text = value.to_string();
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, "0"));
    let decimal = fraction.chars().next().unwrap_or('0');
    if whole == "-0" && decimal == '0' {
        "0.0".into()
    } else {
        format!("{whole}.{decimal}")
    }
}

pub struct ParametersPanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    document_id: Option<DocumentId>,
    visual_revision: Option<u64>,
    visual_selection: Option<String>,
    show_indirect: bool,
    show_groups: bool,
    rows: Vec<ParameterRow>,
    scroll: UniformListScrollHandle,
    subscriptions: Vec<Subscription>,
    slider_drag: Option<Entity<SliderState>>,
    _observe: Subscription,
    _activation: Subscription,
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
            _activation: cx.observe_window_activation(window, |this: &mut Self, window, cx| {
                if !window.is_window_active() {
                    this.finish_slider_drag(cx);
                }
            }),
            owner,
            document_id: None,
            visual_revision: None,
            visual_selection: None,
            show_indirect: false,
            show_groups: false,
            rows: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            subscriptions: Vec::new(),
            slider_drag: None,
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
                doc.map(|doc| {
                    format!(
                        "{:?}/{:?}",
                        doc.context().selections,
                        doc.context().active_binding
                    )
                }),
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
            self.finish_slider_drag(cx);
            self.document_id = document_id;
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
                            .step(0.1)
                            .default_value(format_value(parameter.value))
                    });
                    let view = cx.new(|cx| ParameterRowView {
                        panel: panel.clone(),
                        document_id,
                        parameter: parameter.clone(),
                        visual: visual.clone(),
                        slider: slider.clone(),
                        input: input.clone(),
                        disabled: false,
                        summary: String::new(),
                        related: false,
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
                    let desired = format_value(parameter.value);
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
        if visuals_changed {
            if let Some(doc) = owner.read(cx).editor.document() {
                let index = doc.object_index();
                let selection = doc.context().selections.clone();
                for row in &self.rows {
                    let bindings = index.parameter(&row.parameter.id);
                    let summary = format!(
                        "· {} {} · {} {} · B{}",
                        t!("editor.direct_bindings"),
                        bindings.direct_count(&index),
                        t!("editor.indirect_influences"),
                        bindings.indirect_count(),
                        bindings.blend.len()
                    );
                    let related = selection.iter().any(|r| {
                        bindings
                            .regular
                            .iter()
                            .chain(&bindings.blend)
                            .any(|reference| {
                                index.binding(reference).is_some_and(|b| &b.target == r)
                            })
                            || bindings.indirect.iter().any(|path| &path.target == r)
                    });
                    row.view.update(cx, |v, cx| {
                        v.related = related;
                        v.summary = summary;
                        cx.notify();
                    });
                }
            }
        }
    }

    fn begin_slider_drag(
        &mut self,
        document_id: DocumentId,
        slider: &Entity<SliderState>,
        position: Point<Pixels>,
        move_thumb: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id)
            || !self.rows.iter().any(|row| &row.slider == slider)
            || !self.slider_interaction_allowed(window, cx)
            || slider.read(cx).bounds().size.width <= px(0.)
        {
            return;
        }
        self.finish_slider_drag(cx);
        self.slider_drag = Some(slider.clone());
        if move_thumb {
            slider.update(cx, |slider, cx| {
                slider.update_value_by_position(Axis::Horizontal, position, false, window, cx);
            });
        }
    }

    fn slider_interaction_allowed(&self, window: &mut Window, cx: &mut App) -> bool {
        if window.has_active_dialog(cx) {
            return false;
        }
        self.owner.upgrade().is_some_and(|owner| {
            let owner = owner.read(cx);
            !owner.modal && !owner.editor.is_busy() && owner.editor.active_id() == self.document_id
        })
    }

    fn move_slider_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(slider) = self.slider_drag.clone() else {
            return false;
        };
        if event.pressed_button != Some(MouseButton::Left)
            || !self.slider_interaction_allowed(window, cx)
        {
            self.finish_slider_drag(cx);
            return false;
        }
        slider.update(cx, |slider, cx| {
            slider.update_value_by_position(Axis::Horizontal, event.position, false, window, cx);
        });
        true
    }

    fn finish_slider_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(slider) = self.slider_drag.take() else {
            return false;
        };
        slider.update(cx, |slider, cx| slider.handle_release(cx));
        true
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
                input.set_value(format_value(row.parameter.value), window, cx)
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
        // Focusing and blurring a truncated display must not quantize the preview.
        if !row.input_dirty {
            return;
        }
        let value = row.input.read(cx).value().trim().parse::<f32>().ok();
        let valid = value.filter(|value| {
            value.is_finite() && (row.parameter.minimum..=row.parameter.maximum).contains(value)
        });
        let original = row.parameter.value;
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
            input.set_value(format_value(displayed), window, cx);
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
                            input.set_value(format_value(*value), window, cx)
                        });
                    }
                    row.slider.update(cx, |slider, cx| {
                        slider.set_value(*value, window, cx);
                    });
                }
            }
        }
    }

    fn focus_parameter(
        &mut self,
        document_id: DocumentId,
        parameter: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |owner, cx| {
                if owner.modal
                    || owner.editor.is_busy()
                    || owner.editor.active_id() != Some(document_id)
                {
                    return;
                }
                let result = owner
                    .editor
                    .execute_for(document_id, EditorCommand::FocusParameter(parameter.into()));
                owner.complete_edit(result, cx);
            });
        }
    }
    fn group_browser(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let (Some(doc), Some(id)) = (state.editor.document(), state.editor.active_id()) else {
            return div().into_any_element();
        };
        let index = doc.object_index();
        let groups: Vec<_> = index
            .objects
            .iter()
            .filter(|i| i.reference.kind == editor_core::ObjectKind::ParameterGroup)
            .collect();
        let mut body = div()
            .flex_shrink_0()
            .text_xs()
            .px_2()
            .py_1()
            .child(
                div()
                    .id("parameter-groups-toggle")
                    .cursor_pointer()
                    .child(format!(
                        "{} · {}",
                        t!("editor.parameter_groups"),
                        groups.len()
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_groups = !this.show_groups;
                        cx.notify();
                    })),
            )
            .child(div().text_color(cx.theme().muted_foreground).child(
                if doc.context().active_binding.is_some() {
                    t!("editor.binding_key_marks")
                } else {
                    t!("editor.aggregate_key_marks")
                },
            ));
        if self.show_groups {
            let mut list = div()
                .id("parameter-groups-list")
                .max_h(px(120.))
                .overflow_y_scroll();
            for group in groups {
                let r = group.reference.clone();
                let owner = self.owner.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("parameter-group-{}", r.id)))
                        .py_1()
                        .cursor_pointer()
                        .child(index.name(&r).to_owned())
                        .on_click(cx.listener(move |_, _, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |owner, cx| {
                                    if owner.modal
                                        || owner.editor.is_busy()
                                        || owner.editor.active_id() != Some(id)
                                    {
                                        return;
                                    }
                                    let result = owner
                                        .editor
                                        .execute_for(id, EditorCommand::Navigate(r.clone()));
                                    owner.complete_edit(result, cx);
                                });
                            }
                        })),
                );
            }
            body = body.child(list);
        }
        body.into_any_element()
    }
    fn binding_summary(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let (Some(doc), Some(id)) = (state.editor.document(), state.editor.active_id()) else {
            return div().into_any_element();
        };
        let Some(focus) = doc.context().parameter_focus.as_ref() else {
            return div().into_any_element();
        };
        let index = doc.object_index();
        let bindings = index.parameter(&focus.id);
        let mut body = div()
            .id("parameter-binding-summary")
            .max_h(px(150.))
            .overflow_y_scroll()
            .flex_shrink_0()
            .text_xs()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(format!(
                "{} · {} {} · {} {} · BlendShape {}",
                index.name(focus),
                t!("editor.direct_bindings"),
                bindings.direct_count(&index),
                t!("editor.indirect_influences"),
                bindings.indirect_count(),
                bindings.blend.len()
            ));
        for reference in bindings.regular.iter().chain(&bindings.blend) {
            if let Some(b) = index.binding(reference) {
                let label = format!(
                    "{} · {:?} · {} · {}",
                    index.name(&b.target),
                    b.target.kind,
                    b.axes
                        .iter()
                        .map(|a| index
                            .name(&editor_core::ObjectRef::new(
                                editor_core::ObjectKind::Parameter,
                                &a.parameter_id
                            ))
                            .to_owned())
                        .collect::<Vec<_>>()
                        .join(" × "),
                    b.layout()
                );
                let reference = reference.clone();
                let owner = self.owner.clone();
                body = body.child(
                    div()
                        .id(SharedString::from(format!(
                            "parameter-binding-{:?}-{}",
                            reference.kind, reference.id
                        )))
                        .test_support()
                        .py_1()
                        .cursor_pointer()
                        .hover(|s| s.bg(cx.theme().muted))
                        .child(label)
                        .on_click(cx.listener(move |_, _, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |owner, cx| {
                                    if owner.modal
                                        || owner.editor.is_busy()
                                        || owner.editor.active_id() != Some(id)
                                    {
                                        return;
                                    }
                                    let result = owner.editor.execute_for(
                                        id,
                                        EditorCommand::OpenBinding(reference.clone()),
                                    );
                                    if result.is_ok() {
                                        owner.reveal_keyforms(window, cx);
                                    }
                                    owner.complete_edit(result, cx);
                                });
                            }
                        })),
                );
            }
        }
        body = body.child(
            div()
                .id("parameter-indirect-toggle")
                .py_1()
                .cursor_pointer()
                .child(t!("editor.indirect_influences"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_indirect = !this.show_indirect;
                    cx.notify();
                })),
        );
        if self.show_indirect {
            for path in &bindings.indirect {
                let reference = path.target.clone();
                let label = path
                    .path
                    .iter()
                    .map(|r| index.name(r).to_owned())
                    .collect::<Vec<_>>()
                    .join(" → ");
                let owner = self.owner.clone();
                body = body.child(
                    div()
                        .id(SharedString::from(format!("indirect-path-{label}")))
                        .py_1()
                        .cursor_pointer()
                        .child(label)
                        .on_click(cx.listener(move |_, _, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |owner, cx| {
                                    if owner.modal
                                        || owner.editor.is_busy()
                                        || owner.editor.active_id() != Some(id)
                                    {
                                        return;
                                    }
                                    let result = owner.editor.execute_for(
                                        id,
                                        EditorCommand::Navigate(reference.clone()),
                                    );
                                    owner.complete_edit(result, cx);
                                });
                            }
                        })),
                );
            }
        }
        body.into_any_element()
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
        let create_disabled = disabled
            || !state.editor.command_available(
                state
                    .editor
                    .command(editor_core::CommandId::CreateParameter),
            );
        if disabled {
            self.finish_slider_drag(cx);
        }
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
        let theme = cx.theme().clone();

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
            .child(
                canvas(|_, _, _| (), {
                    let panel = panel.downgrade();
                    move |_, _, window, _| {
                        let moving = panel.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture
                                && moving
                                    .update(cx, |panel, cx| {
                                        panel.move_slider_drag(event, window, cx)
                                    })
                                    .unwrap_or(false)
                            {
                                cx.stop_propagation();
                            }
                        });
                        let releasing = panel.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && event.button == MouseButton::Left
                                && releasing
                                    .update(cx, |panel, cx| panel.finish_slider_drag(cx))
                                    .unwrap_or(false)
                            {
                                cx.stop_propagation();
                            }
                        });
                    }
                })
                .absolute()
                .size_full(),
            )
            .child(
                div().px_2().py_1().flex_shrink_0().child(
                    Button::new("new-parameter")
                        .label(format!("＋ {}", t!("parameter.new")))
                        .disabled(create_disabled)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_create_dialog(window, cx)),
                        ),
                ),
            )
            .child(self.group_browser(cx))
            .child(self.binding_summary(cx))
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
                                    .cached(StyleRefinement::default().w_full().h(rems(2.)))
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
    summary: String,
    related: bool,
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
        let parameter_id = parameter.id.clone();
        let focus_parameter_id = parameter_id.clone();
        let tooltip = format!(
            "{} {}\n{} – {}",
            parameter.name, self.summary, parameter.minimum, parameter.maximum
        );
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
            .items_center()
            .w_full()
            .h(rems(2.))
            .min_w_0()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .w(relative(0.3))
                    .max_w(px(160.))
                    .flex_shrink_0()
                    .min_w_0()
                    .text_xs()
                    .text_ellipsis()
                    .overflow_hidden()
                    .id(SharedString::from(format!("parameter-name-{parameter_id}")))
                    .test_support()
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let _ = this.panel.update(cx, |panel, cx| {
                            if let Some(id) = document_id {
                                panel.focus_parameter(id, &focus_parameter_id, window, cx);
                            }
                        });
                    }))
                    .text_color(if self.related {
                        theme.primary
                    } else {
                        theme.foreground
                    })
                    .child(parameter.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .h_6()
                    .child(parameter_track(
                        slider,
                        parameter,
                        &self.visual,
                        value,
                        disabled,
                        theme,
                        self.panel.clone(),
                        document_id,
                    )),
            )
            .child(
                div()
                    .id(SharedString::from(format!(
                        "parameter-value-{parameter_id}"
                    )))
                    .test_support()
                    .aria_label(parameter.name.clone())
                    .flex()
                    .w(px(48.))
                    .flex_shrink_0()
                    .child(
                        NumberInput::new(&self.input)
                            .w_full()
                            .disabled(disabled)
                            .decrement_button(|button| button.hidden())
                            .increment_button(|button| button.hidden())
                            .input(Input::new(&self.input).aria_label(parameter.name.clone())),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::format_value;

    #[test]
    fn parameter_display_truncates_to_one_decimal_without_rounding() {
        for (value, expected) in [
            (0., "0.0"),
            (-0., "0.0"),
            (30., "30.0"),
            (-30., "-30.0"),
            (0.375, "0.3"),
            (0.7, "0.7"),
            (1.59, "1.5"),
            (-1.59, "-1.5"),
            (0.099, "0.0"),
            (-0.099, "0.0"),
            (f32::MAX, "340282350000000000000000000000000000000.0"),
        ] {
            assert_eq!(format_value(value), expected, "value = {value}");
        }
    }
}
