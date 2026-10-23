//! Project color controls. Colors stay in the caller's RGB encoding; HSV is UI state only.
//! A session emits Begin, zero or more Preview events, then Commit or Cancel.
//! Programmatic synchronization never emits edits. The caller owns document history.
mod color;
mod field;
mod panel;
#[cfg(test)]
mod tests;
mod wheel;

pub(crate) use field::ColorField;
pub(crate) use panel::ColorPickerPanel;

use super::slide_number_input::{
    SlideNumberChange, SlideNumberGesture, SlideNumberInput, SlideNumberOptions,
};
use color::Hsv;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{prelude::*, *};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ColorPickerEvent {
    Begin,
    Preview(Rgba),
    Commit(Rgba),
    Cancel,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Rgb,
    Hsv,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    Sv = 0,
    Hue = 1,
    Alpha = 2,
}

pub(crate) struct ColorPickerState {
    color: Rgba,
    hsv: Hsv,
    original: (Rgba, Hsv),
    alpha: bool,
    mode: Mode,
    editing: bool,
    open: bool,
    disabled: bool,
    label: SharedString,
    focus: FocusHandle,
    surface_focus: [FocusHandle; 3],
    // Inline HEX, panel HEX, then three color channels and optional alpha.
    inputs: [Entity<InputState>; 6],
    displayed: [String; 6],
    invalid: [bool; 6],
    reset_inputs: bool,
    numbers: Vec<Entity<SlideNumberInput>>,
    drag: Option<Surface>,
    bounds: [Bounds<Pixels>; 3],
    wheel_cache: wheel::WheelCache,
    #[cfg(feature = "test-support")]
    benchmark_pointer: bool,
    _events: Vec<Subscription>,
}

impl EventEmitter<ColorPickerEvent> for ColorPickerState {}

