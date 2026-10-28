use super::compact;
use gpui_kit::base::{NumberInput, StepAction, step_value};
use gpui_kit::component::{
    ActiveTheme, Sizable,
    input::{Input, InputState},
};
use gpui_kit::{prelude::*, *};

const DRAG_THRESHOLD: f32 = 3.;
const PIXELS_PER_STEP: f64 = 8.;

/// Truncate the shortest decimal text, preserving authored f32 values like 0.7.
#[allow(dead_code)]
pub(crate) fn format_one_decimal(value: f64) -> String {
    let text = (value as f32).to_string();
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, "0"));
    let decimal = fraction.chars().next().unwrap_or('0');
    if whole == "-0" && decimal == '0' {
        "0.0".into()
    } else {
        format!("{whole}.{decimal}")
    }
}

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

/// Explicit ownership boundaries for callers that preview document candidates.
pub(crate) enum SlideNumberGesture {
    Begin,
    End,
    Cancel,
}

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
    suffix: Option<SharedString>,
    options: SlideNumberOptions,
    focus: FocusHandle,
    drag: Option<Drag>,
    truncate_factor: Option<f64>,
    snap_to_step: bool,
    xsmall: bool,
    sliding_value_format: fn(f64) -> String,
    ignore_mouse_move: Option<fn() -> bool>,
    _observe: Subscription,
    _activation: Subscription,
}

impl EventEmitter<SlideNumberChange> for SlideNumberInput {}
impl EventEmitter<SlideNumberGesture> for SlideNumberInput {}

