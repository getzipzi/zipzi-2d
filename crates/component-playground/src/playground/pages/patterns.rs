use super::{code_card, demo_card, note, page_intro};
use crate::playground::Playground;
use gpui::{prelude::*, *};

const BLEND_MODES: [&str; 3] = ["Normal", "Multiply", "Screen"];

pub fn render(app: &mut Playground, cx: &Context<Playground>) -> Div {
    let mut dropdown = div().relative().w(px(180.0)).child(
        ui::button("blend-menu", format!("{}  ▾", BLEND_MODES[app.blend_mode])).on_click(
            cx.listener(|this, _, window, cx| {
                this.menu_open = !this.menu_open;
                window.focus(&this.focus, cx);
                cx.notify();
            }),
        ),
    );
    if app.menu_open {
        dropdown = dropdown.child(
            deferred(
                div()
                    .absolute()
                    .top(px(42.0))
                    .left_0()
                    .w(px(180.0))
                    .p_1()
                    .rounded_md()
                    .shadow_lg()
                    .border_1()
                    .border_color(rgb(0x6786ac))
                    .bg(rgb(0x31425a))
                    .children(BLEND_MODES.into_iter().enumerate().map(|(index, label)| {
                        div()
                            .id(("blend", index))
                            .p_2()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x4b6487)))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.blend_mode = index;
                                this.menu_open = false;
                                this.status = format!("Blend mode: {label}");
                                cx.notify();
                            }))
                    })),
            )
            .priority(1),
        );
    }

    let tree = div()
        .id("tree-list")
        .h(px(260.0))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            ui::button(
                "tree-expand",
                if app.tree_expanded {
                    "▾  Scene / 场景"
                } else {
                    "▸  Scene / 场景"
                },
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.tree_expanded = !this.tree_expanded;
                this.status = format!("Scene expanded: {}", this.tree_expanded);
                cx.notify();
            })),
        )
        .when(app.tree_expanded, |tree| {
            tree.children((0..30).map(|index| {
                div()
                    .id(("layer", index))
                    .flex_shrink_0()
                    .ml_4()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if app.selected_layer == index {
                        rgb(0x386080)
                    } else {
                        rgb(0x202b3b)
                    })
                    .hover(|style| style.bg(rgb(0x41546c)))
                    .child(format!("◇  Layer {:02} / 网格", index + 1))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_layer = index;
                        this.status = format!("Selected layer {:02}", index + 1);
                        cx.notify();
                    }))
            }))
        });

    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "COMPOSITION EXAMPLES",
            "Patterns",
            "Dropdown, scrolling tree, and dialog examples built from GPUI elements and the public Button control.",
        ))
        .child(demo_card(
            "Dropdown menu",
            "Choose a blend mode. Press Escape to close the menu.",
            dropdown,
        ))
        .child(demo_card(
            "Scene tree",
            "Expand, collapse, scroll through 30 layers, and select a row.",
            tree,
        ))
        .child(demo_card(
            "Dialog overlay",
            "Open the dialog, then Cancel, Confirm, or press Escape.",
            ui::button("open-dialog", "Open dialog").on_click(cx.listener(
                |this, _, window, cx| {
                    this.modal_open = true;
                    this.menu_open = false;
                    window.focus(&this.focus, cx);
                    cx.notify();
                },
            )),
        ))
        .child(code_card(
            "Composition · Rust",
            "// These patterns currently live in the playground.\n// Their public component APIs will be extracted after\n// keyboard, focus, and accessibility behavior is defined.",
            cx,
        ))
        .child(note(
            "Current boundary",
            "The dropdown does not flip at screen edges. The dialog covers this playground window but does not trap keyboard focus. The tree uses a fixed demonstration dataset.",
        ))
}

pub fn dialog(_app: &Playground, cx: &Context<Playground>) -> impl IntoElement {
    deferred(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x000000a0))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(380.0))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .rounded_lg()
                    .shadow_lg()
                    .border_1()
                    .border_color(rgb(0x7a9abb))
                    .bg(rgb(0x27384e))
                    .child(div().text_xl().child("Save changes?"))
                    .child("Dialog overlay / 弹窗与背景遮挡")
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(ui::button("cancel-dialog", "Cancel").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.modal_open = false;
                                    this.status = "Dialog cancelled".into();
                                    cx.notify();
                                },
                            )))
                            .child(
                                ui::button("confirm-dialog", "Confirm").on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.modal_open = false;
                                        this.status = "Dialog confirmed".into();
                                        cx.notify();
                                    },
                                )),
                            ),
                    ),
            ),
    )
    .priority(2)
}
