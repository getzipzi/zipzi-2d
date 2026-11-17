use super::*;
use crate::curve_authoring as labels;
use zipzi_contract::{
    kernel_commands::MotionInterpolation,
    motion_curve::{MotionEasingPreset, MotionFrameStep},
};
use zipzi_editor_core::{TimelineCurveSettings, TimelineMixed};

#[derive(Clone, Copy)]
enum MenuKind {
    All,
    Interpolation,
    Preset,
    FrameStep,
    Handles,
}
#[derive(Clone, Copy)]
enum Choice {
    Interpolation(MotionInterpolation),
    Preset(MotionEasingPreset),
    FrameStep(u32),
    CustomFrameStep,
    Tidy,
    Aligned,
}

fn choice_item(
    menu: PopupMenu,
    panel: WeakEntity<Timeline>,
    binding: Option<zipzi_editor_core::TimelineBinding>,
    settings: &TimelineCurveSettings,
    id: String,
    label: String,
    checked: bool,
    choice: Choice,
    preview: Option<MotionEasingPreset>,
) -> PopupMenu {
    let keys = settings.keys.clone();
    let conversion_locked = matches!(choice,Choice::Preset(preset) if !preset.supports_restricted_time() && settings.restricted_conversion_locked);
    menu.item(
        PopupMenuItem::element(move |_, _| {
            div()
                .id(SharedString::from(id.clone()))
                .test_support()
                .flex()
                .items_center()
                .gap_2()
                .when_some(preview, |row, preset| row.child(preset_preview(preset)))
                .child(label.clone())
        })
        .checked(checked)
        .disabled(!settings.enabled() || conversion_locked)
        .on_click(move |_, window, cx| {
            _ = panel.update(cx, |this, cx| {
                if this
                    .target(cx)
                    .is_none_or(|target| Some(target.binding) != binding)
                {
                    return;
                }
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let editor = &owner.read(cx).editor;
                let Some(projection) = editor.timeline_projection() else {
                    return;
                };
                let Some(view) = editor.timeline_view() else {
                    return;
                };
                let current = projection.curve_settings(&view.keys, !view.events.is_empty());
                if !current.enabled() || current.keys != keys || !this.editable(window, cx) {
                    return;
                }
                if matches!(choice, Choice::Aligned) {
                    this.change_view(|view| view.aligned_handles = !view.aligned_handles, cx);
                    return;
                }
                if matches!(choice, Choice::CustomFrameStep) {
                    this.finish_number(cx);
                    if let Some(control) = this.fields.get(&values::Field::FrameStep) {
                        window.focus(&control.input.read(cx).focus_handle(cx), cx);
                        control
                            .input
                            .update(cx, |input, cx| input.select_all(window, cx));
                    }
                    return;
                }
                let command = match choice {
                    Choice::Interpolation(kind) => TimelineCommand::Interpolation {
                        keys: keys.clone(),
                        kind,
                    },
                    Choice::Preset(preset) => TimelineCommand::EasingPreset {
                        keys: keys.clone(),
                        preset,
                    },
                    Choice::FrameStep(frames) => TimelineCommand::FrameStep {
                        keys: keys.clone(),
                        frame_step: MotionFrameStep::from_frames(frames).unwrap(),
                    },
                    Choice::Tidy => TimelineCommand::TidyHandles { keys: keys.clone() },
                    Choice::Aligned | Choice::CustomFrameStep => unreachable!(),
                };
                this.execute(command, window, cx);
            });
        }),
    )
}

fn preset_preview(preset: MotionEasingPreset) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
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
                window.paint_path(path, cx.theme().foreground);
            }
        },
    )
    .w(compact::rem(36.))
    .h(compact::rem(18.))
}

