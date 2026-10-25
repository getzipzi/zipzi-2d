mod headers;
mod interactions;
mod keyform_glyph;
mod locate;
mod management;
mod selection;
mod summary;
mod sync;
mod view;
use keyform_glyph::KeyformGlyph;
pub(crate) use management::ParameterAction;
use view::ParameterRowView;
pub(super) mod grouping;
pub(super) mod row;
pub(super) mod track;
use grouping::{ListRow, ParameterLayout};
use selection::{ParameterCursor, ParameterNavigation};
use track::parameter_track;

use super::group_header;
use super::sorting::{self, SortItem, SortState};
use super::toolbar::PanelToolbar;
use crate::ui::slide_number_input::{
    SlideNumberChange, SlideNumberInput, SlideNumberOptions, format_one_decimal,
};
use crate::{EditorView, t};
use gpui_kit::assets::IconName;
use gpui_kit::base::{Slider as BaseSlider, SliderIndicator};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState, SliderValue},
    tooltip::Tooltip,
};
use gpui_kit::{prelude::*, *};
use std::collections::{HashMap, HashSet};
use zipzi_editor_core::{DocumentId, EditorCommand, ParameterValue, ParameterVisual};

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

struct KeyformToolbarState {
    context: Option<(DocumentId, String)>,
    add_current_reason: Option<String>,
    delete_current_reason: Option<String>,
    add_endpoints_reason: Option<String>,
    add_three_reason: Option<String>,
    delete_all_reason: Option<String>,
}

impl KeyformToolbarState {
    fn unavailable(reason: String) -> Self {
        Self {
            context: None,
            add_current_reason: Some(reason.clone()),
            delete_current_reason: Some(reason.clone()),
            add_endpoints_reason: Some(reason.clone()),
            add_three_reason: Some(reason.clone()),
            delete_all_reason: Some(reason),
        }
    }
}

