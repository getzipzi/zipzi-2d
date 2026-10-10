use gpui_kit::base::NumberInput;
use gpui_kit::component::{
    ActiveTheme, Sizable,
    input::{Input, InputState},
};
use gpui_kit::{prelude::*, *};

const DRAG_THRESHOLD: f32 = 3.;
const PIXELS_PER_STEP: f64 = 8.;

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct SlideNumberOptions {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Full min-to-max distance in logical pixels. None uses `step` per 8px.
    pub drag_distance: Option<f64>,
    pub disabled: bool,
}

impl SlideNumberOptions {
    fn slide_scale(self) -> (f64, f64) {
        match self.drag_distance {
            Some(distance) if distance.is_finite() && distance > 0. && self.min < self.max => {
                ((self.max - self.min) / distance, 1.)
            }
            _ => (self.step, PIXELS_PER_STEP),
        }
    }
}

/// Values are controlled by the caller, including restoration on Escape.
pub(crate) struct SlideNumberChange(pub f64);

struct Drag {
    origin: Point<Pixels>,
    last_x: Pixels,
    original: f64,
    anchor: f64,
    step: f64,
    value: f64,
    sliding: bool,
}

/// The parameter panel's compact number editor, with relative horizontal scrubbing.
/// A click enters text editing; an already focused editor keeps text selection.
pub(crate) struct SlideNumberInput {
    input: Entity<InputState>,
    label: SharedString,
    options: SlideNumberOptions,
    focus: FocusHandle,
    drag: Option<Drag>,
    truncate_to_integer: bool,
    _observe: Subscription,
    _activation: Subscription,
}

impl EventEmitter<SlideNumberChange> for SlideNumberInput {}

impl SlideNumberInput {
    pub fn new(
        input: Entity<InputState>,
        label: impl Into<SharedString>,
        options: SlideNumberOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            _observe: cx.observe(&input, |_, _, cx| cx.notify()),
            _activation: cx.observe_window_activation(window, |this: &mut Self, window, cx| {
                if !window.is_window_active() && this.drag.take().is_some() {
                    cx.notify();
                }
            }),
            input,
            label: label.into(),
            options,
            focus: cx.focus_handle().tab_stop(false),
            drag: None,
            truncate_to_integer: false,
        }
    }

    /// Keep fractional drag movement, but emit and display only truncated integers.
    pub fn truncate_to_integer(mut self) -> Self {
        self.truncate_to_integer = true;
        self
    }

    pub fn configure(
        &mut self,
        label: impl Into<SharedString>,
        options: SlideNumberOptions,
        cx: &mut Context<Self>,
    ) {
        let label = label.into();
        if self.options != options || self.label != label {
            self.options = options;
            self.label = label;
            if options.disabled {
                self.drag = None;
            }
            cx.notify();
        }
    }

    fn begin(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left
            || self.options.disabled
            || self.input.read(cx).focus_handle(cx).is_focused(window)
            || !self.options.value.is_finite()
            || !self.options.step.is_finite()
            || self.options.step <= 0.
            || self.options.min > self.options.max
        {
            return;
        }
        self.drag = Some(Drag {
            origin: event.position,
            last_x: event.position.x,
            original: self.options.value,
            anchor: self.options.value,
            step: self.options.slide_scale().0 * if event.modifiers.shift { 0.1 } else { 1. },
            value: self.options.value,
            sliding: false,
        });
        window.focus(&self.focus, cx);
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    fn move_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        let Some(drag) = &mut self.drag else {
            return false;
        };
        if self.options.disabled || event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            cx.notify();
            return false;
        }
        if !drag.sliding {
            if (event.position.x - drag.origin.x).abs() < px(DRAG_THRESHOLD) {
                return true;
            }
            drag.sliding = true;
        }
        let (base_step, pixels_per_step) = self.options.slide_scale();
        let step = base_step * if event.modifiers.shift { 0.1 } else { 1. };
        if step != drag.step {
            // Changing precision starts at the current value rather than
            // snapping back to the previous step's grid.
            drag.anchor = self.options.value;
            drag.value = self.options.value;
            drag.step = step;
        }
        let delta = (event.position.x - drag.last_x).as_f32() as f64;
        drag.last_x = event.position.x;
        // Clamp the accumulator too, so reversing at a limit responds immediately.
        drag.value =
            (drag.value + delta / pixels_per_step * step).clamp(self.options.min, self.options.max);
        if drag.value == self.options.min || drag.value == self.options.max {
            drag.anchor = drag.value;
        }
        let value = if self.truncate_to_integer {
            drag.value.trunc()
        } else {
            snap_value(
                drag.value,
                drag.anchor,
                step,
                self.options.min,
                self.options.max,
            )
        };
        if value != self.options.value {
            self.options.value = value;
            cx.emit(SlideNumberChange(value));
        }
        cx.notify();
        true
    }

    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        if !drag.sliding && !self.options.disabled {
            self.input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            });
        }
        cx.notify();
        true
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take() {
            if drag.sliding && self.options.value != drag.original {
                self.options.value = drag.original;
                cx.emit(SlideNumberChange(drag.original));
            }
            cx.notify();
        }
    }
}