impl SlideNumberInput {
    #[cfg(feature = "test-support")]
    pub(crate) fn benchmark_value(&self) -> f64 {
        self.options.value
    }

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
                    cx.emit(SlideNumberGesture::Cancel);
                    cx.notify();
                }
            }),
            input,
            label: label.into(),
            suffix: None,
            options,
            focus: cx.focus_handle().tab_stop(false),
            drag: None,
            truncate_factor: None,
            snap_to_step: false,
            xsmall: false,
            sliding_value_format: |value| (value as f32).to_string(),
            ignore_mouse_move: None,
        }
    }

    pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn xsmall(mut self) -> Self {
        self.xsmall = true;
        self
    }

    pub fn set_suffix(&mut self, suffix: Option<SharedString>, cx: &mut Context<Self>) {
        if self.suffix != suffix {
            self.suffix = suffix;
            cx.notify();
        }
    }

    /// Keep fractional drag movement, but emit and display only truncated integers.
    #[allow(dead_code)]
    pub fn truncate_to_integer(mut self) -> Self {
        self.truncate_factor = Some(1.);
        self
    }

    /// Retain fractional drag movement while emitting tenths, without rounding.
    pub fn truncate_to_one_decimal(mut self) -> Self {
        self.truncate_factor = Some(10.);
        self.sliding_value_format = |value| format!("{value:.1}");
        self
    }

    /// Honor configured increments while retaining truncation precision. Shift
    /// cannot select a finer increment than the emitted decimal/integer value.
    pub fn snap_to_step(mut self) -> Self {
        self.snap_to_step = true;
        self
    }

    fn drag_scale(&self, shift: bool) -> (f64, f64) {
        let (step, distance) = self.options.slide_scale();
        let step = step * if shift { 0.1 } else { 1. };
        let step = if self.snap_to_step {
            self.truncate_factor
                .map_or(step, |factor| step.max(1. / factor))
        } else {
            step
        };
        (step, distance)
    }

    fn step_keyboard(&mut self, action: StepAction, shift: bool, cx: &mut Context<Self>) {
        if self.options.disabled {
            return;
        }
        let text = self.input.read(cx).value();
        let Some(text) = step_value(
            &text,
            action,
            self.drag_scale(shift).0,
            Some(self.options.min),
            Some(self.options.max),
        ) else {
            return;
        };
        let Ok(value) = text.parse::<f64>() else {
            return;
        };
        let value = self
            .truncate_factor
            .map_or(value, |factor| (value * factor).trunc() / factor);
        if value != self.options.value {
            self.options.value = value;
            cx.emit(SlideNumberChange(value));
            cx.notify();
        }
    }

    /// Format the drag display without changing the emitted numeric value.
    #[allow(dead_code)]
    pub fn format_sliding_value(mut self, format: fn(f64) -> String) -> Self {
        self.sliding_value_format = format;
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
            // The caller may round the emitted f64 to its stored f32. Updating
            // that authoritative value must not schedule another frame when
            // the text already on screen is identical.
            let repaint = self.label != label
                || self.options.disabled != options.disabled
                || (self.is_sliding()
                    && (self.sliding_value_format)(self.options.value)
                        != (self.sliding_value_format)(options.value));
            if let Some(drag) = &mut self.drag {
                if drag.sliding
                    && (self.sliding_value_format)(self.options.value)
                        != (self.sliding_value_format)(options.value)
                {
                    drag.value = options.value;
                    drag.anchor = options.value;
                }
            }
            self.options = options;
            self.label = label;
            if options.disabled {
                if self.drag.take().is_some() {
                    cx.emit(SlideNumberGesture::Cancel);
                }
            }
            if repaint {
                cx.notify();
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_sliding(&self) -> bool {
        self.drag.as_ref().is_some_and(|drag| drag.sliding)
    }

    /// Isolate injected drags from unrelated native hover moves in benchmarks.
    /// Normal editor controls leave this unset.
    #[allow(dead_code)]
    pub(crate) fn ignore_mouse_moves_when(&mut self, predicate: Option<fn() -> bool>) {
        self.ignore_mouse_move = predicate;
    }

    /// Stop an owning transaction without emitting a new numeric edit.
    #[allow(dead_code)]
    pub(crate) fn reset_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
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
            step: self.drag_scale(event.modifiers.shift).0,
            value: self.options.value,
            sliding: false,
        });
        window.focus(&self.focus, cx);
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    fn move_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        if self.ignore_mouse_move.is_some_and(|ignore| ignore()) {
            return false;
        }
        let (step, pixels_per_step) = self.drag_scale(event.modifiers.shift);
        let Some(drag) = &mut self.drag else {
            return false;
        };
        if self.options.disabled || event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            cx.emit(SlideNumberGesture::Cancel);
            cx.notify();
            return false;
        }
        let started = !drag.sliding;
        if !drag.sliding {
            if (event.position.x - drag.origin.x).abs() < px(DRAG_THRESHOLD) {
                return true;
            }
            drag.sliding = true;
        }
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
        let value = if self.snap_to_step || self.truncate_factor.is_none() {
            snap_value(
                drag.value,
                drag.anchor,
                step,
                self.options.min,
                self.options.max,
            )
        } else {
            drag.value
        };
        let value = if let Some(factor) = self.truncate_factor {
            (value * factor).trunc() / factor
        } else {
            value
        };
        let changed = value != self.options.value;
        let repaint = started
            || (changed
                && (self.sliding_value_format)(self.options.value)
                    != (self.sliding_value_format)(value));
        if started {
            cx.emit(SlideNumberGesture::Begin);
        }
        if changed {
            self.options.value = value;
            cx.emit(SlideNumberChange(value));
        }
        if repaint {
            cx.notify();
        }
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
        if drag.sliding {
            cx.emit(SlideNumberGesture::End);
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
            cx.emit(SlideNumberGesture::Cancel);
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
            .h(compact::rem(compact::NUMBER_CONTROL))
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
            .when(self.snap_to_step, |element| {
                element
                    .capture_action(cx.listener(
                        |this, _: &gpui_kit::base::input::MoveUp, window, cx| {
                            this.step_keyboard(StepAction::Increment, window.modifiers().shift, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .capture_action(cx.listener(
                        |this, _: &gpui_kit::base::input::MoveDown, window, cx| {
                            this.step_keyboard(StepAction::Decrement, window.modifiers().shift, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .capture_action(cx.listener(
                        |this, _: &gpui_kit::base::Increment, window, cx| {
                            this.step_keyboard(StepAction::Increment, window.modifiers().shift, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .capture_action(cx.listener(
                        |this, _: &gpui_kit::base::Decrement, window, cx| {
                            this.step_keyboard(StepAction::Decrement, window.modifiers().shift, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .capture_action(cx.listener(
                        |this, _: &gpui_kit::base::actions::SelectUp, _, cx| {
                            this.step_keyboard(StepAction::Increment, true, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .capture_action(cx.listener(
                        |this, _: &gpui_kit::base::actions::SelectDown, _, cx| {
                            this.step_keyboard(StepAction::Decrement, true, cx);
                            cx.stop_propagation();
                        },
                    ))
            })
            // The drag surface replaces the editor: its translucent background
            // cannot hide the editor's text underneath it.
            .when(!sliding, |element| {
                element.child(
                    NumberInput::new(&self.input)
                        .w_full()
                        .h_full()
                        .disabled(disabled)
                        .decrement_button(|button| button.hidden())
                        .increment_button(|button| button.hidden())
                        .input(
                            compact::input(Input::new(&self.input))
                                .when(self.xsmall, |input| input.xsmall())
                                .map(|input| {
                                    Styled::h(input, compact::rem(compact::NUMBER_CONTROL))
                                })
                                .line_height(compact::rem(14.))
                                .text_align(TextAlign::Right)
                                .font_family(".AppleSystemUIFontMonospaced")
                                .when_some(self.suffix.clone(), |input, suffix| {
                                    input.suffix(suffix)
                                })
                                .disabled(disabled)
                                .aria_label(self.label.clone()),
                        ),
                )
            })
            .when(!editing, |element| {
                element.child(
                    div()
                        .id("slide-number-surface")
                        .absolute()
                        .inset_0()
                        // The caller controls drag formatting separately from precision.
                        .when(sliding, |surface| {
                            surface
                                .bg(theme.input_background())
                                .rounded(compact::rem(compact::RADIUS))
                                .border_1()
                                .border_color(theme.ring)
                                .px(compact::rem(4.))
                                .flex()
                                .items_center()
                                .justify_end()
                                .text_size(compact::rem(compact::BODY))
                                .font_family(".AppleSystemUIFontMonospaced")
                                .text_color(theme.foreground)
                                .overflow_hidden()
                                .child((self.sliding_value_format)(self.options.value))
                                .when_some(self.suffix.clone(), |surface, suffix| {
                                    surface.gap(px(4.)).pr(px(8.)).child(suffix)
                                })
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
