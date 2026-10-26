//! Shared parameter row geometry for authoring and displayed-frame inspection.
use crate::panels::group_header;
use crate::ui::compact;
use gpui_kit::{prelude::*, *};

pub fn style<T: Styled>(element: T, _: &App) -> T {
    element
        .rounded_none()
        .flex()
        .items_center()
        .w_full()
        .h(crate::ui::compact::rem(super::PARAMETER_ROW_HEIGHT))
        .min_w_0()
        .gap(crate::ui::compact::rem(compact::GAP))
        .pl(group_header::left_padding(0))
        .pr(compact::rem(compact::PADDING))
}

pub fn name(depth: usize) -> Div {
    div()
        .flex_1()
        .flex_grow(2.)
        .max_w(crate::ui::compact::rem(120.))
        .min_w(crate::ui::compact::rem(64.))
        .text_xs()
        .pl(compact::rem(
            depth.min(6) as f32 * group_header::GROUP_INDENT,
        ))
        .text_ellipsis()
        .overflow_hidden()
}

pub fn track() -> Div {
    div()
        .flex()
        .items_center()
        .flex_1()
        .min_w(crate::ui::compact::rem(48.))
        .mr(crate::ui::compact::rem(compact::GAP * 2.))
        .px(crate::ui::compact::rem(4.))
        .h(crate::ui::compact::rem(compact::ROW))
}

pub fn value() -> Div {
    div().flex().w(crate::ui::compact::rem(56.)).flex_shrink_0()
}