fn curve_menu(
    mut menu: PopupMenu,
    panel: WeakEntity<Timeline>,
    binding: Option<zipzi_editor_core::TimelineBinding>,
    settings: TimelineCurveSettings,
    kind: MenuKind,
    aligned: bool,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    menu = compact::menu(menu);
    match kind {
        MenuKind::All => {
            menu = menu.label(labels::settings_hint(&settings));
            for (kind, label) in [
                (MenuKind::Interpolation, "timeline.interpolation"),
                (MenuKind::Preset, "timeline.easing"),
                (MenuKind::FrameStep, "timeline.frame_step"),
            ] {
                if matches!(kind, MenuKind::Preset) && !settings.bezier() {
                    continue;
                }
                let panel = panel.clone();
                let settings = settings.clone();
                menu = menu.submenu(t!(label).into_owned(), window, cx, move |m, w, c| {
                    curve_menu(
                        m,
                        panel.clone(),
                        binding,
                        settings.clone(),
                        kind,
                        aligned,
                        w,
                        c,
                    )
                });
            }
        }
        MenuKind::Interpolation => {
            for (kind, id) in labels::INTERPOLATIONS {
                menu = choice_item(
                    menu,
                    panel.clone(),
                    binding,
                    &settings,
                    format!("timeline-interpolation-{id}"),
                    labels::kind_label(kind),
                    settings.interpolation == TimelineMixed::Uniform(kind),
                    Choice::Interpolation(kind),
                    None,
                );
            }
        }
        MenuKind::Preset => {
            menu = menu.label(t!("timeline.approximate_preset_short").into_owned());
            for (index, preset) in MotionEasingPreset::ALL.into_iter().take(7).enumerate() {
                if index == 0 || index == 4 {
                    menu = menu.separator().label(if index == 0 {
                        t!("timeline.basic_easing").into_owned()
                    } else {
                        "Cubic".into()
                    });
                }
                menu = choice_item(
                    menu,
                    panel.clone(),
                    binding,
                    &settings,
                    format!("timeline-easing-{}", labels::preset_id(preset)),
                    labels::preset_label(preset),
                    settings.preset == TimelineMixed::Uniform(Some(preset)),
                    Choice::Preset(preset),
                    Some(preset),
                );
            }
            menu = menu.separator();
            for (start, label) in [
                (7, "Sine"),
                (10, "Quad"),
                (13, "Quart"),
                (16, "Quint"),
                (19, "Expo"),
                (22, "Circ"),
            ] {
                let panel = panel.clone();
                let settings = settings.clone();
                menu = menu.submenu(label, window, cx, move |mut menu, _, _| {
                    menu = compact::menu(menu);
                    for preset in MotionEasingPreset::ALL.into_iter().skip(start).take(3) {
                        menu = choice_item(
                            menu,
                            panel.clone(),
                            binding,
                            &settings,
                            format!("timeline-easing-{}", labels::preset_id(preset)),
                            labels::preset_label(preset),
                            settings.preset == TimelineMixed::Uniform(Some(preset)),
                            Choice::Preset(preset),
                            Some(preset),
                        );
                    }
                    menu
                });
            }
        }
        MenuKind::Handles => {}
        MenuKind::FrameStep => {
            for n in [1, 2, 3, 4] {
                let step = MotionFrameStep::from_frames(n).unwrap();
                let label = if n == 4 {
                    t!("timeline.custom_step").into_owned()
                } else {
                    labels::step_label(step)
                };
                let checked = if n == 4 {
                    matches!(
                        settings.frame_step,
                        TimelineMixed::Uniform(MotionFrameStep::EveryFrames { frames: 4.. })
                    )
                } else {
                    settings.frame_step == TimelineMixed::Uniform(step)
                };
                menu = choice_item(
                    menu,
                    panel.clone(),
                    binding,
                    &settings,
                    format!("timeline-frame-step-{n}"),
                    label,
                    checked,
                    if n == 4 {
                        Choice::CustomFrameStep
                    } else {
                        Choice::FrameStep(n)
                    },
                    None,
                );
            }
            menu = menu.label(t!("timeline.step_semantics").into_owned());
        }
    }
    if matches!(kind, MenuKind::All | MenuKind::Handles) && settings.handles() {
        menu = menu.separator();
        menu = choice_item(
            menu,
            panel.clone(),
            binding,
            &settings,
            "timeline-aligned-handles".into(),
            t!("timeline.aligned_handles").into_owned(),
            aligned,
            Choice::Aligned,
            None,
        );
        menu = choice_item(
            menu,
            panel,
            binding,
            &settings,
            "timeline-tidy-handles".into(),
            t!("timeline.tidy_handles").into_owned(),
            false,
            Choice::Tidy,
            None,
        );
    }
    menu
}

