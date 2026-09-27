use super::{demo_card, note, page_intro};
use gpui::{prelude::*, *};

pub fn render() -> Div {
    div()
        .flex()
        .flex_col()
        .gap_5()
        .child(page_intro(
            "FOUNDATIONS",
            "Typography",
            "Checks font fallback, symbol coverage, small type, and clipping in the native renderer.",
        ))
        .child(demo_card(
            "Multilingual text",
            "Latin, CJK, Korean, symbols, and ligatures in one surface.",
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_size(px(28.0)).child("Editor typography"))
                .child("中文：图层、参数、时间轴。日本語：レイヤー。한국어: 레이어.")
                .child("Symbols: ← → ↔ ✓ ✕ ◇ ▾  •  fi fl ffi  0123456789")
                .child(div().text_size(px(12.0)).child("Small text / 小字号 — 12 px")),
        ))
        .child(demo_card(
            "Truncation",
            "The label is constrained to 260 px; overflow should end in an ellipsis.",
            div()
                .w(px(260.0))
                .truncate()
                .child("Long layer name / 很长的图层名称 / should truncate at the boundary"),
        ))
        .child(note(
            "Visual check",
            "Use Compact and Full size in the title bar to inspect text under different available widths. Font rendering depends on the operating system's installed fonts.",
        ))
}
