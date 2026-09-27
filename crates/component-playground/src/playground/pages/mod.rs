mod button;
mod checkbox;
mod overview;
pub mod patterns;
mod slider;
mod tab;
mod text_field;
mod typography;

use super::Playground;
use gpui::{prelude::*, *};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Page {
    Overview,
    Button,
    Checkbox,
    TextField,
    Slider,
    Tabs,
    Patterns,
    Typography,
}

impl Page {
    pub const ALL: [Self; 8] = [
        Self::Overview,
        Self::Button,
        Self::Checkbox,
        Self::TextField,
        Self::Slider,
        Self::Tabs,
        Self::Patterns,
        Self::Typography,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Button => "Button",
            Self::Checkbox => "Checkbox",
            Self::TextField => "Text field",
            Self::Slider => "Slider",
            Self::Tabs => "Tabs",
            Self::Patterns => "Patterns",
            Self::Typography => "Typography",
        }
    }
}

pub fn render(
    page: Page,
    app: &mut Playground,
    window: &mut Window,
    cx: &mut Context<Playground>,
) -> Div {
    let body = match page {
        Page::Overview => overview::render(app, cx),
        Page::Button => button::render(app, cx),
        Page::Checkbox => checkbox::render(app, cx),
        Page::TextField => text_field::render(app, window, cx),
        Page::Slider => slider::render(app, cx),
        Page::Tabs => tab::render(app, cx),
        Page::Patterns => patterns::render(app, cx),
        Page::Typography => typography::render(),
    };
    div()
        .w_full()
        .max_w(px(1040.0))
        .p_6()
        .flex()
        .flex_col()
        .gap_5()
        .child(body)
}

pub fn page_intro(category: &'static str, title: &'static str, summary: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_color(rgb(0x7fd1e6)).child(category))
        .child(div().text_size(px(30.0)).child(title))
        .child(div().text_color(rgb(0xa9b9cc)).child(summary))
}

pub fn demo_card(title: &'static str, description: &'static str, example: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x34445a))
        .bg(rgb(0x1c2939))
        .child(
            div()
                .p_4()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_lg().child(title))
                .child(div().text_color(rgb(0x9eb1c6)).child(description)),
        )
        .child(
            div()
                .p_5()
                .border_t_1()
                .border_color(rgb(0x34445a))
                .child(example),
        )
}

pub fn code_card(title: &'static str, source: &'static str, cx: &Context<Playground>) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x34445a))
        .bg(rgb(0x101a27))
        .child(
            div()
                .px_4()
                .py_2()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(0x34445a))
                .child(title)
                .child(ui::button("copy-code", "Copy").on_click(cx.listener(
                    move |this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(source.to_owned()));
                        this.status = format!("Copied {title} example");
                        cx.notify();
                    },
                ))),
        )
        .child(
            div()
                .id("code-scroll")
                .overflow_x_scroll()
                .p_4()
                .font_family("Menlo")
                .text_size(px(12.0))
                .text_color(rgb(0xbad6e8))
                .children(source.lines().map(|line| {
                    div().whitespace_nowrap().py_1().child(if line.is_empty() {
                        " ".into()
                    } else {
                        line.to_owned()
                    })
                })),
        )
}

pub fn api_table(rows: &[(&'static str, &'static str, &'static str)]) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_lg().child("API"))
        .child(
            div()
                .rounded_lg()
                .border_1()
                .border_color(rgb(0x34445a))
                .children(rows.iter().map(|(name, kind, meaning)| {
                    div()
                        .p_3()
                        .border_b_1()
                        .border_color(rgb(0x34445a))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(
                                    div()
                                        .font_family("Menlo")
                                        .text_color(rgb(0x88d3e8))
                                        .child(*name),
                                )
                                .child(
                                    div()
                                        .font_family("Menlo")
                                        .text_color(rgb(0xa6acc0))
                                        .child(*kind),
                                ),
                        )
                        .child(div().text_color(rgb(0xb5c4d4)).child(*meaning))
                })),
        )
}

pub fn note(title: &'static str, text: &'static str) -> Div {
    div()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x3a5870))
        .bg(rgb(0x1a3040))
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_color(rgb(0x89d6e6)).child(title))
        .child(text)
}
