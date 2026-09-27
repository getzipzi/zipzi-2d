use super::{api_table, code_card, demo_card, note, page_intro};
use crate::playground::Playground;
use gpui::{prelude::*, *};

pub fn render(app: &mut Playground, cx: &Context<Playground>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "ACTIONS",
            "Button",
            "A compact action control. The caller supplies a unique element ID and handles the click.",
        ))
        .child(demo_card(
            "Live example",
            "Click the button; the event counter and status bar update.",
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(ui::button("count-clicks", "Apply changes").on_click(cx.listener(
                    |this, _, window, cx| {
                        this.click_count += 1;
                        this.status = format!("Button clicked {} times", this.click_count);
                        window.focus(&this.focus, cx);
                        cx.notify();
                    },
                )))
                .child(format!("Clicked {} times", app.click_count)),
        ))
        .child(demo_card(
            "States",
            "Default, hover, and disabled are visible here. The disabled version has no click handler.",
            div()
                .flex()
                .gap_3()
                .child(ui::button("button-default", "Default"))
                .child(ui::disabled_button("button-disabled", "Disabled")),
        ))
        .child(code_card(
            "Usage · Rust",
            "ui::button(\"apply\", \"Apply\")\n    .on_click(cx.listener(|this, _, _, cx| {\n        this.apply_changes();\n        cx.notify();\n    }));\n\nui::disabled_button(\"unavailable\", \"Disabled\");",
            cx,
        ))
        .child(api_table(&[
            ("button(id, label)", "Stateful<Div>", "Renders an active button. Attach the desired click handler."),
            ("disabled_button(id, label)", "Stateful<Div>", "Renders a non-interactive disabled state."),
            ("id", "Into<ElementId>", "Stable and unique in its rendered scope."),
            ("label", "Into<SharedString>", "Visible button text."),
        ]))
        .child(note(
            "Interaction",
            "Pointer hover changes the fill. Clicking uses GPUI's on_click callback. Keyboard activation and accessibility semantics are not yet implemented by this control.",
        ))
}
