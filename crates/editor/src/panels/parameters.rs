mod delete_confirmation;
mod dialog_session;
mod form;
mod group_form;
mod management;
pub(crate) use management::ParameterAction;
pub(super) mod grouping;
pub(super) mod row;
pub(super) mod track;
use grouping::{ListRow, ParameterLayout};
use track::parameter_track;

use super::group_header;
use super::sorting::{self, SortItem, SortState};
use super::toolbar::PanelToolbar;
use crate::ui::slide_number_input::{SlideNumberChange, SlideNumberInput, SlideNumberOptions};
use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, ParameterValue, ParameterVisual};
use gpui_kit::assets::IconName;
use gpui_kit::base::{Slider as BaseSlider, SliderIndicator};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    input::{Input, InputEvent, InputState},
    slider::{SliderEvent, SliderState, SliderValue},
    tooltip::Tooltip,
};
use gpui_kit::{prelude::*, *};
use std::collections::{HashMap, HashSet};

// Group headers and parameter rows share the uniform list's measured height.
const PARAMETER_ROW_HEIGHT: f32 = group_header::GROUP_ROW_HEIGHT;
// All numeric fields cover their complete range over the same drag distance.
const PARAMETER_SLIDE_DISTANCE_PX: f64 = 400.;
// Keep the binding summary available while temporarily hiding it from the panel.
const SHOW_BINDING_SUMMARY: bool = false;

struct ParameterRow {
    parameter: ParameterValue,
    visual: ParameterVisual,
    slider: Entity<SliderState>,
    input: Entity<InputState>,
    number_input: Entity<SlideNumberInput>,
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

fn slide_number_options(parameter: &ParameterValue, disabled: bool) -> SlideNumberOptions {
    // Compute range proportions from the shortest authored decimal values.
    let decimal = |value: f32| value.to_string().parse::<f64>().unwrap_or(value as f64);
    SlideNumberOptions {
        value: decimal(parameter.value),
        min: decimal(parameter.minimum),
        max: decimal(parameter.maximum),
        step: 0.1,
        drag_distance: Some(PARAMETER_SLIDE_DISTANCE_PX),
        disabled,
    }
}

pub struct ParametersPanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    document_id: Option<DocumentId>,
    visual_revision: Option<u64>,
    visual_selection: Option<String>,
    show_indirect: bool,
    collapsed_groups: HashSet<Option<String>>,
    layout: ParameterLayout,
    visible_rows: Vec<ListRow>,
    rows: Vec<ParameterRow>,
    scroll: UniformListScrollHandle,
    subscriptions: Vec<Subscription>,
    slider_drag: Option<Entity<SliderState>>,
    sorting: Entity<SortState>,
    _sorting_observe: Subscription,
    _observe: Subscription,
    _activation: Subscription,
}

