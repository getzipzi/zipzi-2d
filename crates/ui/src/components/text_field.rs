//! Single-line input using the upstream editable text state and IME handling.

use gpui::{prelude::*, *};
use gpui_elements::editable_text::{
    actions::{DEFAULT_INPUT_CONTEXT, default_bindings},
    text_input,
};

pub use gpui_elements::editable_text::{EditableTextState, StringStorage};

pub fn bind_text_input_keys(cx: &mut App) {
    cx.bind_keys(default_bindings().as_keybindings(Some(DEFAULT_INPUT_CONTEXT)));
}

/// The caller owns the state, so edits and selection survive redraws.
pub fn text_field(
    id: &'static str,
    state: &Entity<EditableTextState>,
    focus: FocusHandle,
    window: &Window,
    placeholder: &'static str,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(38.0))
        .px_2()
        .flex()
        .items_center()
        .bg(rgb(0x111b29))
        .border_1()
        .border_color(if focus.is_focused(window) {
            rgb(0x62cde6)
        } else {
            rgb(0x506078)
        })
        .rounded_md()
        .overflow_hidden()
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&focus, cx)
        })
        .child(
            text_input((id, 0usize))
                .state(state.downgrade())
                .caret_blink_interval_500ms()
                .placeholder(placeholder)
                .w_full()
                .h(px(20.0))
                .line_height(px(20.0))
                .whitespace_nowrap()
                .overflow_x_scroll(),
        )
}
