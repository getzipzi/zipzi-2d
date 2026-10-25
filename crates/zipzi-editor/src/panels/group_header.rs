//! Shared folding chrome for parameter and animation resource groups.
use gpui_kit::component::ActiveTheme;
use gpui_kit::{prelude::*, *};

pub const GROUP_ROW_HEIGHT: f32 = crate::ui::compact::ROW;
pub const ROW_LEFT_PADDING: f32 = 4.;
pub const GROUP_INDENT: f32 = 8.;

pub fn left_padding(depth: usize) -> Rems {
    crate::ui::compact::rem(ROW_LEFT_PADDING + depth.min(6) as f32 * GROUP_INDENT)
}

pub fn row_style<T: Styled>(element: T, depth: usize, cx: &App) -> T {
    element
        .rounded_none()
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .h(crate::ui::compact::rem(GROUP_ROW_HEIGHT))
        .pl(left_padding(depth))
        .pr_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(if depth == 0 { 0.6 } else { 0.3 }))
}
