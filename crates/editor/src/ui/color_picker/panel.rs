use super::*;
use crate::{t, ui::compact};
use gpui_kit::IntoElement;
use gpui_kit::base::{Tab, Tabs};
use gpui_kit::component::{
    ActiveTheme, Disableable,
    button::{Button, ButtonVariants},
    input::Input,
};

/// Complete picker content, also usable without a popover. Edits begin a session
/// automatically; Done/Enter commits it and Cancel/Escape restores its baseline.
#[derive(IntoElement)]
pub(crate) struct ColorPickerPanel {
    state: Entity<ColorPickerState>,
}

impl ColorPickerPanel {
    pub fn new(state: &Entity<ColorPickerState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl RenderOnce for ColorPickerPanel {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.state
    }
}

pub(super) fn swatch(color: Rgba) -> Div {
    div()
        .relative()
        .overflow_hidden()
        .bg(checkerboard(rgba(0x808080ff), 4.))
        .child(div().absolute().inset_0().bg(color))
}

fn gradient(from: Rgba, to: Rgba, angle: f32) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(from, 0.),
        linear_color_stop(to, 1.),
    )
}

fn marker(position: wheel::Position, color: Rgba) -> Div {
    div()
        .absolute()
        .left(relative(position[0]))
        .top(relative(position[1]))
        .ml(compact::rem(-4.))
        .mt(compact::rem(-4.))
        .size(compact::rem(8.))
        .rounded_full()
        .border_2()
        .border_color(rgb(0xffffff))
        .shadow(vec![BoxShadow {
            color: rgba(0x000000b0).into(),
            offset: point(px(0.), px(0.)),
            blur_radius: px(1.),
            spread_radius: px(1.),
            inset: false,
        }])
        .bg(Rgba { a: 1., ..color })
}

fn surface_frame(
    this: &ColorPickerState,
    kind: Surface,
    cx: &Context<ColorPickerState>,
) -> Stateful<Div> {
    let index = kind as usize;
    div()
        .id(["color-picker-sv", "color-picker-hue", "color-picker-alpha"][index])
        .track_focus(&this.surface_focus[index])
        .cursor(CursorStyle::Crosshair)
        .aria_label(t!([
            "color_picker.saturation_value",
            "color_picker.hue",
            "color_picker.alpha"
        ][index]))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if this.disabled {
                    return;
                }
                let position = normalized(this.bounds[index], event.position);
                if kind != Surface::Alpha && wheel::hit(position, this.hsv.h) != Some(kind) {
                    return;
                }
                this.begin(cx);
                this.drag = Some(kind);
                window.focus(&this.surface_focus[index], cx);
                this.pick(kind, event.position, window, cx);
                window.prevent_default();
                cx.stop_propagation();
            }),
        )
        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
            let delta = if event.keystroke.modifiers.shift {
                0.1
            } else {
                0.01
            };
            let (dx, dy) = match event.keystroke.key.as_str() {
                "left" => (-delta, 0.),
                "right" => (delta, 0.),
                "up" => (0., delta),
                "down" => (0., -delta),
                _ => return,
            };
            if this.disabled {
                return;
            }
            this.begin(cx);
            let mut hsv = this.hsv;
            let mut color = this.color;
            match kind {
                Surface::Sv => {
                    hsv.s = (hsv.s + dx).clamp(0., 1.);
                    hsv.v = (hsv.v + dy).clamp(0., 1.);
                    color = hsv.rgb(color.a);
                }
                Surface::Hue => {
                    hsv.h = (hsv.h + dx + dy).rem_euclid(1.);
                    color = hsv.rgb(color.a);
                }
                Surface::Alpha => color.a = (color.a + dx + dy).clamp(0., 1.),
            }
            this.hsv = hsv;
            this.preview(color, cx);
            this.sync_inputs(window, cx);
            cx.stop_propagation();
        }))
}

