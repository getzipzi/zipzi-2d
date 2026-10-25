use super::*;

const TRACK_HEIGHT: f32 = 2.;
const MARKER_SIZE: f32 = 10.;
const HANDLE_HIT_SIZE: f32 = 16.;

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
    let disabled = disabled || parameter.maximum <= parameter.minimum;
    let marks = track_marks(
        parameter,
        visual,
        value,
        theme,
        (!disabled).then(|| (slider.clone(), panel.clone(), document_id)),
    );
    let snap_slider = slider.clone();
    let snap_panel = panel.clone();
    let snap_keys = visual.keys.clone();
    let snap_parameter = parameter.clone();
    div()
        .w_full()
        .h(crate::ui::compact::rem(crate::ui::compact::ROW))
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
                    let _ = snap_panel.update(cx, |panel, cx| {
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
                .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                .child(
                    div()
                        .id("slider-bar-container")
                        .test_support()
                        .when(!disabled, |this| {
                            let panel = panel.clone();
                            let slider = slider.clone();
                            this.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                                cx.stop_propagation();
                                if let Some(id) = document_id {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.begin_slider_drag(
                                            id,
                                            &slider,
                                            event.position,
                                            true,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            })
                        })
                        .w_full()
                        .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                        .flex()
                        .items_center()
                        .child(track_style(SliderIndicator::new(slider), theme).children(marks)),
                ),
        )
        .into_any_element()
}

// Only the authoring track supplies interaction; preview tracks have no input
// handlers or slider state and cannot write back to the document.
type TrackInteraction = (
    Entity<SliderState>,
    WeakEntity<ParametersPanel>,
    Option<DocumentId>,
);

fn track_marks(
    parameter: &ParameterValue,
    visual: &ParameterVisual,
    value: f32,
    theme: &gpui_kit::component::Theme,
    interaction: Option<TrackInteraction>,
) -> Vec<AnyElement> {
    let span = parameter.maximum - parameter.minimum;

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
    let mut marks: Vec<AnyElement> = visual
        .keys
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
            let parameter_id = parameter.id.clone();
            div()
                .id(SharedString::from(format!(
                    "parameter-key-{}-{}",
                    parameter.id, key.value
                )))
                .test_support()
                .absolute()
                .left(relative(position))
                .top(crate::ui::compact::rem((TRACK_HEIGHT - MARKER_SIZE) / 2.))
                .ml(crate::ui::compact::rem(-MARKER_SIZE / 2.))
                .size(crate::ui::compact::rem(MARKER_SIZE))
                .border_1()
                .border_color(color)
                .when(
                    !key.base && visual.kind == zipzi_core::ParameterKind::Normal,
                    |this| this.rounded_full(),
                )
                .bg(if key.base && key.selected {
                    theme.foreground
                } else if key.selected && !key.base {
                    color
                } else {
                    theme.background
                })
                .when_some(interaction.clone(), |this, (_, panel, document_id)| {
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
        div()
            .id(("slider-thumb", 0_u32))
            .test_support()
            .when_some(interaction, |this, (slider, panel, document_id)| {
                this.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    cx.stop_propagation();
                    if let Some(id) = document_id {
                        let _ = panel.update(cx, |panel, cx| {
                            panel.begin_slider_drag(id, &slider, event.position, false, window, cx);
                        });
                    }
                })
            })
            .absolute()
            .left(relative(fraction))
            .top(crate::ui::compact::rem(
                (TRACK_HEIGHT - HANDLE_HIT_SIZE) / 2.,
            ))
            .ml(crate::ui::compact::rem(-HANDLE_HIT_SIZE / 2.))
            .size(crate::ui::compact::rem(HANDLE_HIT_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size(crate::ui::compact::rem(MARKER_SIZE))
                    .rounded_full()
                    .border_1()
                    .border_color(cursor_color)
                    .when(!visual.keys.iter().any(|key| key.value == value), |this| {
                        this.bg(cursor_color)
                    }),
            )
            .into_any_element(),
    );
    marks
}

fn track_style<T: Styled>(element: T, theme: &gpui_kit::component::Theme) -> T {
    element
        .relative()
        .w_full()
        .h(crate::ui::compact::rem(TRACK_HEIGHT))
        .bg(theme.muted_foreground.opacity(0.55))
}
