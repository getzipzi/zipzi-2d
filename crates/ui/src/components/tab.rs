//! Controlled tab item. Selection and panel content live in the owning view.

use gpui::{prelude::*, *};

pub fn tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_4()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(if selected {
            rgb(0x3c698a)
        } else {
            rgb(0x263448)
        })
        .child(label.into())
}
