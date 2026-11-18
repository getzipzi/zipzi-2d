use super::*;
use crate::curve_authoring as labels;
use gpui_kit::{
    base::{Popup, RadioGroup, StyledExt as _, Tab, Tabs},
    component::{ThemeStyled, checkbox::Checkbox, radio::Radio},
};
use zipzi_contract::{
    kernel_commands::MotionInterpolation,
    motion_curve::{MotionEasingPreset, MotionFrameStep},
};
use zipzi_editor_core::{TimelineBinding, TimelineCurveSettings, TimelineMixed};

#[derive(Clone, PartialEq, Eq)]
struct Selection {
    binding: TimelineBinding,
    keys: Vec<TimelineKeyRef>,
}

pub(super) struct CurvePopover {
    selection: Selection,
    custom_step: bool,
    preset_family: Option<usize>,
}

#[derive(Clone, Copy)]
enum Choice {
    Interpolation(MotionInterpolation),
    Preset(MotionEasingPreset),
    FrameStep(u32),
    CustomFrameStep,
    TogglePresets,
    PresetFamily(usize),
    Tidy,
    Aligned,
}

fn radio(id: impl Into<ElementId>, label: impl Into<SharedString>, checked: bool) -> Radio {
    let label = label.into();
    Radio::new(id)
        .xsmall()
        .checked(checked)
        .accessibility_label(label.clone())
        .gap(compact::rem(4.))
        .h(compact::rem(18.))
        .items_center()
        .child(div().truncate().child(label))
}

fn apply_choice(
    selection: &Selection,
    choice: Choice,
    cx: &Context<Timeline>,
) -> impl Fn(&mut Window, &mut App) + use<> {
    let panel = cx.weak_entity();
    let selection = selection.clone();
    move |window, cx| {
        let _ = panel.update(cx, |timeline, cx| {
            timeline.apply_curve_choice(&selection, choice, window, cx);
        });
    }
}

fn radio_choice(
    radio: Radio,
    selection: &Selection,
    choice: Choice,
    cx: &Context<Timeline>,
) -> Radio {
    let apply = apply_choice(selection, choice, cx);
    radio.on_change(move |_, window, cx| apply(window, cx))
}

fn setting_row(label: String, mixed: bool, controls: impl IntoElement, cx: &App) -> Stateful<Div> {
    div()
        .id(SharedString::from(label.clone()))
        .role(Role::Group)
        .aria_label(label.clone())
        .flex()
        .items_start()
        .gap(compact::rem(6.))
        .child(
            div()
                .w(compact::rem(76.))
                .flex_shrink_0()
                .pt(compact::rem(2.))
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .flex_wrap()
                .items_center()
                .gap_x(compact::rem(9.))
                .gap_y(compact::rem(2.))
                .child(controls)
                .when(mixed, |group| {
                    group.child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(t!("properties.mixed")),
                    )
                }),
        )
}

// MotionEasingPreset::ALL groups the presets in this stable order.
const PRESET_FAMILIES: [(&str, usize); 8] = [
    ("basic", 0),
    ("Cubic", 4),
    ("Sine", 7),
    ("Quad", 10),
    ("Quart", 13),
    ("Quint", 16),
    ("Expo", 19),
    ("Circ", 22),
];

fn family_presets(family: usize) -> &'static [MotionEasingPreset] {
    let start = PRESET_FAMILIES[family].1;
    let end = PRESET_FAMILIES
        .get(family + 1)
        .map_or(MotionEasingPreset::ALL.len(), |&(_, start)| start);
    &MotionEasingPreset::ALL[start..end]
}

