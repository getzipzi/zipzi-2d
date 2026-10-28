use super::{
    view::{LinkedPadState, LinkedRole},
    *,
};

const TRACK_HEIGHT: f32 = 2.;
const MARKER_SIZE: f32 = 10.;
const HANDLE_HIT_SIZE: f32 = 16.;
pub(super) const PAD_VERTICAL_INSET: f32 = 6.;

pub(super) fn parameter_step(parameter: &ParameterValue) -> f32 {
    let span = parameter.maximum - parameter.minimum;
    10_f32
        .powi(-(parameter.decimal_places as i32))
        .min(span)
        .max(span / 1000.)
        .max(f32::EPSILON)
}

pub(super) fn value_from_fraction(parameter: &ParameterValue, fraction: f32) -> f32 {
    let fraction = fraction.clamp(0., 1.);
    if fraction <= 0. {
        return parameter.minimum;
    }
    if fraction >= 1. {
        return parameter.maximum;
    }
    let raw = parameter.minimum + (parameter.maximum - parameter.minimum) * fraction;
    let step = parameter_step(parameter);
    ((raw / step).round() * step).clamp(parameter.minimum, parameter.maximum)
}

pub(super) fn pad_full_bounds(row_bounds: Bounds<Pixels>, role: LinkedRole) -> Bounds<Pixels> {
    let origin = match role {
        LinkedRole::Top => row_bounds.origin,
        LinkedRole::Bottom => point(
            row_bounds.origin.x,
            row_bounds.origin.y - row_bounds.size.height,
        ),
    };
    Bounds::new(
        origin,
        size(row_bounds.size.width, row_bounds.size.height * 2.),
    )
}

pub(super) fn pad_values_at_position(
    top: &ParameterValue,
    bottom: &ParameterValue,
    full_bounds: Bounds<Pixels>,
    position: Point<Pixels>,
    cx: &App,
) -> (f32, f32) {
    let width = full_bounds.size.width.max(px(1.));
    let inset = crate::ui::compact::scaled(PAD_VERTICAL_INSET, cx);
    let grid_top = full_bounds.top() + inset;
    let grid_height = (full_bounds.size.height - inset * 2.).max(px(1.));
    let x_fraction = ((position.x - full_bounds.left()) / width).clamp(0., 1.);
    let y_fraction = (1. - (position.y - grid_top) / grid_height).clamp(0., 1.);
    (
        value_from_fraction(top, x_fraction),
        value_from_fraction(bottom, y_fraction),
    )
}

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
                .map(|marker| {
                    key_marker_style(marker, key.base, key.selected, visual.kind, color, theme)
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
            .child(cursor_marker(
                cursor_color,
                visual.keys.iter().any(|key| key.value == value),
            ))
            .into_any_element(),
    );
    marks
}

fn track_style<T: Styled>(element: T, theme: &gpui_kit::component::Theme) -> T {
    element
        .relative()
        .w_full()
        .h(crate::ui::compact::rem(TRACK_HEIGHT))
        .bg(track_color(theme))
}

fn track_color(theme: &gpui_kit::component::Theme) -> Hsla {
    theme.muted_foreground.opacity(0.55)
}

fn key_marker_style<T: Styled + FluentBuilder>(
    marker: T,
    base: bool,
    selected: bool,
    kind: zipzi_core::ParameterKind,
    color: Hsla,
    theme: &gpui_kit::component::Theme,
) -> T {
    marker
        .size(crate::ui::compact::rem(MARKER_SIZE))
        .border_1()
        .border_color(color)
        .when(!base && kind == zipzi_core::ParameterKind::Normal, |this| {
            this.rounded_full()
        })
        .bg(if base && selected {
            theme.foreground
        } else if selected && !base {
            color
        } else {
            theme.background
        })
}