impl ParametersPanel {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            let sorting = SortState::new(owner.clone(), focus.clone(), window, cx);
            let sorting_observe = cx.observe(&sorting, |_, _, cx| cx.notify());
            Self {
                focus,
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
                collapsed_groups: HashSet::new(),
                layout: ParameterLayout::default(),
                visible_rows: Vec::new(),
                rows: Vec::new(),
                scroll: UniformListScrollHandle::new(),
                subscriptions: Vec::new(),
                slider_drag: None,
                sorting,
                _sorting_observe: sorting_observe,
            }
        })
    }

    pub(crate) fn benchmark_slider(&self, cx: &App) -> Option<(Bounds<Pixels>, Point<Pixels>)> {
        let viewport = self.scroll.0.borrow().base_handle.bounds();
        self.visible_rows.iter().find_map(|entry| {
            let ListRow::Parameter { index, .. } = entry else {
                return None;
            };
            let row = &self.rows[*index];
            let bounds = row.slider.read(cx).bounds();
            // Virtualized rows may retain nonzero bounds from measurement or
            // earlier paints, even when their track is outside the viewport.
            (row.parameter.minimum < row.parameter.maximum
                && bounds.size.width > px(0.)
                && viewport.contains(&bounds.center()))
            .then(|| {
                let fraction = ((row.parameter.value - row.parameter.minimum)
                    / (row.parameter.maximum - row.parameter.minimum))
                    .clamp(0., 1.);
                (
                    bounds,
                    point(
                        bounds.left() + bounds.size.width * fraction,
                        bounds.center().y,
                    ),
                )
            })
        })
    }

    #[cfg(test)]
    pub(crate) fn sorting_scroll_handle(&self) -> UniformListScrollHandle {
        self.scroll.clone()
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
        let by_id: HashMap<_, _> = self
            .rows
            .iter()
            .map(|row| (row.parameter.id.as_str(), &row.parameter))
            .collect();
        let structure_changed = self.document_id != document_id
            || self.rows.len() != parameters.len()
            || parameters.iter().any(|parameter| {
                by_id.get(parameter.id.as_str()).is_none_or(|old| {
                    old.minimum != parameter.minimum
                        || old.maximum != parameter.maximum
                        || old.decimal_places != parameter.decimal_places
                })
            });
        let order_changed = self
            .rows
            .iter()
            .zip(&parameters)
            .any(|(row, parameter)| row.parameter.id != parameter.id);
        let layout_changed = structure_changed
            || order_changed
            || self.visual_revision != revision
            || self
                .rows
                .iter()
                .zip(&parameters)
                .any(|(row, parameter)| row.parameter.group_id != parameter.group_id);
        if layout_changed {
            if self.document_id != document_id {
                self.collapsed_groups.clear();
            }
            let groups = owner
                .read(cx)
                .editor
                .document()
                .map_or_else(Vec::new, |doc| doc.parameter_groups());
            self.collapsed_groups.retain(|id| {
                id.as_ref()
                    .is_none_or(|id| groups.iter().any(|group| &group.id == id))
            });
            self.layout = ParameterLayout::new(&groups, &parameters, &t!("parameter.ungrouped"));
        }
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
                    let number_input = cx.new(|cx| {
                        SlideNumberInput::new(
                            input.clone(),
                            parameter.name.clone(),
                            slide_number_options(&parameter, false),
                            window,
                            cx,
                        )
                    });
                    let view = cx.new(|cx| ParameterRowView {
                        panel: panel.clone(),
                        document_id,
                        parameter: parameter.clone(),
                        visual: visual.clone(),
                        slider: slider.clone(),
                        input: input.clone(),
                        number_input: number_input.clone(),
                        disabled: false,
                        summary: String::new(),
                        related: false,
                        depth: 0,
                        _observe: cx.observe(&slider, |_, _, cx| cx.notify()),
                        sorting: self.sorting.clone(),
                        _sorting_observe: cx.observe(&self.sorting, |_, _, cx| cx.notify()),
                    });
                    ParameterRow {
                        view,
                        parameter,
                        visual,
                        slider,
                        input,
                        number_input,
                        input_dirty: false,
                    }
                })
                .collect();
            for row in &self.rows {
                let parameter_id = row.parameter.id.clone();
                self.subscriptions.push(cx.subscribe_in(
                    &row.number_input,
                    window,
                    move |this: &mut Self, source, event: &SlideNumberChange, window, cx| {
                        let Some(document_id) = document_id else {
                            return;
                        };
                        if this.rows.iter().any(|row| {
                            row.parameter.id == parameter_id && &row.number_input == source
                        }) {
                            this.apply_values(
                                document_id,
                                &[(parameter_id.clone(), event.0 as f32)],
                                window,
                                cx,
                            );
                        }
                    },
                ));
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
            if order_changed {
                let ranks: HashMap<_, _> = parameters
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| (parameter.id.as_str(), index))
                    .collect();
                self.rows
                    .sort_by_key(|row| ranks[row.parameter.id.as_str()]);
            }
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
        if layout_changed {
            self.refresh_visible_rows(cx);
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
    fn toolbar(&self, create_disabled: bool, cx: &mut Context<Self>) -> PanelToolbar {
        let new_label = t!("parameter.new").into_owned();
        let on_create = cx.listener(|this, _, window, cx| this.open_create_dialog(window, cx));
        let on_group = cx.listener(|this, _, window, cx| {
            this.open_management(ParameterAction::CreateGroup, window, cx)
        });
        PanelToolbar::new("parameters-panel-toolbar")
            .action(move |_| {
                Button::new("new-parameter")
                    .icon(IconName::Plus)
                    .accessibility_label(new_label.clone())
                    .tooltip(new_label)
                    .disabled(create_disabled)
                    .on_click(on_create)
            })
            .action(move |_| {
                Button::new("new-parameter-group-action")
                    .icon(IconName::Folder)
                    .accessibility_label(t!("parameter.new_group"))
                    .tooltip(t!("parameter.new_group"))
                    .disabled(create_disabled)
                    .on_click(on_group)
            })
    }

    fn refresh_visible_rows(&mut self, cx: &mut Context<Self>) {
        self.visible_rows = self.layout.visible(&self.collapsed_groups);
        for row in &self.visible_rows {
            if let ListRow::Parameter { index, depth } = row {
                let view = &self.rows[*index].view;
                if view.read(cx).depth != *depth {
                    view.update(cx, |view, cx| {
                        view.depth = *depth;
                        cx.notify();
                    });
                }
            }
        }
    }

    fn group_header(
        &self,
        id: Option<String>,
        name: String,
        depth: usize,
        count: usize,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = id.as_deref().unwrap_or("ungrouped").to_owned();
        let expanded = !self.collapsed_groups.contains(&id);
        let group = id.clone();
        let owner = self.owner.clone();
        let document_id = self.document_id;
        let panel = cx.weak_entity();
        let expand: sorting::ExpandTarget = std::rc::Rc::new(move |item, _, cx| {
            if let SortItem::Group(group) = item {
                _ = panel.update(cx, |panel, cx| {
                    if panel.collapsed_groups.remove(group) {
                        panel.refresh_visible_rows(cx);
                        cx.notify();
                    }
                });
            }
        });
        let mut menu_actions = vec![ParameterAction::Create(id.clone())];
        if let Some(id) = &id {
            menu_actions.extend([
                ParameterAction::EditGroup(id.clone()),
                ParameterAction::DeleteGroup(id.clone()),
            ]);
        }
        let menu_panel = cx.weak_entity();
        let sort_item = SortItem::Group(id.clone());
        div()
            .id(SharedString::from(format!("parameter-group-{key}")))
            .map(|element| {
                sorting::drop_target(element, &self.sorting, sort_item.clone(), Some(expand), cx)
            })
            .when_some(document_id, |element, document| {
                sorting::drag_source(
                    element,
                    &self.sorting,
                    document,
                    sort_item,
                    name.clone(),
                    !disabled && id.is_some(),
                )
            })
            .test_support()
            .role(gpui::Role::Group)
            .aria_label(name.clone())
            .aria_expanded(expanded)
            .map(|element| group_header::row_style(element, depth, cx))
            .child(
                group_header::toggle(
                    SharedString::from(format!("parameter-group-toggle-{key}")),
                    SharedString::from(format!("parameter-group-count-{key}")),
                    name,
                    count,
                    expanded,
                    depth,
                    cx,
                )
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !this.slider_interaction_allowed(window, cx) {
                        return;
                    }
                    this.finish_slider_drag(cx);
                    if !this.collapsed_groups.remove(&group) {
                        this.collapsed_groups.insert(group.clone());
                    }
                    this.refresh_visible_rows(cx);
                    cx.notify();
                })),
            )
            .when_some(id, |header, id| {
                header.child(
                    Button::new(SharedString::from(format!("parameter-group-inspect-{id}")))
                        .icon(IconName::Info)
                        .ghost()
                        .small()
                        .disabled(disabled)
                        .accessibility_label(t!("editor.parameter_group_inspect"))
                        .tooltip(t!("editor.parameter_group_inspect"))
                        .on_click(move |_, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            let (Some(owner), Some(document_id)) = (owner.upgrade(), document_id)
                            else {
                                return;
                            };
                            owner.update(cx, |owner, cx| {
                                if owner.modal
                                    || owner.editor.is_busy()
                                    || owner.editor.active_id() != Some(document_id)
                                {
                                    return;
                                }
                                let result = owner.editor.execute_for(
                                    document_id,
                                    EditorCommand::Navigate(editor_core::ObjectRef::new(
                                        editor_core::ObjectKind::ParameterGroup,
                                        &id,
                                    )),
                                );
                                owner.complete_edit(result, cx);
                            });
                        }),
                )
            })
            .map(|element| management::row_menu(element, menu_panel, document_id, menu_actions))
            .into_any_element()
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
    fn inner_padding(&self, _: &App) -> bool {
        false
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.parameters")
    }
}