fn normalized(bounds: Bounds<Pixels>, position: Point<Pixels>) -> wheel::Position {
    [
        (position.x - bounds.left()) / bounds.size.width.max(px(1.)),
        (position.y - bounds.top()) / bounds.size.height.max(px(1.)),
    ]
}

fn color_wheel(
    this: &mut ColorPickerState,
    window: &mut Window,
    cx: &mut Context<ColorPickerState>,
) -> impl IntoElement + use<> {
    let side = (compact::scaled(wheel::SIDE, cx).as_f32() * window.scale_factor())
        .ceil()
        .max(1.) as u32;
    let [ring, triangle_image] = this.wheel_cache.images(side, this.hsv.h, window, cx);
    let triangle = wheel::Triangle::new(this.hsv.h);
    let (min, max) = triangle.bounds();
    let weak = cx.entity().downgrade();
    let border = cx.theme().border;
    surface_frame(this, Surface::Hue, cx)
        .test_support()
        .relative()
        .w(compact::rem(wheel::SIDE))
        .self_center()
        .h(compact::rem(wheel::SIDE))
        .child(img(ring).absolute().inset_0().size_full())
        .child(img(triangle_image).absolute().inset_0().size_full())
        .child(
            surface_frame(this, Surface::Sv, cx)
                .test_support()
                .absolute()
                .left(relative(min[0]))
                .top(relative(min[1]))
                .w(relative(max[0] - min[0]))
                .h(relative(max[1] - min[1])),
        )
        .child(marker(
            wheel::hue_position(this.hsv.h),
            Hsv {
                h: this.hsv.h,
                s: 1.,
                v: 1.,
            }
            .rgb(1.),
        ))
        .child(
            marker(triangle.position(this.hsv.s, this.hsv.v), this.color)
                .id("color-picker-sv-handle")
                .test_support()
                .cursor(CursorStyle::Crosshair)
                // Keep the handle interactive even where it extends past the triangle.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        if this.disabled {
                            return;
                        }
                        this.begin(cx);
                        this.drag = Some(Surface::Sv);
                        window.focus(&this.surface_focus[Surface::Sv as usize], cx);
                        // Grabbing the handle must not change the color before dragging.
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                ),
        )
        .child(
            canvas(
                move |bounds, _, cx| {
                    if let Some(state) = weak.upgrade() {
                        state.update(cx, |state, _| {
                            state.bounds[Surface::Hue as usize] = bounds;
                            state.bounds[Surface::Sv as usize] = bounds;
                        });
                    }
                },
                move |bounds, _, window, _| {
                    let mut path = PathBuilder::stroke(px(0.7));
                    for (i, p) in triangle.vertices.into_iter().enumerate() {
                        let point = bounds.origin
                            + point(bounds.size.width * p[0], bounds.size.height * p[1]);
                        if i == 0 {
                            path.move_to(point);
                        } else {
                            path.line_to(point);
                        }
                    }
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, border);
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
}

fn alpha_bar(this: &ColorPickerState, cx: &Context<ColorPickerState>) -> impl IntoElement + use<> {
    let weak = cx.entity().downgrade();
    surface_frame(this, Surface::Alpha, cx)
        .test_support()
        .relative()
        .w_full()
        .h(compact::rem(10.))
        .bg(checkerboard(rgba(0x808080ff), 4.))
        .child(div().absolute().inset_0().bg(gradient(
            Rgba {
                a: 0.,
                ..this.color
            },
            Rgba {
                a: 1.,
                ..this.color
            },
            90.,
        )))
        .child(marker([this.color.a, 0.5], this.color))
        .child(
            canvas(
                move |bounds, _, cx| {
                    if let Some(state) = weak.upgrade() {
                        state.update(cx, |state, _| {
                            state.bounds[Surface::Alpha as usize] = bounds
                        });
                    }
                },
                |_, _, _, _| (),
            )
            .absolute()
            .inset_0(),
        )
}

impl ColorPickerState {
    fn pick(
        &mut self,
        kind: Surface,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.bounds[kind as usize];
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return;
        }
        let point = normalized(bounds, position);
        let mut color = self.color;
        match kind {
            Surface::Sv => {
                (self.hsv.s, self.hsv.v) = wheel::Triangle::new(self.hsv.h).sv(point, self.hsv.s);
                color = self.hsv.rgb(color.a);
            }
            Surface::Hue => {
                self.hsv.h = wheel::hue_at(point);
                color = self.hsv.rgb(color.a);
            }
            Surface::Alpha => color.a = point[0].clamp(0., 1.),
        }
        self.preview(color, cx);
        self.sync_inputs(window, cx);
    }
}

