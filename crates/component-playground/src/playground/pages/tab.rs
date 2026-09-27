use super::{api_table, code_card, demo_card, note, page_intro};
use crate::playground::Playground;
use gpui::{prelude::*, *};

pub fn render(app: &mut Playground, cx: &Context<Playground>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "NAVIGATION",
            "Tabs",
            "A controlled tab item. The owner tracks which tab is selected and renders its panel.",
        ))
        .child(demo_card(
            "Live example",
            "Switch between Properties and History; the selected style and panel change together.",
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(div().flex().gap_2().children(
                    ["Properties", "History"]
                        .into_iter()
                        .enumerate()
                        .map(|(index, label)| {
                            ui::tab(("demo-tab", index), label, app.active_tab == index)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.active_tab = index;
                                    this.status = format!("Selected {label} tab");
                                    cx.notify();
                                }))
                        }),
                ))
                .child(
                    div()
                        .p_4()
                        .rounded_md()
                        .bg(rgb(0x172435))
                        .child(if app.active_tab == 0 {
                            "Properties · Mesh / 4 vertices / 2 triangles"
                        } else {
                            "History · No edits in this session"
                        }),
                ),
        ))
        .child(code_card(
            "Usage · Rust",
            "ui::tab(\"properties\", \"Properties\", self.active_tab == 0)\n    .on_click(cx.listener(|this, _, _, cx| {\n        this.active_tab = 0;\n        cx.notify();\n    }));",
            cx,
        ))
        .child(api_table(&[
            ("tab(id, label, selected)", "Stateful<Div>", "Renders a tab with selected or idle fill."),
            ("selected", "bool", "Controlled by the owner; does not switch content itself."),
            ("on_click", "GPUI callback", "Update the selected index and notify the view."),
        ]))
        .child(note(
            "Interaction",
            "Pointer selection is implemented. Arrow-key navigation, focus management, and tab accessibility roles are not yet implemented.",
        ))
}
