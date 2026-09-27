use super::{api_table, code_card, demo_card, note, page_intro};
use crate::playground::Playground;
use gpui::{prelude::*, *};

pub fn render(app: &mut Playground, cx: &Context<Playground>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "FORM CONTROLS",
            "Slider",
            "A 0–100 value control that emits SliderChanged as the pointer moves.",
        ))
        .child(demo_card(
            "Live example",
            "Drag or click the track to change opacity. Reset returns to 65%.",
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(format!("Opacity: {:.0}%", app.slider_value))
                .child(app.slider.clone())
                .child(
                    div().child(ui::button("reset-slider", "Reset to 65%").w(px(160.0)).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.slider.update(cx, |slider, cx| slider.set_value(65.0, cx));
                        }),
                    )),
                ),
        ))
        .child(code_card(
            "Usage · Rust",
            "let slider = cx.new(|_| ui::Slider::new(65.0));\n\ncx.subscribe(&slider, |this, _, event: &ui::SliderChanged, cx| {\n    this.opacity = event.0 / 100.0;\n    cx.notify();\n}).detach();\n\n// In render:\n.child(self.slider.clone())",
            cx,
        ))
        .child(api_table(&[
            ("Slider::new(value)", "Slider", "Initial percentage, clamped to 0–100."),
            ("value()", "f32", "Current percentage."),
            ("set_value(value, cx)", "()", "Clamps and rounds; emits only when the value changes."),
            ("SliderChanged(value)", "Event", "Subscribe from the owning view to react to changes."),
        ]))
        .child(note(
            "Interaction",
            "Pointer down sets a value; dragging updates it continuously; pointer up ends the drag. Keyboard arrows and accessibility range semantics are not implemented yet.",
        ))
}
