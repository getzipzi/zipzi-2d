use super::{api_table, code_card, demo_card, note, page_intro};
use crate::playground::Playground;
use gpui::{prelude::*, *};

pub fn render(app: &mut Playground, cx: &Context<Playground>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "SELECTION",
            "Checkbox",
            "A controlled boolean control: the owner stores the value and updates it on click.",
        ))
        .child(demo_card(
            "Controlled example",
            "Toggle visibility; the label reflects the current state.",
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(ui::checkbox("visible", "Visible / 可见", app.checked).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.checked = !this.checked;
                        this.status = format!("Visible: {}", this.checked);
                        cx.notify();
                    }),
                ))
                .child(if app.checked { "On" } else { "Off" }),
        ))
        .child(demo_card(
            "Visual states",
            "Both glyph states are shown without changing the live value.",
            div()
                .flex()
                .gap_3()
                .child(ui::checkbox("checked-sample", "Checked", true))
                .child(ui::checkbox("unchecked-sample", "Unchecked", false)),
        ))
        .child(code_card(
            "Usage · Rust",
            "ui::checkbox(\"visible\", \"Visible\", self.visible)\n    .on_click(cx.listener(|this, _, _, cx| {\n        this.visible = !this.visible;\n        cx.notify();\n    }));",
            cx,
        ))
        .child(api_table(&[
            ("checkbox(id, label, checked)", "Stateful<Div>", "Renders the current boolean state."),
            ("checked", "bool", "The owner stores and updates this value."),
            ("label", "&str", "Text shown next to the state glyph."),
        ]))
        .child(note(
            "Interaction",
            "The current implementation uses text glyphs for its checked state and pointer clicks for changes. Keyboard toggle and checkbox accessibility semantics are future work.",
        ))
}