pub(super) fn render(
    this: &mut ColorPickerState,
    window: &mut Window,
    cx: &mut Context<ColorPickerState>,
) -> impl IntoElement {
    let mut controls = div().flex().flex_col().gap(compact::rem(compact::GAP));
    let labels = if this.mode == Mode::Rgb {
        [
            "color_picker.red",
            "color_picker.green",
            "color_picker.blue",
            "color_picker.alpha",
        ]
    } else {
        [
            "color_picker.hue",
            "color_picker.saturation",
            "color_picker.value",
            "color_picker.alpha",
        ]
    };
    for (i, label) in labels
        .into_iter()
        .enumerate()
        .take(if this.alpha { 4 } else { 3 })
    {
        let suffix = if i == 3 || (this.mode == Mode::Hsv && i > 0) {
            Some(SharedString::from("%"))
        } else if this.mode == Mode::Hsv {
            Some(SharedString::from("°"))
        } else {
            None
        };
        let label = t!(label).into_owned();
        let options = this.number_options(i);
        this.numbers[i].update(cx, |number, cx| {
            number.set_suffix(suffix, cx);
            number.configure(label.clone(), options, cx);
        });
        controls = controls.child(
            div()
                .flex()
                .items_center()
                .h(compact::rem(compact::ROW))
                .gap(compact::rem(compact::PADDING))
                .min_w_0()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "color-picker-channel-label-{i}"
                        )))
                        .test_support()
                        .w(compact::rem(70.))
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("color-picker-channel-{i}")))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .when(this.invalid[i + 2], |el| {
                            el.border_1().border_color(cx.theme().danger)
                        })
                        .child(this.numbers[i].clone()),
                ),
        );
    }
    let wheel = color_wheel(this, window, cx);
    let weak = cx.entity().downgrade();
    div()
        .id("color-picker-panel")
        .test_support()
        .track_focus(&this.focus)
        .w(compact::rem(wheel::SIDE + compact::PADDING * 2.))
        .p(compact::rem(compact::PADDING))
        .flex()
        .flex_col()
        .gap(compact::rem(compact::PADDING))
        .text_size(compact::rem(compact::BODY))
        .text_color(cx.theme().foreground)
        .on_action(
            cx.listener(|this, _: &gpui_kit::base::actions::Cancel, _, cx| {
                this.cancel(cx);
                cx.stop_propagation();
            }),
        )
        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
            if this.composing(window, cx) {
                return;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    this.cancel(cx);
                    cx.stop_propagation();
                }
                "enter" => {
                    this.accept(window, cx);
                    cx.stop_propagation();
                }
                _ => {}
            }
        }))
        .child(wheel)
        .when(this.alpha, |panel| panel.child(alpha_bar(this, cx)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(compact::rem(compact::PADDING))
                .child(
                    div().flex().gap(compact::rem(compact::GAP)).children(
                        [
                            (this.original.0, "color_picker.original"),
                            (this.color, "color_picker.current"),
                        ]
                        .map(|(color, label)| {
                            let label = t!(label).into_owned();
                            swatch(color)
                                .id(SharedString::from(format!("color-picker-swatch-{label}")))
                                .w(compact::rem(20.))
                                .h(compact::rem(compact::NUMBER_CONTROL))
                                .rounded(compact::rem(compact::RADIUS))
                                .border_1()
                                .border_color(cx.theme().border)
                                .aria_label(label.clone())
                                .tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(label.clone())
                                        .build(window, cx)
                                })
                        }),
                    ),
                )
                .child(
                    div()
                        .text_size(compact::rem(compact::CAPTION))
                        .text_color(cx.theme().muted_foreground)
                        .child("HEX"),
                )
                .child(
                    div()
                        .id("color-picker-hex")
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .child(
                            compact::input(Input::new(&this.inputs[1]))
                                .map(|input| {
                                    Styled::h(input, compact::rem(compact::NUMBER_CONTROL))
                                })
                                .disabled(this.disabled)
                                .aria_label("HEX")
                                .font_family(".AppleSystemUIFontMonospaced")
                                .when(this.invalid[1], |input| {
                                    input.border_color(cx.theme().danger)
                                }),
                        ),
                ),
        )
        .child(
            Tabs::new("color-picker-mode-tabs")
                .flex()
                .w_full()
                .h(compact::rem(compact::NUMBER_CONTROL))
                .rounded(compact::rem(compact::RADIUS))
                .bg(cx.theme().input_background())
                .gap(compact::rem(compact::GAP))
                .children(
                    [("RGB", Mode::Rgb), ("HSV", Mode::Hsv)].map(|(label, mode)| {
                        Tab::new(label)
                            .flex_1()
                            .h_full()
                            .rounded(compact::rem(compact::RADIUS))
                            .text_size(compact::rem(compact::BODY))
                            .text_color(cx.theme().muted_foreground)
                            .selected(this.mode == mode)
                            .accessibility_label(label)
                            .set_position(if mode == Mode::Rgb { 1 } else { 2 }, 2)
                            .when(this.mode == mode, |tab| {
                                tab.bg(cx.theme().foreground.opacity(0.10))
                                    .text_color(cx.theme().foreground)
                            })
                            .when(this.mode != mode && !this.disabled, |tab| {
                                tab.cursor_pointer()
                                    .hover(|style| style.bg(cx.theme().foreground.opacity(0.05)))
                            })
                            .when(this.disabled, |tab| tab.opacity(0.5))
                            .disabled(this.disabled)
                            .child(label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.settle_drafts(cx);
                                this.mode = mode;
                                this.reset_inputs = true;
                                window.focus(&this.focus, cx);
                                this.sync_inputs(window, cx);
                                cx.notify();
                            }))
                    }),
                ),
        )
        .child(controls)
        .when(this.invalid.iter().any(|v| *v), |panel| {
            panel.child(
                div()
                    .text_color(cx.theme().danger)
                    .child(t!("color_picker.invalid")),
            )
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(compact::rem(compact::PADDING))
                .child(
                    div()
                        .flex()
                        .gap(compact::rem(compact::GAP))
                        .child(
                            compact::button(Button::new("color-picker-cancel"))
                                .h(compact::rem(compact::NUMBER_CONTROL))
                                .ghost()
                                .label(t!("color_picker.cancel"))
                                .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                        )
                        .child(
                            compact::button(Button::new("color-picker-done"))
                                .h(compact::rem(compact::NUMBER_CONTROL))
                                .primary()
                                .label(t!("color_picker.done"))
                                .disabled(this.disabled)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.accept(window, cx)),
                                ),
                        ),
                ),
        )
        .child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let moving = weak.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = moving.update(cx, |this, cx| {
                                if let Some(kind) = this.drag {
                                    if event.pressed_button == Some(MouseButton::Left) {
                                        this.pick(kind, event.position, window, cx);
                                    } else {
                                        this.drag = None;
                                    }
                                    cx.stop_propagation();
                                }
                            });
                        }
                    });
                    let releasing = weak.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                            let _ = releasing.update(cx, |this, _| this.drag = None);
                        }
                    });
                },
            )
            .absolute()
            .size_0(),
        )
}
