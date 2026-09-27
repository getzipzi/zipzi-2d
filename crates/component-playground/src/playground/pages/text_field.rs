use super::{api_table, code_card, demo_card, note, page_intro};
use crate::playground::Playground;
use gpui::{prelude::*, *};

pub fn render(app: &mut Playground, window: &Window, cx: &Context<Playground>) -> Div {
    let focus = app.input.read(cx).focus_handle(cx);
    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "FORM CONTROLS",
            "Text field",
            "A single-line field backed by gpui_ce_elements::EditableTextState. Keep the state entity alive across renders.",
        ))
        .child(demo_card(
            "Live example",
            "Edit the layer name, then use Read input. Try Chinese or Japanese IME composition.",
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(ui::text_field(
                    "layer-name",
                    &app.input,
                    focus,
                    window,
                    "Layer name / 图层名称",
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(ui::button("read-input", "Read input").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.status = format!("Input: {}", this.input.read(cx).as_str());
                                window.focus(&this.focus, cx);
                                cx.notify();
                            },
                        )))
                        .child(ui::button("reset-input", "Reset").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.input.update(cx, |input, cx| {
                                    input.emplace("Layer 01 · 图层", cx);
                                });
                                this.status = "Input reset".into();
                                window.focus(&this.focus, cx);
                                cx.notify();
                            },
                        ))),
                ),
        ))
        .child(code_card(
            "Usage · Rust",
            "ui::bind_text_input_keys(cx); // once at app startup\n\nlet input = cx.new(|cx| {\n    ui::EditableTextState::new(\n        ui::StringStorage::from(\"Layer 01\"), cx\n    )\n});\n\nlet focus = input.read(cx).focus_handle(cx);\nui::text_field(\"layer-name\", &input, focus, window, \"Layer name\");",
            cx,
        ))
        .child(api_table(&[
            ("text_field(id, state, focus, window, placeholder)", "Stateful<Div>", "Renders the field and its current focus border."),
            ("state", "&Entity<EditableTextState>", "Persistent text, selection, and IME composition state."),
            ("focus", "FocusHandle", "The text state's focus handle."),
            ("bind_text_input_keys", "fn(&mut App)", "Registers the upstream editing shortcuts once."),
        ]))
        .child(note(
            "IME and layout",
            "The text line is vertically centered inside a 38 px field. IME pre-edit replacement uses the upstream editable_text state. The regression tests cover composition replacement and UTF-16 ranges after emoji.",
        ))
}