fn snap_value(value: f64, origin: f64, step: f64, min: f64, max: f64) -> f64 {
    // Preserve endpoints even when they are not multiples of the step.
    if value <= min || value >= max {
        return value.clamp(min, max);
    }
    let snapped = (origin + ((value - origin) / step).round() * step).clamp(min, max);
    // Avoid exposing binary arithmetic noise in the text editor.
    format!("{snapped:.12}")
        .parse::<f64>()
        .unwrap_or(snapped)
        .clamp(min, max)
}

impl Render for SlideNumberInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editing = self.input.read(cx).focus_handle(cx).is_focused(window);
        let disabled = self.options.disabled;
        let sliding = self.drag.as_ref().is_some_and(|drag| drag.sliding);
        let theme = cx.theme().clone();
        let entity = cx.entity().downgrade();
        div()
            .id("slide-number-input")
            .test_support()
            .relative()
            .w_full()
            .track_focus(&self.focus)
            .capture_any_mouse_down(cx.listener(Self::begin))
            .on_action(
                cx.listener(|this, _: &gpui_kit::component::dialog::Cancel, _, cx| {
                    if this.drag.is_some() {
                        this.cancel(cx);
                        cx.stop_propagation();
                    } else {
                        cx.propagate();
                    }
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.drag.is_some() {
                    this.cancel(cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                NumberInput::new(&self.input)
                    .w_full()
                    .disabled(disabled)
                    .decrement_button(|button| button.hidden())
                    .increment_button(|button| button.hidden())
                    .input(
                        Input::new(&self.input)
                            .small()
                            .text_xs()
                            .disabled(disabled)
                            .aria_label(self.label.clone()),
                    ),
            )
            .when(!editing, |element| {
                element.child(
                    div()
                        .id("slide-number-surface")
                        .absolute()
                        .inset_0()
                        // Show fine adjustments while dragging; the idle text retains
                        // the caller's compact formatting.
                        .when(sliding, |surface| {
                            surface
                                .bg(theme.input_background())
                                .rounded(theme.radius)
                                .border_1()
                                .border_color(theme.ring)
                                .px_2()
                                .flex()
                                .items_center()
                                .text_xs()
                                .text_color(theme.foreground)
                                .overflow_hidden()
                                .child((self.options.value as f32).to_string())
                        })
                        .cursor(if disabled {
                            CursorStyle::Arrow
                        } else {
                            CursorStyle::ResizeLeftRight
                        }),
                )
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let moving = entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && moving
                                    .update(cx, |this, cx| this.move_drag(event, cx))
                                    .unwrap_or(false)
                            {
                                cx.stop_propagation();
                            }
                        });
                        let releasing = entity.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture
                                && event.button == MouseButton::Left
                                && releasing
                                    .update(cx, |this, cx| this.release(window, cx))
                                    .unwrap_or(false)
                            {
                                cx.stop_propagation();
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            )
    }
}
