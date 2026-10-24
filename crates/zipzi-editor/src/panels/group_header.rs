//! Shared folding chrome for parameter and animation resource groups.
use gpui_kit::component::ActiveTheme;
use gpui_kit::{prelude::*, *};

pub const GROUP_ROW_HEIGHT: f32 = crate::ui::compact::ROW;

pub fn row_style<T: Styled>(element: T, depth: usize, cx: &App) -> T {
    element
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .h(crate::ui::compact::rem(GROUP_ROW_HEIGHT))
        .pr_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(if depth == 0 { 0.6 } else { 0.3 }))
}