impl Timeline {
    pub(super) fn curve_controls(
        &mut self,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = projection.curve_settings(&state.keys, !state.events.is_empty());
        let enabled = settings.enabled() && self.editable(window, cx);
        let hint = if self
            .owner
            .upgrade()
            .is_some_and(|owner| owner.read(cx).editor.animation().is_some_and(|a| a.playing))
        {
            t!("timeline.pause_to_edit").into_owned()
        } else {
            labels::settings_hint(&settings)
        };
        let binding = self.target(cx).map(|t| t.binding);
        let aligned = state.aligned_handles;
        let mut row = div()
            .id("timeline-curve-settings")
            .test_support()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .h(compact::rem(30.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(cx.theme().border);
        let choices = if self.compact {
            vec![(
                MenuKind::All,
                "timeline-curve-menu",
                t!("timeline.curve_settings").into_owned(),
            )]
        } else {
            let interpolation = match settings.interpolation {
                TimelineMixed::Uniform(kind) => labels::kind_label(kind),
                TimelineMixed::Mixed => t!("properties.mixed").into_owned(),
                _ => "—".into(),
            };
            let mut buttons = vec![(
                MenuKind::Interpolation,
                "timeline-interpolation",
                format!("{}: {interpolation}", t!("timeline.interpolation_short")),
            )];
            if settings.bezier() {
                let preset = match settings.preset {
                    TimelineMixed::Uniform(Some(p)) => labels::preset_label(p),
                    TimelineMixed::Uniform(None) => t!("timeline.custom_curve").into_owned(),
                    _ => t!("properties.mixed").into_owned(),
                };
                buttons.push((
                    MenuKind::Preset,
                    "timeline-easing",
                    format!("{}: {preset}", t!("timeline.easing")),
                ));
            }
            let step = match settings.frame_step {
                TimelineMixed::Uniform(step) => labels::step_label(step),
                _ => t!("properties.mixed").into_owned(),
            };
            buttons.push((
                MenuKind::FrameStep,
                "timeline-frame-step",
                format!("{}: {step}", t!("timeline.frame_step")),
            ));
            if settings.handles() {
                buttons.push((
                    MenuKind::Handles,
                    "timeline-handles",
                    t!("timeline.handles").into_owned(),
                ));
            }
            buttons
        };
        for (kind, id, label) in choices {
            let panel = cx.weak_entity();
            let settings = settings.clone();
            row = row.child(
                compact::header_button(Button::new(id))
                    .label(label.clone())
                    .accessibility_label(label)
                    .icon(IconName::ChevronDown)
                    .tooltip(hint.clone())
                    .disabled(!enabled)
                    .dropdown_menu(move |m, w, c| {
                        curve_menu(
                            m,
                            panel.clone(),
                            binding,
                            settings.clone(),
                            kind,
                            aligned,
                            w,
                            c,
                        )
                    }),
            );
        }
        let n = match settings.frame_step {
            TimelineMixed::Uniform(MotionFrameStep::EveryFrames { frames }) => frames as f64,
            _ => 1.,
        };
        let field = values::Field::FrameStep;
        let text = match settings.frame_step {
            TimelineMixed::Mixed => t!("properties.mixed").into_owned(),
            _ => format!("{n:.0}"),
        };
        self.ensure_field(&field, text, n, (1., u32::MAX as f64), !enabled, window, cx);
        row = row
            .child(div().flex_shrink_0().child(t!("timeline.frame_step_n")))
            .child(
                div()
                    .id("timeline-frame-step-number")
                    .test_support()
                    .w(compact::rem(68.))
                    .flex_shrink_0()
                    .child(self.fields[&field].number.clone()),
            );
        row.child(
            div()
                .id("timeline-curve-status")
                .test_support()
                .aria_label(hint.clone())
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        )
        .into_any_element()
    }

    pub(super) fn curve_context_menu(
        m: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
        panel: WeakEntity<Self>,
    ) -> PopupMenu {
        let data = (|| {
            let entity = panel.upgrade()?;
            let this = entity.read(cx);
            let owner = this.owner.upgrade()?;
            let editor = &owner.read(cx).editor;
            let (projection, view) = (editor.timeline_projection()?, editor.timeline_view()?);
            let settings = projection.curve_settings(&view.keys, !view.events.is_empty());
            Some((
                editor.timeline_binding(this.id),
                settings,
                view.aligned_handles,
            ))
        })();
        let Some((binding, settings, aligned)) = data else {
            return m;
        };
        curve_menu(
            m,
            panel,
            binding,
            settings,
            MenuKind::All,
            aligned,
            window,
            cx,
        )
    }
}