impl crate::docking::header::PanelHeader for ParametersPanel {
    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let dialog = window.has_active_dialog(cx);
        let disabled = self.owner.upgrade().is_none_or(|owner| {
            let state = owner.read(cx);
            state.modal
                || dialog
                || state.editor.is_busy()
                || !state.editor.command_available(
                    state
                        .editor
                        .command(editor_core::CommandId::CreateParameter),
                )
        });
        self.toolbar(disabled, cx).into_any_element()
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
        self.sorting.update(cx, |state, cx| {
            state.reconcile(self.document_id, !disabled, window, cx)
        });
        let row_count = self.visible_rows.len();
        let panel = cx.entity();
        let theme = cx.theme().clone();

        sorting::panel_actions(div().id("parameters-panel"), &self.sorting)
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
            .when(SHOW_BINDING_SUMMARY, |panel| {
                panel.child(self.binding_summary(cx))
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
                sorting::scroll_area(
                    uniform_list("parameters-scroll-area", row_count, move |range, _, cx| {
                        panel.update(cx, |this, cx| {
                            range
                                .map(|index| match this.visible_rows[index].clone() {
                                    ListRow::Parameter { index, .. } => this.rows[index]
                                        .view
                                        .clone()
                                        .cached(
                                            StyleRefinement::default()
                                                .w_full()
                                                .h(px(PARAMETER_ROW_HEIGHT)),
                                        )
                                        .into_any_element(),
                                    ListRow::Group {
                                        id,
                                        name,
                                        depth,
                                        count,
                                    } => this.group_header(id, name, depth, count, disabled, cx),
                                })
                                .collect()
                        })
                    }),
                    &self.sorting,
                    &self.scroll,
                    row_count,
                )
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
    number_input: Entity<SlideNumberInput>,
    disabled: bool,
    summary: String,
    related: bool,
    depth: usize,
    _observe: Subscription,
    sorting: Entity<SortState>,
    _sorting_observe: Subscription,
}

impl Render for ParameterRowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let parameter = &self.parameter;
        let slider = &self.slider;
        let disabled = self.disabled;
        let document_id = self.document_id;
        self.number_input.update(cx, |input, cx| {
            input.configure(
                parameter.name.clone(),
                slide_number_options(parameter, disabled),
                cx,
            );
        });
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
            .test_support()
            .map(|element| {
                sorting::drop_target(
                    element,
                    &self.sorting,
                    SortItem::Parameter(parameter_id.clone()),
                    None,
                    cx,
                )
            })
            .capture_action(
                cx.listener(|this, action: &crate::actions::RunCommand, window, cx| {
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
            .map(|element| row::style(element, cx))
            .child(
                row::name(self.depth)
                    .id(SharedString::from(format!("parameter-name-{parameter_id}")))
                    .when_some(document_id, |element, document| {
                        sorting::drag_source(
                            element,
                            &self.sorting,
                            document,
                            SortItem::Parameter(parameter_id.clone()),
                            parameter.name.clone(),
                            !disabled,
                        )
                    })
                    .test_support()
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
                    .flex()
                    .items_center()
                    .h_full()
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .child(parameter.name.clone()),
                    ),
            )
            .child(row::track().child(parameter_track(
                slider,
                parameter,
                &self.visual,
                value,
                disabled,
                theme,
                self.panel.clone(),
                document_id,
            )))
            .child(
                row::value()
                    .id(SharedString::from(format!(
                        "parameter-value-{parameter_id}"
                    )))
                    .test_support()
                    .aria_label(parameter.name.clone())
                    .child(self.number_input.clone()),
            )
            .map(|element| {
                management::row_menu(
                    element,
                    self.panel.clone(),
                    document_id,
                    vec![
                        ParameterAction::Edit(parameter_id.clone()),
                        ParameterAction::Delete(parameter_id),
                    ],
                )
            })
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