fn preset_preview(
    preset: MotionEasingPreset,
    width: f32,
    height: f32,
    color: Hsla,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let [[x1, y1], [x2, y2]] = preset.definition().controls;
            let mut path = PathBuilder::stroke(px(1.2));
            for i in 0..=24 {
                let t = i as f32 / 24.;
                let a = 1. - t;
                let x = 3. * a * a * t * x1 + 3. * a * t * t * x2 + t * t * t;
                let y = 3. * a * a * t * y1 + 3. * a * t * t * y2 + t * t * t;
                let p = bounds.origin + point(bounds.size.width * x, bounds.size.height * (1. - y));
                if i == 0 {
                    path.move_to(p);
                } else {
                    path.line_to(p);
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w(compact::rem(width))
    .h(compact::rem(height))
}

fn easing_presets(
    family: usize,
    settings: &TimelineCurveSettings,
    selection: &Selection,
    width: Pixels,
    cx: &Context<Timeline>,
) -> impl IntoElement {
    let families = PRESET_FAMILIES
        .iter()
        .enumerate()
        .map(|(index, &(name, _))| {
            let label = if index == 0 {
                t!("timeline.basic_easing").into_owned()
            } else {
                name.into()
            };
            let selected = family == index;
            let apply = apply_choice(selection, Choice::PresetFamily(index), cx);
            Tab::new(SharedString::from(format!(
                "timeline-easing-family-{}",
                name.to_ascii_lowercase()
            )))
            .selected(selected)
            .accessibility_label(label.clone())
            .set_position(index + 1, PRESET_FAMILIES.len())
            .h(compact::rem(20.))
            .px(compact::rem(4.))
            .border_b_1()
            .border_color(if selected {
                cx.theme().primary
            } else {
                transparent_black()
            })
            .text_color(if selected {
                cx.theme().foreground
            } else {
                cx.theme().muted_foreground
            })
            .cursor_pointer()
            .child(label)
            .on_click(move |_, window, cx| apply(window, cx))
        });
    let columns = if width < compact::scaled(250., cx) {
        1
    } else if family == 0 || width < compact::scaled(320., cx) {
        2
    } else {
        3
    };
    div()
        .flex()
        .flex_col()
        .gap(compact::rem(3.))
        .child(
            Tabs::new("timeline-easing-families")
                .aria_label(t!("timeline.easing").into_owned())
                .flex()
                .flex_wrap()
                .items_center()
                .gap(compact::rem(3.))
                .border_b_1()
                .border_color(cx.theme().border)
                .children(families),
        )
        .child(
            RadioGroup::new("timeline-easing-presets")
                .aria_label(t!("timeline.easing").into_owned())
                .axis(Axis::Horizontal)
                .grid()
                .grid_cols(columns)
                .gap_x(compact::rem(12.))
                .gap_y(compact::rem(1.))
                .children(family_presets(family).iter().copied().map(|preset| {
                    let label = labels::preset_label(preset);
                    let short_label = label
                        .strip_prefix(PRESET_FAMILIES[family].0)
                        .and_then(|suffix| suffix.strip_prefix(' '))
                        .unwrap_or(&label)
                        .to_owned();
                    let locked =
                        settings.restricted_conversion_locked && !preset.supports_restricted_time();
                    let selected = settings.preset == TimelineMixed::Uniform(Some(preset));
                    let color = if locked {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().primary
                    };
                    let tooltip = if locked {
                        t!("timeline.preset_conversion_locked").into_owned()
                    } else if preset.definition().approximate {
                        t!("timeline.approximate_preset").into_owned()
                    } else {
                        label.clone()
                    };
                    radio_choice(
                        Radio::new(SharedString::from(format!(
                            "timeline-easing-{}",
                            labels::preset_id(preset)
                        )))
                        .xsmall()
                        .checked(selected)
                        .gap(compact::rem(4.))
                        .h(compact::rem(18.))
                        .items_center()
                        .min_w_0()
                        .accessibility_label(label.clone())
                        .disabled(locked)
                        .tooltip(tooltip)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(compact::rem(4.))
                                .child(div().flex_1().truncate().child(short_label))
                                .child(preset_preview(preset, 24., 11., color)),
                        ),
                        selection,
                        Choice::Preset(preset),
                        cx,
                    )
                })),
        )
}

impl Timeline {
    pub(super) fn current_curve_settings(&self, cx: &App) -> Option<TimelineCurveSettings> {
        let owner = self.owner.upgrade()?;
        let editor = &owner.read(cx).editor;
        let view = editor.timeline_view()?;
        Some(
            editor
                .timeline_projection()?
                .curve_settings(&view.keys, !view.events.is_empty()),
        )
    }

    fn curve_selection(&self, settings: &TimelineCurveSettings, cx: &App) -> Option<Selection> {
        Some(Selection {
            binding: self.target(cx)?.binding,
            keys: settings.keys.clone(),
        })
    }

    pub(super) fn curve_settings_hint(
        &self,
        settings: Option<&TimelineCurveSettings>,
        cx: &App,
    ) -> String {
        if self
            .owner
            .upgrade()
            .is_some_and(|owner| owner.read(cx).editor.animation().is_some_and(|a| a.playing))
        {
            return t!("timeline.pause_to_edit").into_owned();
        }
        settings.map_or_else(
            || t!("timeline.select_curve_keys").into_owned(),
            labels::settings_hint,
        )
    }

    pub(super) fn show_curve_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settings) = self
            .current_curve_settings(cx)
            .filter(TimelineCurveSettings::enabled)
        else {
            return;
        };
        if !self.editable(window, cx) {
            return;
        }
        let Some(selection) = self.curve_selection(&settings, cx) else {
            return;
        };
        self.finish_number(cx);
        self.curve_popover = Some(CurvePopover {
            selection,
            custom_step: matches!(
                settings.frame_step,
                TimelineMixed::Uniform(MotionFrameStep::EveryFrames { frames: 4.. })
            ),
            preset_family: None,
        });
        window.focus(&self.curve_focus, cx);
        cx.notify();
    }

    pub(super) fn close_curve_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.curve_popover.take().is_some() {
            self.finish_number(cx);
            if self.curve_focus.contains_focused(window, cx) {
                window.focus(&self.focus, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn curve_settings_popup(
        &mut self,
        trigger: impl IntoElement,
        settings: Option<TimelineCurveSettings>,
        toggle_trigger: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut popup = Popup::new("timeline-curve-popup", trigger)
            .anchor(Anchor::BottomRight)
            .offset(compact::scaled(4., cx));
        if let Some(settings) = settings
            && self.curve_popover.is_some()
        {
            let trigger_bounds = Rc::new(Cell::new(Bounds::default()));
            popup = popup
                .on_position({
                    let trigger_bounds = trigger_bounds.clone();
                    move |_, bounds| trigger_bounds.set(bounds)
                })
                .content(
                    self.render_curve_settings(settings, window, cx)
                        .on_mouse_down_out(cx.listener(
                            move |this, event: &MouseDownEvent, window, cx| {
                                if !toggle_trigger
                                    || !trigger_bounds.get().contains(&event.position)
                                {
                                    this.close_curve_settings(window, cx);
                                }
                            },
                        ))
                        .test_support(),
                );
        }
        popup.into_any_element()
    }

    fn render_curve_settings(
        &mut self,
        settings: TimelineCurveSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let open = self.curve_popover.as_ref().unwrap();
        let selection = open.selection.clone();
        let custom_step = open.custom_step;
        let preset_family = open.preset_family;
        let action = |button: Button, choice| {
            let apply = apply_choice(&selection, choice, cx);
            button.on_click(move |_, window, cx| apply(window, cx))
        };
        let choose = |radio, choice| radio_choice(radio, &selection, choice, cx);
        let kinds = labels::INTERPOLATIONS.into_iter().map(|(kind, id)| {
            choose(
                radio(
                    SharedString::from(format!("timeline-interpolation-{id}")),
                    labels::kind_label(kind),
                    settings.interpolation == TimelineMixed::Uniform(kind),
                ),
                Choice::Interpolation(kind),
            )
        });
        let steps = [1, 2, 3].into_iter().map(|n| {
            let step = MotionFrameStep::from_frames(n).unwrap();
            choose(
                radio(
                    SharedString::from(format!("timeline-frame-step-{n}")),
                    labels::step_label(step),
                    settings.frame_step == TimelineMixed::Uniform(step) && !custom_step,
                ),
                Choice::FrameStep(n),
            )
        });
        let custom = choose(
            radio(
                "timeline-frame-step-4",
                t!("timeline.custom_curve").into_owned(),
                custom_step,
            )
            .tooltip(t!("timeline.custom_step")),
            Choice::CustomFrameStep,
        );
        let hint = labels::settings_hint(&settings);
        let width = compact::scaled(356., cx).min(window.viewport_size().width - px(16.));
        let mut popover = div()
            .id("timeline-curve-settings-popover")
            .role(Role::Dialog)
            .aria_label(t!("timeline.interval_curve").into_owned())
            .track_focus(&self.curve_focus)
            .tab_group()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_curve_settings(window, cx);
                    cx.stop_propagation();
                }
            }))
            .popover_style(cx)
            .w(width)
            .max_h(window.viewport_size().height - px(16.))
            .overflow_y_scroll()
            .p(compact::rem(8.))
            .flex()
            .flex_col()
            .gap(compact::rem(5.))
            .text_size(compact::rem(compact::BODY))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().font_semibold().child(t!("timeline.interval_curve")))
                    .child(
                        div()
                            .id("timeline-curve-status")
                            .test_support()
                            .aria_label(hint.clone())
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(hint.clone())
                                    .build(window, cx)
                            })
                            .text_size(compact::rem(compact::CAPTION))
                            .text_color(cx.theme().muted_foreground)
                            .child(t!(
                                "timeline.intervals_selected",
                                count = settings.keys.len()
                            )),
                    ),
            )
            .child(setting_row(
                t!("timeline.interpolation_short").into_owned(),
                settings.interpolation == TimelineMixed::Mixed,
                RadioGroup::new("timeline-interpolation-options")
                    .aria_label(t!("timeline.interpolation_short").into_owned())
                    .axis(Axis::Horizontal)
                    .flex()
                    .flex_wrap()
                    .gap_x(compact::rem(9.))
                    .gap_y(compact::rem(2.))
                    .children(kinds),
                cx,
            ));
        if settings.bezier() {
            let current = match settings.preset {
                TimelineMixed::Uniform(Some(preset)) => labels::preset_label(preset),
                TimelineMixed::Uniform(None) => t!("timeline.custom_curve").into_owned(),
                _ => t!("properties.mixed").into_owned(),
            };
            let show_presets = preset_family.is_some();
            popover = popover.child(setting_row(
                t!("timeline.easing").into_owned(),
                false,
                action(
                    compact::header_button(Button::new("timeline-easing"))
                        .ghost()
                        .h(compact::rem(18.))
                        .label(current)
                        .icon(if show_presets {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .open(show_presets)
                        .when_some(
                            match settings.preset {
                                TimelineMixed::Uniform(preset) => preset,
                                _ => None,
                            },
                            |button, preset| {
                                button.child(preset_preview(
                                    preset,
                                    24.,
                                    11.,
                                    cx.theme().foreground,
                                ))
                            },
                        ),
                    Choice::TogglePresets,
                ),
                cx,
            ));
            if let Some(family) = preset_family {
                popover = popover.child(easing_presets(family, &settings, &selection, width, cx));
            }
        }
        let steps = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(compact::rem(9.))
            .gap_y(compact::rem(2.))
            .child(
                RadioGroup::new("timeline-frame-step-options")
                    .aria_label(t!("timeline.frame_step").into_owned())
                    .axis(Axis::Horizontal)
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(compact::rem(9.))
                    .gap_y(compact::rem(2.))
                    .children(steps)
                    .child(custom),
            );
        let handles = settings.handles().then(|| {
            let aligned = self.owner.upgrade().is_some_and(|owner| {
                owner
                    .read(cx)
                    .editor
                    .timeline_view()
                    .is_some_and(|view| view.aligned_handles)
            });
            setting_row(
                t!("timeline.handles").into_owned(),
                false,
                div()
                    .flex()
                    .items_center()
                    .gap(compact::rem(9.))
                    .child({
                        let apply = apply_choice(&selection, Choice::Aligned, cx);
                        Checkbox::new("timeline-aligned-handles")
                            .xsmall()
                            .label(t!("timeline.aligned_handles_short").into_owned())
                            .checked(aligned)
                            .gap(compact::rem(4.))
                            .h(compact::rem(18.))
                            .items_center()
                            .tooltip(t!("timeline.aligned_handles"))
                            .on_change(move |_, window, cx| apply(window, cx))
                    })
                    .child(action(
                        compact::header_button(Button::new("timeline-tidy-handles"))
                            .h(compact::rem(18.))
                            .label(t!("timeline.tidy_handles_short").into_owned())
                            .tooltip(t!("timeline.tidy_handles")),
                        Choice::Tidy,
                    )),
                cx,
            )
        });
        // Prepare the retained field before Custom moves keyboard focus into it.
        let n = match settings.frame_step {
            TimelineMixed::Uniform(MotionFrameStep::EveryFrames { frames }) => f64::from(frames),
            _ => 1.,
        };
        let field = values::Field::FrameStep;
        let text = if settings.frame_step == TimelineMixed::Mixed {
            t!("properties.mixed").into_owned()
        } else {
            format!("{n:.0}")
        };
        self.ensure_field(&field, text, n, (1., u32::MAX as f64), false, window, cx);
        let steps = steps.when(custom_step, |steps| {
            steps.child(
                div()
                    .id("timeline-frame-step-number")
                    .test_support()
                    .w(compact::rem(64.))
                    .child(self.fields[&field].number.clone()),
            )
        });
        popover = popover.child(setting_row(
            t!("timeline.frame_step_short").into_owned(),
            settings.frame_step == TimelineMixed::Mixed && !custom_step,
            steps,
            cx,
        ));
        popover.children(handles)
    }

    fn apply_curve_choice(
        &mut self,
        selection: &Selection,
        choice: Choice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self
            .current_curve_settings(cx)
            .filter(TimelineCurveSettings::enabled)
        else {
            return;
        };
        if self.curve_selection(&settings, cx).as_ref() != Some(selection)
            || !self.editable(window, cx)
        {
            return;
        }
        let Some(open) = self
            .curve_popover
            .as_mut()
            .filter(|open| &open.selection == selection)
        else {
            return;
        };
        let keys = settings.keys;
        let command = match choice {
            Choice::Interpolation(kind) => {
                if kind != MotionInterpolation::Bezier {
                    open.preset_family = None;
                }
                TimelineCommand::Interpolation { keys, kind }
            }
            Choice::Preset(preset) => TimelineCommand::EasingPreset { keys, preset },
            Choice::FrameStep(n) => {
                open.custom_step = false;
                TimelineCommand::FrameStep {
                    keys,
                    frame_step: MotionFrameStep::from_frames(n).unwrap(),
                }
            }
            Choice::Tidy => TimelineCommand::TidyHandles { keys },
            Choice::Aligned => {
                self.change_view(|view| view.aligned_handles = !view.aligned_handles, cx);
                return;
            }
            Choice::TogglePresets => {
                open.preset_family = match (open.preset_family, settings.preset) {
                    (Some(_), _) => None,
                    (None, TimelineMixed::Uniform(Some(preset))) => Some(
                        (0..PRESET_FAMILIES.len())
                            .find(|&family| family_presets(family).contains(&preset))
                            .unwrap_or(0),
                    ),
                    (None, _) => Some(0),
                };
                cx.notify();
                return;
            }
            Choice::PresetFamily(family) => {
                open.preset_family = Some(family);
                cx.notify();
                return;
            }
            Choice::CustomFrameStep => {
                open.custom_step = true;
                if let Some(control) = self.fields.get(&values::Field::FrameStep) {
                    window.focus(&control.input.read(cx).focus_handle(cx), cx);
                    control
                        .input
                        .update(cx, |input, cx| input.select_all(window, cx));
                }
                cx.notify();
                return;
            }
        };
        self.execute(command, window, cx);
    }

    pub(super) fn curve_settings_menu_item(
        panel: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> PopupMenuItem {
        let dialog = window.has_active_dialog(cx);
        let selection = panel.upgrade().and_then(|panel| {
            let panel = panel.read(cx);
            let settings = panel.current_curve_settings(cx)?;
            (settings.enabled() && !dialog && panel.can_edit(cx))
                .then(|| panel.curve_selection(&settings, cx))
                .flatten()
        });
        PopupMenuItem::element(|_, _| {
            let label = t!("timeline.interval_curve_settings");
            div()
                .id("timeline-menu-curve-settings")
                .test_support()
                .aria_label(label.clone().into_owned())
                .child(label)
        })
        .disabled(selection.is_none())
        .on_click(move |_, window, cx| {
            _ = panel.update(cx, |this, cx| {
                let current = this
                    .current_curve_settings(cx)
                    .and_then(|settings| this.curve_selection(&settings, cx));
                if selection.is_some() && current == selection {
                    this.show_curve_settings(window, cx);
                }
            });
        })
    }

    pub(super) fn curve_context_menu(
        m: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
        panel: WeakEntity<Self>,
    ) -> PopupMenu {
        compact::menu(m).item(Self::curve_settings_menu_item(panel, window, cx))
    }

    pub(super) fn sync_curve_settings(
        &mut self,
        settings: Option<&TimelineCurveSettings>,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = &self.curve_popover
            && !(editable
                && settings.is_some_and(|settings| {
                    settings.enabled()
                        && self.curve_selection(settings, cx).as_ref() == Some(&open.selection)
                }))
        {
            self.close_curve_settings(window, cx);
        }
    }
}
