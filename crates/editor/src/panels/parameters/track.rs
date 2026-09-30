use super::*;

pub(super) fn parameter_track(
    slider: &Entity<SliderState>,
    parameter: &ParameterValue,
    visual: &ParameterVisual,
    value: f32,
    disabled: bool,
    theme: &gpui_kit::component::Theme,
    panel: WeakEntity<ParametersPanel>,
    document_id: Option<DocumentId>,
) -> AnyElement {
    let span = parameter.maximum - parameter.minimum;
    let disabled = disabled || span <= 0.;
    let fraction = if span > 0. {
        ((value - parameter.minimum) / span).clamp(0., 1.)
    } else {
        0.
    };
    let cursor_color = if visual.repeat {
        theme.magenta
    } else if !visual.keys.is_empty() {
        theme.red
    } else {
        theme.foreground
    };
    let display_keys = display_keys(parameter, visual);
    let mut marks: Vec<AnyElement> = display_keys
        .iter()
        .map(|key| {
            let position = if span > 0. {
                ((key.value - parameter.minimum) / span).clamp(0., 1.)
            } else {
                0.
            };
            let color = if key.partial {
                theme.yellow
            } else if key.selected {
                theme.green
            } else {
                theme.muted_foreground
            };
            let key_value = key.value;
            let panel = panel.clone();
            let parameter_id = parameter.id.clone();
            div()
                .id(SharedString::from(format!(
                    "parameter-key-{}-{}",
                    parameter.id, key.value
                )))
                .test_support()
                .absolute()
                .left(relative(position))
                .top(px(-4.))
                .ml(-px(5.))
                .size(px(10.))
                .border_1()
                .border_color(color)
                .when(
                    !key.base && visual.kind == kasane_core::ParameterKind::Normal,
                    |this| this.rounded_full(),
                )
                .bg(if key.base && key.selected {
                    theme.foreground
                } else if key.selected && !key.base {
                    color
                } else {
                    theme.background
                })
                .when(!disabled, |this| {
                    this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        if let Some(id) = document_id {
                            let _ = panel.update(cx, |panel, cx| {
                                panel.apply_values(
                                    id,
                                    &[(parameter_id.clone(), key_value)],
                                    window,
                                    cx,
                                )
                            });
                        }
                    })
                })
                .into_any_element()
        })
        .collect();
    marks.push(
        SliderThumb::new(slider)
            .disabled(disabled)
            .absolute()
            .left(relative(fraction))
            .top(-px(7.))
            .ml(-px(7.))
            .size(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size(px(9.))
                    .rounded_full()
                    .border_1()
                    .border_color(cursor_color)
                    .when(!visual.keys.iter().any(|key| key.value == value), |this| {
                        this.bg(cursor_color)
                    }),
            )
            .into_any_element(),
    );
    let snap_slider = slider.clone();
    let snap_keys = visual.keys.clone();
    let snap_parameter = parameter.clone();
    div()
        .w_full()
        .h_6()
        .when(!disabled, |this| {
            this.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let bounds = snap_slider.read(cx).bounds();
                if bounds.size.width <= px(0.) {
                    return;
                }
                let fraction =
                    ((event.position.x - bounds.left()) / bounds.size.width).clamp(0., 1.);
                let value = snap_parameter.minimum
                    + (snap_parameter.maximum - snap_parameter.minimum) * fraction;
                if let (Some(id), Some(key)) = (
                    document_id,
                    snap_keys
                        .iter()
                        .min_by(|a, b| (a.value - value).abs().total_cmp(&(b.value - value).abs())),
                ) {
                    cx.stop_propagation();
                    let _ = panel.update(cx, |panel, cx| {
                        panel.apply_values(
                            id,
                            &[(snap_parameter.id.clone(), key.value)],
                            window,
                            cx,
                        )
                    });
                }
            })
        })
        .child(
            BaseSlider::new(slider)
                .disabled(disabled)
                .w_full()
                .h_6()
                .child(
                    SliderTrack::new(slider)
                        .disabled(disabled)
                        .w_full()
                        .h_6()
                        .flex()
                        .items_center()
                        .child(
                            SliderIndicator::new(slider)
                                .relative()
                                .w_full()
                                .h(px(2.))
                                .bg(theme.muted_foreground.opacity(0.55))
                                .children(marks),
                        ),
                ),
        )
        .into_any_element()
}

pub(super) fn display_keys(
    parameter: &ParameterValue,
    visual: &ParameterVisual,
) -> Vec<editor_core::ParameterKey> {
    if !visual.keys.is_empty() {
        return visual.keys.clone();
    }
    let mut values = vec![parameter.minimum];
    if parameter.maximum != parameter.minimum {
        values.push(parameter.maximum);
    }
    values
        .into_iter()
        .map(|value| editor_core::ParameterKey {
            value,
            selected: false,
            partial: false,
            targets: Vec::new(),
            base: false,
        })
        .collect()
}
