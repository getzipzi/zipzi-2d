use super::{Page, code_card, demo_card, note, page_intro};
use crate::playground::Playground;
use gpui::{prelude::*, *};

pub fn render(_app: &mut Playground, cx: &Context<Playground>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "GETTING STARTED",
            "Component library",
            "Reusable GPUI controls for the editor. Each page shows live states, integration code, API, and interaction behavior.",
        ))
        .child(demo_card(
            "Explore the components",
            "Select a page from the sidebar, or jump directly from here.",
            div().flex().flex_wrap().gap_2().children(
                [Page::Button, Page::Checkbox, Page::TextField, Page::Slider, Page::Tabs]
                    .into_iter()
                    .map(|page| {
                        ui::button(("overview-link", page as usize), page.label()).on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.page = page;
                                this.docs_scroll.set_offset(point(px(0.0), px(0.0)));
                                this.status = format!("Viewing {}", page.label());
                                window.focus(&this.focus, cx);
                                cx.notify();
                            }),
                        )
                    }),
            ),
        ))
        .child(code_card(
            "Cargo.toml",
            "[dependencies]\nui = { path = \"../ui\" }",
            cx,
        ))
        .child(note(
            "Structure",
            "crates/ui/src/components has one module per control. The editor imports ui; this playground imports ui and owns only examples and documentation.",
        ))
        .child(note(
            "Scope",
            "Button, Checkbox, Text field, Slider, and Tabs are public UI controls. Dropdown, tree, and dialog remain documented composition patterns until their keyboard and accessibility behavior is ready for a reusable API.",
        ))
}