fn format_value(value: f32) -> String {
    format_one_decimal(value as f64)
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

fn keyform_toolbar_button(
    id: &'static str,
    label: String,
    glyph: KeyformGlyph,
    action: zipzi_editor_core::ParameterKeyformAction,
    context: Option<(DocumentId, String)>,
    reason: Option<String>,
    interaction_reason: Option<String>,
    cx: &mut Context<ParametersPanel>,
) -> Button {
    let disabled = interaction_reason.is_some() || reason.is_some();
    let tip = interaction_reason
        .or(reason)
        .unwrap_or_else(|| label.clone());
    let on_click = cx.listener(move |this, _, window, cx| {
        if let Some((document, parameter)) = &context {
            this.edit_keyforms(*document, parameter, action, window, cx);
        }
    });
    crate::ui::compact::header_button(
        Button::new(id)
            .accessibility_label(label)
            .tooltip(tip)
            .disabled(disabled)
            .on_click(on_click)
            .icon(glyph),
    )
}

pub struct ParametersPanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    document_id: Option<DocumentId>,
    visual_revision: Option<u64>,
    visual_selection: Option<String>,
    show_indirect: bool,
    only_related: bool,
    collapsed_groups: HashSet<Option<String>>,
    layout: ParameterLayout,
    visible_rows: Vec<ListRow>,
    navigation: ParameterNavigation,
    navigation_focused: bool,
    _focus_events: Vec<Subscription>,
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
                _focus_events: vec![
                    cx.on_focus(&focus, window, |this: &mut Self, _, cx| {
                        this.navigation_focused = true;
                        this.refresh_navigation_styles(cx);
                        cx.notify();
                    }),
                    cx.on_blur(&focus, window, |this: &mut Self, _, cx| {
                        this.navigation_focused = false;
                        this.refresh_navigation_styles(cx);
                        cx.notify();
                    }),
                ],
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
                only_related: false,
                collapsed_groups: HashSet::new(),
                layout: ParameterLayout::default(),
                visible_rows: Vec::new(),
                navigation: ParameterNavigation::default(),
                navigation_focused: false,
                rows: Vec::new(),
                scroll: UniformListScrollHandle::new(),
                subscriptions: Vec::new(),
                slider_drag: None,
                sorting,
                _sorting_observe: sorting_observe,
            }
        })
    }

    pub(crate) fn benchmark_slider_dragging(&self) -> bool {
        self.slider_drag.is_some()
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

    #[cfg(test)]
    pub(crate) fn sorting_plans_built(&self, cx: &App) -> usize {
        self.sorting.read(cx).plans_built
    }

    fn toolbar(
        &self,
        create_disabled: bool,
        interaction_reason: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PanelToolbar {
        let new_label = t!("parameter.new").into_owned();
        let available = |action: ParameterAction| {
            self.owner
                .upgrade()
                .and_then(|o| action.availability(&o.read(cx).editor))
        };
        let new_cap = available(ParameterAction::Create(None));
        let group_cap = available(ParameterAction::CreateGroup);
        let new_disabled = create_disabled || new_cap.as_ref().is_none_or(|c| !c.enabled);
        let group_disabled = create_disabled || group_cap.as_ref().is_none_or(|c| !c.enabled);
        let new_tip = new_cap
            .and_then(|c| c.reason)
            .unwrap_or_else(|| t!("parameter.new").into());
        let group_tip = group_cap
            .and_then(|c| c.reason)
            .unwrap_or_else(|| t!("parameter.new_group").into());

        let on_create = cx.listener(|this, _, window, cx| {
            this.open_management(ParameterAction::Create(None), window, cx)
        });
        let on_group = cx.listener(|this, _, window, cx| {
            this.open_management(ParameterAction::CreateGroup, window, cx)
        });
        let keyforms = self.keyform_toolbar_state(cx);
        let add_current = keyform_toolbar_button(
            "parameter-add-keyform",
            t!("parameter.add_keyform").into_owned(),
            KeyformGlyph::AddCurrent,
            zipzi_editor_core::ParameterKeyformAction::AddCurrent,
            keyforms.context.clone(),
            keyforms.add_current_reason,
            interaction_reason.clone(),
            cx,
        );
        let delete_current = keyform_toolbar_button(
            "parameter-delete-keyform",
            t!("parameter.delete_keyform").into_owned(),
            KeyformGlyph::DeleteCurrent,
            zipzi_editor_core::ParameterKeyformAction::DeleteCurrent,
            keyforms.context.clone(),
            keyforms.delete_current_reason,
            interaction_reason.clone(),
            cx,
        );
        let add_endpoints = keyform_toolbar_button(
            "parameter-add-two-keyforms",
            t!("parameter.add_endpoint_keyforms").into_owned(),
            KeyformGlyph::AddEndpoints,
            zipzi_editor_core::ParameterKeyformAction::AddEndpoints,
            keyforms.context.clone(),
            keyforms.add_endpoints_reason,
            interaction_reason.clone(),
            cx,
        );
        let add_three = keyform_toolbar_button(
            "parameter-add-three-keyforms",
            t!("parameter.add_endpoint_midpoint_keyforms").into_owned(),
            KeyformGlyph::AddThree,
            zipzi_editor_core::ParameterKeyformAction::AddEndpointsAndMidpoint,
            keyforms.context.clone(),
            keyforms.add_three_reason,
            interaction_reason.clone(),
            cx,
        );
        let delete_all = keyform_toolbar_button(
            "parameter-delete-all-keyforms",
            t!("parameter.delete_all_keyforms").into_owned(),
            KeyformGlyph::DeleteAll,
            zipzi_editor_core::ParameterKeyformAction::DeleteAll,
            keyforms.context,
            keyforms.delete_all_reason,
            interaction_reason,
            cx,
        );
        let add_keyforms = crate::ui::compact::group(
            "parameter-add-keyform-actions",
            t!("parameter.add_keyform_actions"),
            Axis::Horizontal,
            vec![add_current, add_endpoints, add_three],
        )
        .flex_shrink_0();
        let delete_keyforms = crate::ui::compact::group(
            "parameter-delete-keyform-actions",
            t!("parameter.delete_keyform_actions"),
            Axis::Horizontal,
            vec![delete_current, delete_all],
        )
        .flex_shrink_0();
        PanelToolbar::new("parameters-panel-toolbar")
            .action(move |_| {
                Button::new("new-parameter")
                    .icon(IconName::Plus)
                    .accessibility_label(new_label.clone())
                    .tooltip(new_tip)
                    .disabled(new_disabled)
                    .on_click(on_create)
            })
            .action(move |_| {
                crate::ui::compact::new_group_button(Button::new("new-parameter-group-action"))
                    .accessibility_label(t!("parameter.new_group"))
                    .tooltip(group_tip)
                    .disabled(group_disabled)
                    .on_click(on_group)
            })
            .control(add_keyforms)
            .control(delete_keyforms)
            .control(self.expansion_button(window, cx))
            .control(self.related_filter_button(window, cx))
    }

    fn keyform_toolbar_state(&self, cx: &App) -> KeyformToolbarState {
        let Some(document_id) = self.document_id else {
            return KeyformToolbarState::unavailable(
                t!("parameter.keyform_select_parameter").into_owned(),
            );
        };
        let Some(parameter_id) = self.navigation.inspected.as_ref().and_then(|object| {
            (object.kind == zipzi_editor_core::ObjectKind::Parameter).then(|| object.id.clone())
        }) else {
            return KeyformToolbarState::unavailable(
                t!("parameter.keyform_select_parameter").into_owned(),
            );
        };
        let Some(row) = self
            .rows
            .iter()
            .find(|row| row.parameter.id == parameter_id)
        else {
            return KeyformToolbarState::unavailable(
                t!("parameter.keyform_select_parameter").into_owned(),
            );
        };
        let Some(owner) = self.owner.upgrade() else {
            return KeyformToolbarState::unavailable(
                t!("parameter.keyform_unavailable").into_owned(),
            );
        };
        let state = owner.read(cx);
        let Some(document) = state.editor.document() else {
            return KeyformToolbarState::unavailable(
                t!("parameter.keyform_unavailable").into_owned(),
            );
        };
        let context = Some((document_id, parameter_id));
        if row.visual.kind != zipzi_core::ParameterKind::Normal {
            let mut unavailable =
                KeyformToolbarState::unavailable(t!("parameter.keyform_normal_only").into_owned());
            unavailable.context = context;
            return unavailable;
        }
        let targets: Vec<_> = document
            .context()
            .selections
            .iter()
            .filter(|target| {
                matches!(
                    target.kind,
                    zipzi_editor_core::ObjectKind::Mesh
                        | zipzi_editor_core::ObjectKind::Deformer
                        | zipzi_editor_core::ObjectKind::Part
                )
            })
            .cloned()
            .collect();
        if targets.is_empty() {
            let mut unavailable = KeyformToolbarState::unavailable(
                t!("parameter.keyform_select_target").into_owned(),
            );
            unavailable.context = context;
            return unavailable;
        }
        let value = match row.slider.read(cx).value() {
            SliderValue::Single(value) => value,
            _ => row.parameter.value,
        };
        let target_axis = |target: &zipzi_editor_core::ObjectRef| {
            document.ordinary_binding(target).and_then(|binding| {
                binding
                    .axes
                    .into_iter()
                    .find(|axis| axis.parameter_id == row.parameter.id)
            })
        };
        let target_has_key = |target: &zipzi_editor_core::ObjectRef, value: f32| {
            target_axis(target).is_some_and(|axis| axis.keys.contains(&value))
        };
        let covered_targets = targets
            .iter()
            .filter(|target| target_has_key(target, value))
            .count();
        let endpoints = [row.parameter.minimum, row.parameter.maximum];
        let midpoint = row.parameter.minimum + (row.parameter.maximum - row.parameter.minimum) / 2.;
        let all_have_endpoints = targets
            .iter()
            .all(|target| endpoints.iter().all(|&value| target_has_key(target, value)));
        let all_have_three = targets.iter().all(|target| {
            endpoints
                .iter()
                .copied()
                .chain(std::iter::once(midpoint))
                .all(|value| target_has_key(target, value))
        });
        let any_keyforms = targets.iter().any(|target| target_axis(target).is_some());
        KeyformToolbarState {
            context,
            add_current_reason: (covered_targets == targets.len())
                .then(|| t!("parameter.keyform_add_exists").into_owned()),
            delete_current_reason: (covered_targets == 0)
                .then(|| t!("parameter.keyform_delete_missing").into_owned()),
            add_endpoints_reason: all_have_endpoints
                .then(|| t!("parameter.keyform_preset_exists").into_owned()),
            add_three_reason: all_have_three
                .then(|| t!("parameter.keyform_preset_exists").into_owned()),
            delete_all_reason: (!any_keyforms)
                .then(|| t!("parameter.keyform_delete_all_missing").into_owned()),
        }
    }

    fn refresh_visible_rows(&mut self, cx: &mut Context<Self>) {
        let related = self.only_related.then(|| {
            self.rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    let view = row.view.read(cx);
                    (view.directly_related || view.indirectly_related).then_some(index)
                })
                .collect()
        });
        self.visible_rows =
            grouping::visible_rows(&self.layout, &self.collapsed_groups, related.as_ref());
        self.reconcile_navigation(cx);
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
        let (interaction_reason, create_disabled) = self.owner.upgrade().map_or(
            (Some(t!("parameter.keyform_unavailable").into_owned()), true),
            |owner| {
                let state = owner.read(cx);
                let interaction_disabled = state.modal || dialog || state.editor.is_busy();
                let interaction_reason = interaction_disabled
                    .then(|| t!("parameter.keyform_finish_operation").into_owned());
                let create_disabled = interaction_disabled
                    || !state.editor.command_available(
                        state
                            .editor
                            .command(zipzi_editor_core::CommandId::CreateParameter),
                    );
                (interaction_reason, create_disabled)
            },
        );
        self.toolbar(create_disabled, interaction_reason, window, cx)
            .into_any_element()
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
            .key_context("ZipziPalette")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::parameter_key_down))
            .child(
                canvas(|_, _, _| (), {
                    let panel = panel.downgrade();
                    move |_, _, window, _| {
                        window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && crate::benchmark::ignore_native_pointer_event()
                            {
                                cx.stop_propagation();
                            }
                        });
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
                                && crate::benchmark::ignore_native_pointer_event()
                            {
                                cx.stop_propagation();
                                return;
                            }
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
                    .child(t!(if self.only_related {
                        "parameter.no_related"
                    } else {
                        "editor.no_parameters"
                    }))
                    .into_any_element()
            } else {
                // A drag repaints this panel; keep work proportional to visible rows.
                sorting::scroll_area(
                    uniform_list("parameters-scroll-area", row_count, move |range, _, cx| {
                        panel.update(cx, |this, cx| {
                            range
                                .map(|index| match this.visible_rows[index].clone() {
                                    // The cache supplies uniform_list's measured geometry;
                                    // it must scale with the row's own content height.
                                    ListRow::Parameter { index, .. } => this.rows[index]
                                        .view
                                        .clone()
                                        .cached(StyleRefinement::default().w_full().h(
                                            crate::ui::compact::scaled(PARAMETER_ROW_HEIGHT, cx),
                                        ))
                                        .into_any_element(),
                                    ListRow::Group {
                                        id, name, depth, ..
                                    } => this.group_header(id, name, depth, disabled, cx),
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
                .map(|list| {
                    crate::ui::compact::scroll_frame(list, &self.scroll)
                        .id("parameters-scroll-frame")
                        .test_support()
                })
                .into_any_element()
            })
            .when(self.document_id.is_some(), |panel| {
                panel.child(sorting::root_drop_target(
                    "parameters-root-drop",
                    &self.sorting,
                    SortItem::Group(None),
                    cx,
                ))
            })
            .into_any_element()
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
