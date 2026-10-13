//! Shared parameter row geometry for authoring and displayed-frame inspection.
use gpui_kit::component::ActiveTheme;
use gpui_kit::{prelude::*, *};

pub fn style<T: Styled>(element: T, cx: &App) -> T {
    element
        .flex()
        .items_center()
        .w_full()
        .h(px(super::PARAMETER_ROW_HEIGHT))
        .min_w_0()
        .gap_2()
        .px_3()
        .border_b_1()
        .border_color(cx.theme().border)
}

pub fn name(depth: usize) -> Div {
    div()
        .flex_1()
        .flex_grow(2.)
        .max_w(px(200.))
        .min_w(px(64.))
        .text_xs()
        .pl(px(depth.min(6) as f32 * 12.))
        .text_ellipsis()
        .overflow_hidden()
}

pub fn track() -> Div {
    div()
        .flex()
        .items_center()
        .flex_1()
        .min_w(px(48.))
        .px_2()
        .h_6()
}

pub fn value() -> Div {
    div().flex().w(px(48.)).flex_shrink_0()
}
