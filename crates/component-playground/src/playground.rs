//! Component documentation browser and interaction state.
mod pages;

use gpui::{prelude::*, *};
use pages::Page;
use ui::{EditableTextState, Slider, SliderChanged, StringStorage};

pub struct Playground {
    page: Page,
    input: Entity<EditableTextState>,
    slider: Entity<Slider>,
    slider_value: f32,
    checked: bool,
    active_tab: usize,
    click_count: usize,
    menu_open: bool,
    blend_mode: usize,
    tree_expanded: bool,
    selected_layer: usize,
    modal_open: bool,
    focus: FocusHandle,
    docs_scroll: ScrollHandle,
    status: String,
}

impl Playground {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let slider = cx.new(|_| Slider::new(65.0));
        cx.subscribe(&slider, |this, _, event: &SliderChanged, cx| {
            this.slider_value = event.0;
            this.status = format!("Slider changed to {:.0}%", event.0);
            cx.notify();
        })
        .detach();
        Self {
            page: Page::Overview,
            input: cx.new(|cx| EditableTextState::new(StringStorage::from("Layer 01 · 图层"), cx)),
            slider,
            slider_value: 65.0,
            checked: true,
            active_tab: 0,
            click_count: 0,
            menu_open: false,
            blend_mode: 0,
            tree_expanded: true,
            selected_layer: 0,
            modal_open: false,
            focus: cx.focus_handle(),
            docs_scroll: ScrollHandle::new(),
            status: "Ready · select a component".into(),
        }
    }
}

impl Render for Playground {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.page;
        let mut root = div()
            .id("component-playground")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x111925))
            .text_color(rgb(0xe3e9f2))
            .text_sm()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.menu_open = false;
                    this.modal_open = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .h(px(70.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_6()
                    .border_b_1()
                    .border_color(rgb(0x303d50))
                    .bg(rgb(0x1b2737))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_lg().child("KASANE UI"))
                            .child(
                                div()
                                    .text_color(rgb(0x91a8bf))
                                    .child("Component playground · GPUI"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                ui::button("compact-window", "Compact").on_click(|_, window, _| {
                                    window.resize(size(px(800.0), px(600.0)))
                                }),
                            )
                            .child(ui::button("full-window", "Full size").on_click(
                                |_, window, _| window.resize(size(px(1180.0), px(800.0))),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("sidebar")
                            .w(px(210.0))
                            .flex_shrink_0()
                            .overflow_y_scroll()
                            .p_3()
                            .bg(rgb(0x172231))
                            .border_r_1()
                            .border_color(rgb(0x303d50))
                            .child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_color(rgb(0x8199b2))
                                    .child("DOCUMENTATION"),
                            )
                            .children(Page::ALL.iter().copied().enumerate().map(
                                |(index, page)| {
                                    div()
                                        .id(("page-nav", index))
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(if current == page {
                                            rgb(0x345a78)
                                        } else {
                                            rgb(0x172231)
                                        })
                                        .hover(|style| style.bg(rgb(0x2d4157)))
                                        .child(page.label())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.page = page;
                                            this.menu_open = false;
                                            this.modal_open = false;
                                            this.docs_scroll.set_offset(point(px(0.0), px(0.0)));
                                            window.focus(&this.focus, cx);
                                            this.status = format!("Viewing {}", page.label());
                                            cx.notify();
                                        }))
                                },
                            )),
                    )
                    .child(
                        div()
                            .id("documentation-scroll")
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.docs_scroll)
                            .child(pages::render(current, self, window, cx)),
                    ),
            )
            .child(
                div()
                    .h(px(34.0))
                    .flex_shrink_0()
                    .px_5()
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(rgb(0x303d50))
                    .text_color(rgb(0xa9c4d8))
                    .child(self.status.clone()),
            );
        if self.modal_open {
            root = root.child(pages::patterns::dialog(self, cx));
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    /// Exercise the same state mounted by Playground, using macOS IME callbacks.
    #[gpui::test]
    fn ime_updates_replace_composition_instead_of_appending(cx: &mut TestAppContext) {
        let view = cx.add_window(|window, cx| Playground::new(window, cx));
        view.update(cx, |view, window, cx| {
            view.input.update(cx, |input, cx| {
                input.emplace("", cx);
                for text in ["n", "ni", "你"] {
                    let end = text.encode_utf16().count();
                    input.replace_and_mark_text_in_range(None, text, Some(end..end), window, cx);
                    assert_eq!(input.as_str(), text);
                    assert_eq!(input.marked_text_range(window, cx), Some(0..end));
                }
                input.replace_text_in_range(None, "你", window, cx);
                assert_eq!(input.as_str(), "你");
                assert_eq!(input.marked_text_range(window, cx), None);
                for text in ["h", "hao", "好"] {
                    let end = text.encode_utf16().count();
                    input.replace_and_mark_text_in_range(None, text, Some(end..end), window, cx);
                    assert_eq!(input.as_str(), format!("你{text}"));
                }
                input.replace_text_in_range(None, "好", window, cx);
                assert_eq!(input.as_str(), "你好");
                assert_eq!(input.marked_text_range(window, cx), None);
            });
        })
        .unwrap();
    }

    #[gpui::test]
    fn ime_replacement_uses_utf16_ranges_after_emoji(cx: &mut TestAppContext) {
        let view = cx.add_window(|window, cx| Playground::new(window, cx));
        view.update(cx, |view, window, cx| {
            view.input.update(cx, |input, cx| {
                input.emplace("😀旧文", cx);
                input.replace_and_mark_text_in_range(Some(2..4), "zhong", Some(5..5), window, cx);
                assert_eq!(input.as_str(), "😀zhong");
                assert_eq!(input.marked_text_range(window, cx), Some(2..7));
                input.replace_and_mark_text_in_range(None, "中文", Some(2..2), window, cx);
                assert_eq!(input.as_str(), "😀中文");
                input.replace_text_in_range(None, "中文", window, cx);
                assert_eq!(input.as_str(), "😀中文");
                assert_eq!(input.marked_text_range(window, cx), None);
            });
        })
        .unwrap();
    }
}
