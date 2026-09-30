use super::track::display_keys;
use super::*;

#[derive(Clone)]
struct LinkedDrag {
    document_id: DocumentId,
    pair: (String, String),
}

impl Render for LinkedDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub(super) fn linked_pad(
    x: &ParameterValue,
    y: &ParameterValue,
    x_visual: &ParameterVisual,
    y_visual: &ParameterVisual,
    panel: &Entity<ParametersPanel>,
    document_id: DocumentId,
    disabled: bool,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    let fraction = |value: f32, minimum: f32, maximum: f32| {
        if maximum > minimum {
            ((value - minimum) / (maximum - minimum)).clamp(0., 1.)
        } else {
            0.
        }
    };
    let xp = fraction(x.value, x.minimum, x.maximum);
    let yp = 1. - fraction(y.value, y.minimum, y.maximum);
    let x_has_key = !x_visual.keys.is_empty();
    let y_has_key = !y_visual.keys.is_empty();
    let line_color = if x_has_key ^ y_has_key {
        theme.green
    } else {
        theme.foreground
    };
    let solid = x_has_key && y_has_key;
    let pair = (x.id.clone(), y.id.clone());
    let pointer_pair = pair.clone();
    let bounds_pair = pair.clone();
    let x_blend = x_visual.kind == kasane_core::ParameterKind::BlendShape;
    let y_blend = y_visual.kind == kasane_core::ParameterKind::BlendShape;
    let marker_square = x_blend || y_blend;
    let mixed_blend = x_blend ^ y_blend;
    let pointer = panel.clone();
    let drag = panel.clone();
    let bounds = panel.clone();
    let mut grid: Vec<AnyElement> = Vec::new();
    let x_marks = display_keys(x, x_visual);
    let y_marks = display_keys(y, y_visual);
    let x_positions: Vec<_> = x_marks
        .iter()
        .map(|key| (fraction(key.value, x.minimum, x.maximum), key))
        .collect();
    let y_positions: Vec<_> = y_marks
        .iter()
        .map(|key| (1. - fraction(key.value, y.minimum, y.maximum), key))
        .collect();
    for &(position, _) in &x_positions {
        let color = line_color;
        for segment in 0..if solid { 1 } else { 12 } {
            let start = if solid { 0. } else { segment as f32 / 12. };
            grid.push(
                div()
                    .absolute()
                    .left(relative(position))
                    .top(relative(start))
                    .w(px(1.))
                    .h(relative(if solid { 1. } else { 0.045 }))
                    .bg(color)
                    .into_any_element(),
            );
        }
    }
    for &(position, _) in &y_positions {
        let color = line_color;
        for segment in 0..if solid { 1 } else { 12 } {
            let start = if solid { 0. } else { segment as f32 / 12. };
            grid.push(
                div()
                    .absolute()
                    .top(relative(position))
                    .left(relative(start))
                    .h(px(1.))
                    .w(relative(if solid { 1. } else { 0.045 }))
                    .bg(color)
                    .into_any_element(),
            );
        }
    }
    for &(x_position, x_key_selected) in &x_positions {
        for &(y_position, y_key_selected) in &y_positions {
            let selected = x_key_selected.selected && y_key_selected.selected;
            let partial = x_key_selected.partial || y_key_selected.partial;
            let key_panel = panel.clone();
            let key_pair = (x.id.clone(), y.id.clone());
            let key_values = vec![
                (x.id.clone(), x_key_selected.value),
                (y.id.clone(), y_key_selected.value),
            ];
            let color = if partial {
                theme.yellow
            } else if selected {
                theme.green
            } else {
                theme.muted_foreground
            };
            grid.push(
                div()
                    .id(SharedString::from(format!(
                        "linked-key-{}-{}",
                        x_key_selected.value, y_key_selected.value
                    )))
                    .test_support()
                    .when(!disabled, |this| {
                        this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            key_panel.update(cx, |panel, cx| {
                                if panel.document_id == Some(document_id)
                                    && panel.linked.as_ref() == Some(&key_pair)
                                {
                                    panel.apply_values(document_id, &key_values, window, cx);
                                }
                            });
                        })
                    })
                    .absolute()
                    .left(relative(x_position))
                    .top(relative(y_position))
                    .ml(-px(5.))
                    .mt(-px(5.))
                    .size(px(10.))
                    .when(!marker_square, |this| this.rounded_full())
                    .border_1()
                    .border_color(color)
                    .bg(if selected { color } else { theme.muted })
                    .when(mixed_blend, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .h_full()
                                .w(relative(0.5))
                                .bg(theme.foreground),
                        )
                    })
                    .into_any_element(),
            );
        }
    }
    div()
        .p_2()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_xs()
                .child(format!("{} / {}", x.name, y.name))
                .child(format!(
                    "{} / {}",
                    format_value(x.value, x.decimal_places),
                    format_value(y.value, y.decimal_places)
                )),
        )
        .child(
            div()
                .id("linked-parameter-pad")
                .test_support()
                .relative()
                .mt_1()
                .w_full()
                .h(px(126.))
                .bg(theme.muted)
                .border_1()
                .border_color(theme.border)
                .on_prepaint(move |rect, _, cx| {
                    bounds.update(cx, |panel, _| {
                        if panel.document_id == Some(document_id)
                            && panel.linked.as_ref() == Some(&bounds_pair)
                        {
                            panel.linked_bounds = rect;
                        }
                    });
                })
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        if disabled {
                            return;
                        }
                        pointer.update(cx, |panel, cx| {
                            if panel.document_id == Some(document_id)
                                && panel.linked.as_ref() == Some(&pointer_pair)
                            {
                                panel.move_linked(event.position, window, cx);
                            }
                        });
                    },
                )
                .when(!disabled, |this| {
                    this.on_drag(LinkedDrag { document_id, pair }, |drag, _, _, cx| {
                        cx.new(|_| drag.clone())
                    })
                })
                .on_drag_move(move |event: &DragMoveEvent<LinkedDrag>, window, cx| {
                    if disabled {
                        return;
                    }
                    drag.update(cx, |panel, cx| {
                        if panel.document_id == Some(event.drag(cx).document_id)
                            && panel.linked.as_ref() == Some(&event.drag(cx).pair)
                        {
                            panel.move_linked(event.event.position, window, cx);
                        }
                    });
                })
                .children(grid)
                .child(
                    div()
                        .absolute()
                        .left(relative(xp))
                        .top(relative(yp))
                        .ml(-px(6.))
                        .mt(-px(6.))
                        .size(px(12.))
                        .when(!marker_square, |this| this.rounded_full())
                        .border_1()
                        .border_color(theme.background)
                        .bg(if x_visual.repeat || y_visual.repeat {
                            theme.magenta
                        } else if x_has_key || y_has_key {
                            theme.red
                        } else {
                            theme.foreground
                        }),
                ),
        )
        .into_any_element()
}
