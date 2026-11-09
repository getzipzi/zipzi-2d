//! Compact table layout shared by inspector mappings.
use super::*;

pub(super) const FLEX_MIN_WIDTH: f32 = 92.;

pub(super) fn cell(width: Option<f32>, content: impl IntoElement, cx: &App) -> Div {
    div()
        .min_w_0()
        .px(compact::rem(2.))
        .flex()
        .items_center()
        .when_some(width, |cell, width| {
            cell.w(compact::rem(width)).flex_shrink_0()
        })
        .when(width.is_none(), |cell| {
            cell.flex_1().min_w(compact::rem(FLEX_MIN_WIDTH))
        })
        .border_r_1()
        .border_color(cx.theme().border.opacity(0.45))
        .child(div().w_full().min_w_0().child(content))
}

pub(super) fn header(cx: &App) -> Stateful<Div> {
    div()
        .id("table-header")
        .role(gpui::Role::Row)
        .flex()
        .items_center()
        .h(compact::rem(compact::HEADER_CONTROL))
        .flex_shrink_0()
        .text_size(compact::rem(compact::CAPTION))
        .text_color(cx.theme().muted_foreground)
        .bg(cx.theme().muted.opacity(0.5))
        .border_b_1()
        .border_color(cx.theme().border)
}

pub(super) fn row(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .role(gpui::Role::Row)
        .flex()
        .items_center()
        .min_h(compact::rem(compact::HEADER_CONTROL + 2.))
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.5))
}