fn cursor_marker(color: Hsla, at_key: bool) -> Div {
    div()
        .size(crate::ui::compact::rem(MARKER_SIZE))
        .rounded_full()
        .border_1()
        .border_color(color)
        .when(!at_key, |this| this.bg(color))
}

pub(super) fn parameter_pad_track(
    slider: &Entity<SliderState>,
    linked: &LinkedPadState,
    top_value: f32,
    bottom_value: f32,
    disabled: bool,
    theme: &gpui_kit::component::Theme,
    panel: WeakEntity<ParametersPanel>,
    document_id: Option<DocumentId>,
) -> AnyElement {
    let top_span = linked.top_parameter.maximum - linked.top_parameter.minimum;
    let bottom_span = linked.bottom_parameter.maximum - linked.bottom_parameter.minimum;
    let disabled = disabled || top_span <= 0. || bottom_span <= 0.;
    let role = linked.role;
    let top_id = linked.top_parameter.id.clone();
    let bottom_id = linked.bottom_parameter.id.clone();
    let container_id = match role {
        LinkedRole::Top => format!("parameter-pad-top-{top_id}-{bottom_id}"),
        LinkedRole::Bottom => format!("parameter-pad-bottom-{top_id}-{bottom_id}"),
    };

    let marks = pad_marks(
        linked,
        top_value,
        bottom_value,
        theme,
        (!disabled).then(|| (slider.clone(), panel.clone(), document_id)),
    );

    let snap_slider = slider.clone();
    let snap_panel = panel.clone();
    let snap_top = linked.top_parameter.clone();
    let snap_bottom = linked.bottom_parameter.clone();
    let snap_top_keys = linked.top_visual.keys.clone();
    let snap_bottom_keys = linked.bottom_visual.keys.clone();

    div()
        .w_full()
        .h(crate::ui::compact::rem(crate::ui::compact::ROW))
        .when(!disabled, |this| {
            this.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let row_bounds = snap_slider.read(cx).bounds();
                if row_bounds.size.width <= px(0.) {
                    return;
                }
                let full_bounds = pad_full_bounds(row_bounds, role);
                let (x_val, y_val) = pad_values_at_position(
                    &snap_top,
                    &snap_bottom,
                    full_bounds,
                    event.position,
                    cx,
                );
                let nearest_x = snap_top_keys
                    .iter()
                    .min_by(|a, b| (a.value - x_val).abs().total_cmp(&(b.value - x_val).abs()))
                    .map(|k| k.value);
                let nearest_y = snap_bottom_keys
                    .iter()
                    .min_by(|a, b| (a.value - y_val).abs().total_cmp(&(b.value - y_val).abs()))
                    .map(|k| k.value);
                if let Some(id) = document_id {
                    let mut updates = Vec::new();
                    if let Some(x) = nearest_x {
                        updates.push((snap_top.id.clone(), x));
                    }
                    if let Some(y) = nearest_y {
                        updates.push((snap_bottom.id.clone(), y));
                    }
                    if !updates.is_empty() {
                        cx.stop_propagation();
                        let _ = snap_panel.update(cx, |panel, cx| {
                            panel.apply_values(id, &updates, window, cx);
                        });
                    }
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
                        .id(SharedString::from(container_id))
                        .test_support()
                        .relative()
                        .w_full()
                        .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                        .when(!disabled, |this| {
                            let panel = panel.clone();
                            let slider = slider.clone();
                            let top_id = top_id.clone();
                            let bottom_id = bottom_id.clone();
                            this.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                                cx.stop_propagation();
                                if let Some(id) = document_id {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.begin_pad_drag(
                                            id,
                                            &top_id,
                                            &bottom_id,
                                            &slider,
                                            role,
                                            event.position,
                                            true,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            })
                        })
                        .child(
                            // Clip each virtual row vertically, while leaving room for
                            // the endpoint markers and their hit targets horizontally.
                            div()
                                .absolute()
                                .left(crate::ui::compact::rem(-HANDLE_HIT_SIZE / 2.))
                                .right(crate::ui::compact::rem(-HANDLE_HIT_SIZE / 2.))
                                .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                                .overflow_hidden()
                                .child(
                                    SliderIndicator::new(slider)
                                        .relative()
                                        .mx(crate::ui::compact::rem(HANDLE_HIT_SIZE / 2.))
                                        .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                                        .child(
                                            div()
                                                .absolute()
                                                .left_0()
                                                .right_0()
                                                .h(crate::ui::compact::rem(
                                                    crate::ui::compact::ROW * 2.,
                                                ))
                                                .map(|el| match role {
                                                    LinkedRole::Top => el.top_0(),
                                                    LinkedRole::Bottom => el.bottom_0(),
                                                })
                                                .child(
                                                    div()
                                                        .absolute()
                                                        .left_0()
                                                        .right_0()
                                                        .top(crate::ui::compact::rem(
                                                            PAD_VERTICAL_INSET,
                                                        ))
                                                        .bottom(crate::ui::compact::rem(
                                                            PAD_VERTICAL_INSET,
                                                        ))
                                                        .children(marks),
                                                ),
                                        ),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

fn pad_marks(
    linked: &LinkedPadState,
    top_value: f32,
    bottom_value: f32,
    theme: &gpui_kit::component::Theme,
    interaction: Option<TrackInteraction>,
) -> Vec<AnyElement> {
    let top = &linked.top_parameter;
    let bottom = &linked.bottom_parameter;
    let top_span = top.maximum - top.minimum;
    let bottom_span = bottom.maximum - bottom.minimum;
    let x_norm = |v: f32| {
        if top_span > 0. {
            ((v - top.minimum) / top_span).clamp(0., 1.)
        } else {
            0.
        }
    };
    let y_norm = |v: f32| {
        if bottom_span > 0. {
            ((v - bottom.minimum) / bottom_span).clamp(0., 1.)
        } else {
            0.
        }
    };

    let mut x_lines = vec![0., 1.];
    for key in &linked.top_visual.keys {
        x_lines.push(x_norm(key.value));
    }
    x_lines.sort_by(f32::total_cmp);
    x_lines.dedup_by(|a, b| (*a - *b).abs() < 1e-4);

    let mut y_lines = vec![0., 1.];
    for key in &linked.bottom_visual.keys {
        y_lines.push(y_norm(key.value));
    }
    y_lines.sort_by(f32::total_cmp);
    y_lines.dedup_by(|a, b| (*a - *b).abs() < 1e-4);

    let line_color = track_color(theme);
    let mut elements = Vec::new();
    for x in x_lines {
        elements.push(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative(x))
                .ml(crate::ui::compact::rem(-TRACK_HEIGHT / 2.))
                .w(crate::ui::compact::rem(TRACK_HEIGHT))
                .bg(line_color)
                .into_any_element(),
        );
    }
    for y in y_lines {
        elements.push(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(1. - y))
                .mt(crate::ui::compact::rem(-TRACK_HEIGHT / 2.))
                .h(crate::ui::compact::rem(TRACK_HEIGHT))
                .bg(line_color)
                .into_any_element(),
        );
    }

    let role_prefix = match linked.role {
        LinkedRole::Top => "parameter-pad-key",
        LinkedRole::Bottom => "parameter-pad-bottom-key",
    };

    let mut intersections: Vec<(f32, f32, bool, bool, bool)> = Vec::new();
    match (
        linked.top_visual.keys.is_empty(),
        linked.bottom_visual.keys.is_empty(),
    ) {
        (false, false) => {
            for kx in &linked.top_visual.keys {
                for ky in &linked.bottom_visual.keys {
                    let selected = kx.selected && ky.selected;
                    let partial =
                        !selected && (kx.selected || kx.partial || ky.selected || ky.partial);
                    let base = kx.base && ky.base;
                    intersections.push((kx.value, ky.value, selected, partial, base));
                }
            }
        }
        (false, true) => {
            for kx in &linked.top_visual.keys {
                intersections.push((
                    kx.value,
                    bottom.default_value,
                    kx.selected,
                    kx.partial,
                    kx.base,
                ));
            }
        }
        (true, false) => {
            for ky in &linked.bottom_visual.keys {
                intersections.push((
                    top.default_value,
                    ky.value,
                    ky.selected,
                    ky.partial,
                    ky.base,
                ));
            }
        }
        (true, true) => {}
    }

    for (vx, vy, selected, partial, base) in &intersections {
        let px_norm = x_norm(*vx);
        let py_norm = y_norm(*vy);
        let color = if *partial {
            theme.yellow
        } else if *selected {
            theme.green
        } else {
            theme.muted_foreground
        };
        let top_id = top.id.clone();
        let bottom_id = bottom.id.clone();
        let vx = *vx;
        let vy = *vy;
        let kind = if linked.top_visual.kind == zipzi_core::ParameterKind::Normal {
            linked.bottom_visual.kind
        } else {
            linked.top_visual.kind
        };
        elements.push(
            div()
                .id(SharedString::from(format!(
                    "{role_prefix}-{top_id}-{bottom_id}-{vx}-{vy}"
                )))
                .test_support()
                .absolute()
                .left(relative(px_norm))
                .top(relative(1. - py_norm))
                .ml(crate::ui::compact::rem(-MARKER_SIZE / 2.))
                .mt(crate::ui::compact::rem(-MARKER_SIZE / 2.))
                .map(|marker| key_marker_style(marker, *base, *selected, kind, color, theme))
                .when_some(interaction.clone(), |this, (_, panel, document_id)| {
                    this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        if let Some(id) = document_id {
                            let _ = panel.update(cx, |panel, cx| {
                                panel.apply_values(
                                    id,
                                    &[(top_id.clone(), vx), (bottom_id.clone(), vy)],
                                    window,
                                    cx,
                                )
                            });
                        }
                    })
                })
                .into_any_element(),
        );
    }

    let x_fraction = x_norm(top_value);
    let y_fraction = y_norm(bottom_value);
    let has_keys = !linked.top_visual.keys.is_empty() || !linked.bottom_visual.keys.is_empty();
    let cursor_color = if linked.top_visual.repeat || linked.bottom_visual.repeat {
        theme.magenta
    } else if has_keys {
        theme.red
    } else {
        theme.foreground
    };
    let at_key = intersections.iter().any(|(vx, vy, _, _, _)| {
        (*vx - top_value).abs() < 1e-6 && (*vy - bottom_value).abs() < 1e-6
    });
    let thumb_id = match linked.role {
        LinkedRole::Top => ("parameter-pad-thumb", 0_u32),
        LinkedRole::Bottom => ("parameter-pad-bottom-thumb", 0_u32),
    };
    let top_id = top.id.clone();
    let bottom_id = bottom.id.clone();
    let role = linked.role;
    elements.push(
        div()
            .id(thumb_id)
            .test_support()
            .when_some(interaction, |this, (slider, panel, document_id)| {
                this.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    cx.stop_propagation();
                    if let Some(id) = document_id {
                        let _ = panel.update(cx, |panel, cx| {
                            panel.begin_pad_drag(
                                id,
                                &top_id,
                                &bottom_id,
                                &slider,
                                role,
                                event.position,
                                false,
                                window,
                                cx,
                            );
                        });
                    }
                })
            })
            .absolute()
            .left(relative(x_fraction))
            .top(relative(1. - y_fraction))
            .ml(crate::ui::compact::rem(-HANDLE_HIT_SIZE / 2.))
            .mt(crate::ui::compact::rem(-HANDLE_HIT_SIZE / 2.))
            .size(crate::ui::compact::rem(HANDLE_HIT_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .child(cursor_marker(cursor_color, at_key))
            .into_any_element(),
    );
    elements
}