impl ColorPickerState {
    #[cfg(feature = "test-support")]
    pub(crate) fn benchmark_isolate_pointer(&mut self, cx: &mut Context<Self>) {
        self.benchmark_pointer = true;
        for number in &self.numbers {
            number.update(cx, |number, _| {
                number.ignore_mouse_moves_when(Some(crate::benchmark::ignore_native_pointer_event))
            });
        }
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn benchmark_color(&self) -> Rgba {
        self.color
    }

    /// Enable `alpha` for RGBA controls; RGB controls preserve the supplied alpha.
    pub fn new(color: Rgba, alpha: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hsv = Hsv::from_rgb(color, Hsv::default());
        let inputs = std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx)));
        let mut this = Self {
            color,
            hsv,
            original: (color, hsv),
            alpha,
            mode: Mode::Rgb,
            editing: false,
            open: false,
            disabled: false,
            label: "Color".into(),
            focus: cx.focus_handle(),
            surface_focus: std::array::from_fn(|_| cx.focus_handle()),
            inputs,
            displayed: Default::default(),
            invalid: [false; 6],
            reset_inputs: true,
            numbers: vec![],
            drag: None,
            bounds: Default::default(),
            wheel_cache: Default::default(),
            #[cfg(feature = "test-support")]
            benchmark_pointer: false,
            _events: vec![],
        };
        cx.on_release(|this, cx| this.wheel_cache.clear(cx))
            .detach();
        for i in 0..6 {
            this._events.push(cx.subscribe_in(
                &this.inputs[i],
                window,
                move |this, source, event, window, cx| match event {
                    InputEvent::Change => {
                        let raw = source.read(cx).value().to_string();
                        if raw != this.displayed[i] && !this.disabled {
                            this.begin(cx);
                            this.displayed[i] = raw.clone();
                            this.edit_text(i, &raw, cx);
                            this.sync_inputs(window, cx);
                        }
                    }
                    InputEvent::PressEnter { .. } if !this.composing(window, cx) => {
                        this.accept(window, cx)
                    }
                    InputEvent::Blur => {
                        if !this.open {
                            this.accept(window, cx);
                        } else {
                            this.sync_inputs(window, cx);
                        }
                    }
                    _ => {}
                },
            ));
        }
        for i in 0..4 {
            let options = this.number_options(i);
            let number = cx.new(|cx| {
                SlideNumberInput::new(
                    this.inputs[i + 2].clone(),
                    "Color channel",
                    options,
                    window,
                    cx,
                )
                .format_sliding_value(|value| color::number(value as f32))
            });
            this._events.push(cx.subscribe(
                &number,
                move |this, _, event: &SlideNumberGesture, cx| match event {
                    SlideNumberGesture::Begin => this.begin(cx),
                    SlideNumberGesture::Cancel => this.cancel(cx),
                    SlideNumberGesture::End => {}
                },
            ));
            this._events.push(cx.subscribe_in(
                &number,
                window,
                move |this, _, event: &SlideNumberChange, window, cx| {
                    if !this.disabled {
                        this.begin(cx);
                        this.edit_channel(i, event.0 as f32, cx);
                        this.sync_inputs(window, cx);
                    }
                },
            ));
            this.numbers.push(number);
        }
        this._events
            .push(cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() && this.editing {
                    this.cancel(cx);
                }
            }));
        this.sync_inputs(window, cx);
        this
    }

    pub fn value(&self) -> Rgba {
        self.color
    }
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The inline HEX field follows text submission rules at object boundaries.
    /// An open popup retains its separate accept/cancel lifecycle.
    pub fn inline_text(&self, window: &Window, cx: &App) -> Option<String> {
        (!self.open && self.inputs[0].read(cx).focus_handle(cx).is_focused(window))
            .then(|| self.inputs[0].read(cx).value().to_string())
    }

    pub fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
            || self
                .inputs
                .iter()
                .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Reconcile caller data without quantizing it through HEX or HSV.
    pub fn sync(
        &mut self,
        color: Rgba,
        label: impl Into<SharedString>,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = label.into();
        let changed = self.label != label || self.disabled != disabled;
        self.label = label;
        self.disabled = disabled;
        if disabled && self.editing {
            self.cancel(cx);
        }
        if !self.editing && self.color != color {
            self.color = color;
            self.hsv = Hsv::from_rgb(color, self.hsv);
            self.original = (color, self.hsv);
            self.reset_inputs = true;
            self.sync_inputs(window, cx);
            cx.notify();
        } else if changed {
            cx.notify();
        }
    }

    fn begin(&mut self, cx: &mut Context<Self>) {
        if !self.editing && !self.disabled {
            self.original = (self.color, self.hsv);
            self.editing = true;
            cx.emit(ColorPickerEvent::Begin);
        }
    }

    fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open && !self.disabled {
            self.settle_drafts(cx);
            self.begin(cx);
            self.open = true;
            window.focus(&self.focus, cx);
            self.sync_inputs(window, cx);
            cx.notify();
        } else if !open && self.open {
            self.accept(window, cx);
        }
    }

    pub fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || self.composing(window, cx) {
            return;
        }
        self.settle_drafts(cx);
        // Unfinished input never overwrites the last valid preview.
        if !self.open && self.invalid[0] {
            self.cancel(cx);
        } else if self.editing {
            self.editing = false;
            self.open = false;
            self.drag = None;
            self.invalid = [false; 6];
            self.original = (self.color, self.hsv);
            cx.emit(ColorPickerEvent::Commit(self.color));
        }
        self.reset_inputs = true;
        self.sync_inputs(window, cx);
        cx.notify();
    }

    fn reset_to(&mut self, color: Rgba, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.begin(cx);
        self.invalid = [false; 6];
        self.hsv = Hsv::from_rgb(color, self.hsv);
        self.preview(color, cx);
        // Replace pending/invalid text before accepting so it cannot overwrite reset.
        self.reset_inputs = true;
        self.sync_inputs(window, cx);
        self.accept(window, cx);
    }

    // Input events are queued; a blur, format switch or Enter may precede them.
    fn settle_drafts(&mut self, cx: &mut Context<Self>) {
        for i in 0..self.inputs.len() {
            let raw = self.inputs[i].read(cx).value().to_string();
            if raw != self.displayed[i] {
                self.begin(cx);
                self.displayed[i] = raw.clone();
                self.edit_text(i, &raw, cx);
            }
        }
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        let editing = self.editing;
        self.cancel_silently(cx);
        if editing {
            cx.emit(ColorPickerEvent::Cancel);
        }
    }

    /// Used when the owner has already cancelled or invalidated its transaction.
    pub fn cancel_silently(&mut self, cx: &mut Context<Self>) {
        if self.editing || self.open {
            (self.color, self.hsv) = self.original;
            self.editing = false;
            self.open = false;
            self.drag = None;
            self.invalid = [false; 6];
            for number in &self.numbers {
                number.update(cx, |number, cx| number.reset_drag(cx));
            }
        }
        self.reset_inputs = true;
        cx.notify();
    }

    pub fn composing(&self, window: &mut Window, cx: &mut App) -> bool {
        self.inputs.iter().any(|input| {
            input.read(cx).focus_handle(cx).is_focused(window)
                && input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
        })
    }

    fn preview(&mut self, color: Rgba, cx: &mut Context<Self>) {
        if color != self.color {
            self.color = color;
            cx.emit(ColorPickerEvent::Preview(color));
        }
        cx.notify();
    }

    fn edit_text(&mut self, index: usize, text: &str, cx: &mut Context<Self>) {
        if index < 2 {
            if let Some(color) = color::parse_hex(text, self.alpha, self.color.a) {
                self.invalid[index] = false;
                // HEX is a rounded presentation of the authored floats. Retyping
                // that presentation (including shorthand/case changes) is a no-op.
                if color::hex(color, self.alpha) != color::hex(self.color, self.alpha) {
                    self.hsv = Hsv::from_rgb(color, self.hsv);
                    self.preview(color, cx);
                } else {
                    cx.notify();
                }
            } else {
                self.invalid[index] = true;
                cx.notify();
            }
        } else {
            let channel = index - 2;
            let options = self.number_options(channel);
            if let Some(value) = text
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0. && *v <= options.max as f32)
            {
                self.invalid[index] = false;
                // RGB/HSV channel labels are rounded too; formatting-only edits
                // must not round-trip the underlying color through that display.
                let displayed = color::number(options.value as f32).parse::<f32>().ok();
                if displayed != Some(value) {
                    self.edit_channel(channel, value, cx);
                } else {
                    cx.notify();
                }
            } else {
                self.invalid[index] = true;
                cx.notify();
            }
        }
    }

    fn edit_channel(&mut self, channel: usize, value: f32, cx: &mut Context<Self>) {
        let mut color = self.color;
        if channel == 3 {
            color.a = value / 100.;
        } else if self.mode == Mode::Rgb {
            match channel {
                0 => color.r = value / 255.,
                1 => color.g = value / 255.,
                _ => color.b = value / 255.,
            }
            self.hsv = Hsv::from_rgb(color, self.hsv);
        } else {
            match channel {
                0 => self.hsv.h = value / 360.,
                1 => self.hsv.s = value / 100.,
                _ => self.hsv.v = value / 100.,
            }
            color = self.hsv.rgb(color.a);
        }
        self.preview(color, cx);
    }

    fn number_options(&self, channel: usize) -> SlideNumberOptions {
        let (value, max) = if channel == 3 {
            (self.color.a * 100., 100.)
        } else if self.mode == Mode::Rgb {
            (
                [self.color.r, self.color.g, self.color.b][channel] * 255.,
                255.,
            )
        } else {
            (
                [self.hsv.h * 360., self.hsv.s * 100., self.hsv.v * 100.][channel],
                if channel == 0 { 360. } else { 100. },
            )
        };
        SlideNumberOptions {
            value: value as f64,
            min: 0.,
            max,
            step: 1.,
            drag_distance: None,
            disabled: self.disabled,
        }
    }

    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for i in 0..6 {
            if !self.reset_inputs
                && (self.inputs[i].read(cx).focus_handle(cx).is_focused(window)
                    || self.inputs[i].read(cx).value().as_ref() != self.displayed[i])
            {
                continue;
            }
            let text = if i < 2 {
                color::hex(self.color, self.alpha)
            } else {
                color::number(self.number_options(i - 2).value as f32)
            };
            self.displayed[i] = text.clone();
            self.invalid[i] = false;
            if self.inputs[i].read(cx).value().as_ref() != text {
                self.inputs[i].update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
        self.reset_inputs = false;
    }
}

impl Render for ColorPickerState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        panel::render(self, window, cx)
    }
}
