use super::*;
use crate::ui::compact;
use gpui_kit::IntoElement;
use gpui_kit::component::{
    ActiveTheme, Disableable,
    button::{Button, ButtonVariants},
    input::Input,
    popover::Popover,
};
use std::{cell::Cell, rc::Rc};

/// Compact form entry, backed by the same state as the embeddable picker panel.
#[derive(IntoElement)]
pub(crate) struct ColorField {
    id: SharedString,
    input_id: SharedString,
    state: Entity<ColorPickerState>,
    reset_color: Option<Rgba>,
}

impl ColorField {
    pub fn new(id: impl Into<SharedString>, state: &Entity<ColorPickerState>) -> Self {
        let id = id.into();
        Self {
            input_id: format!("{id}-input").into(),
            id,
            state: state.clone(),
            reset_color: None,
        }
    }

    pub fn input_id(mut self, id: impl Into<SharedString>) -> Self {
        self.input_id = id.into();
        self
    }

    /// Add a suffix button that restores the caller's default color.
    pub fn reset_color(mut self, color: Rgba) -> Self {
        self.reset_color = Some(color);
        self
    }
}

impl RenderOnce for ColorField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let observed = self.state.clone();
        let parent = window.current_view();
        let _observer = window.use_keyed_state(
            ("color-field-observer", self.state.entity_id()),
            cx,
            move |window, cx| window.observe(&observed, cx, move |_, _, cx| cx.notify(parent)),
        );
        self.state
            .update(cx, |state, cx| state.sync_inputs(window, cx));
        let state = self.state.read(cx);
        let (color, open, disabled, label, focus, input, invalid) = (
            state.value(),
            state.is_open(),
            state.disabled,
            state.label.clone(),
            state.focus.clone(),
            state.inputs[0].clone(),
            state.invalid[0],
        );
        let popup_state = self.state.clone();
        let cancel_state = self.state.clone();
        let accept_state = self.state.clone();
        let blur_state = self.state.clone();
        let reset_state = self.state.clone();
        let reset_id = SharedString::from(format!("{}-reset", self.id));
        let reset_bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
        let reset_target = reset_bounds.clone();
        div()
            .flex()
            .w_full()
            .min_w_0()
            .gap(compact::rem(compact::GAP))
            .on_mouse_down_out(move |_, window, cx| {
                blur_state.update(cx, |state, cx| {
                    if !state.open
                        && (state.editing
                            || state.inputs[0].read(cx).value().as_ref() != state.displayed[0])
                    {
                        state.accept(window, cx);
                    }
                });
            })
            .capture_key_down(move |event, window, cx| {
                if event.keystroke.key == "escape"
                    && !cancel_state.update(cx, |state, cx| state.composing(window, cx))
                {
                    cancel_state.update(cx, |state, cx| state.cancel(cx));
                    cx.stop_propagation();
                }
            })
            .child(
                Popover::new(self.id.clone())
                    .p_0()
                    .open(open)
                    .track_focus(&focus)
                    .on_open_change(move |open, window, cx| {
                        // The suffix is outside the popup. Let its click replace the
                        // current candidate before closing, instead of committing twice.
                        if !open
                            && reset_target
                                .get()
                                .is_some_and(|bounds| bounds.contains(&window.mouse_position()))
                        {
                            return;
                        }
                        popup_state.update(cx, |state, cx| state.set_open(*open, window, cx))
                    })
                    .trigger(
                        compact::button(Button::new(self.id.clone()))
                            .disabled(disabled)
                            .accessibility_label(label.clone())
                            .w(compact::rem(24.))
                            .h(compact::rem(compact::NUMBER_CONTROL))
                            .child(
                                panel::swatch(color)
                                    .size(compact::rem(14.))
                                    .border_1()
                                    .border_color(cx.theme().border),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("{}-panel", self.id)))
                            .test_support()
                            .child(ColorPickerPanel::new(&self.state)),
                    ),
            )
            .child(
                div()
                    .id(self.input_id)
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .child(
                        compact::input(Input::new(&input))
                            .map(|input| Styled::h(input, compact::rem(compact::NUMBER_CONTROL)))
                            .disabled(disabled)
                            .aria_label(label)
                            .font_family(".AppleSystemUIFontMonospaced")
                            .when_some(self.reset_color, |input, color| {
                                let label = crate::t!(
                                    "color_picker.reset",
                                    color = color::hex(color, state.alpha)
                                )
                                .into_owned();
                                input.suffix(
                                    div()
                                        .relative()
                                        .size(compact::rem(14.))
                                        .child(
                                            compact::button(Button::new(reset_id))
                                                .ghost()
                                                .icon(gpui_kit::assets::IconName::RotateCcw)
                                                .h(compact::rem(14.))
                                                .w(compact::rem(14.))
                                                .min_w(compact::rem(14.))
                                                .p_0()
                                                .disabled(disabled)
                                                .accessibility_label(label.clone())
                                                .tooltip(label)
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_click(move |_, window, cx| {
                                                    reset_state.update(cx, |state, cx| {
                                                        state.reset_to(color, window, cx)
                                                    });
                                                    cx.stop_propagation();
                                                }),
                                        )
                                        .child(
                                            canvas(
                                                move |bounds, _, _| reset_bounds.set(Some(bounds)),
                                                |_, _, _, _| (),
                                            )
                                            .absolute()
                                            .inset_0(),
                                        ),
                                )
                            })
                            .when(invalid, |input| input.border_color(cx.theme().danger)),
                    ),
            )
            .capture_key_down(move |event, window, cx| {
                if event.keystroke.key == "enter"
                    && !accept_state.update(cx, |state, cx| state.composing(window, cx))
                {
                    accept_state.update(cx, |state, cx| state.accept(window, cx));
                    cx.stop_propagation();
                }
            })
    }
}
