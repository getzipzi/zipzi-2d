//! Clickable and disabled action buttons. Callers attach `on_click` to active buttons.

use gpui::{prelude::*, *};

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(0x34445b))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x476489)))
        .child(label.into())
}

pub fn disabled_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(0x2a3341))
        .text_color(rgb(0x727e91))
        .child(label.into())
}
