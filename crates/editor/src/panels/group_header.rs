//! Shared folding chrome for parameter and animation resource groups.
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub const GROUP_ROW_HEIGHT: f32 = 26.;

pub fn row_style<T: Styled>(element: T, depth: usize, cx: &App) -> T {
    element
        .flex()
        .items_center()
        .w_full()
        .h(px(GROUP_ROW_HEIGHT))
        .pr_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(if depth == 0 { 0.6 } else { 0.3 }))
}

pub fn toggle(
    id: impl Into<ElementId>,
    count_id: impl Into<ElementId>,
    name: String,
    count: usize,
    expanded: bool,
    depth: usize,
    cx: &App,
) -> Button {
    let theme = cx.theme();
    Button::new(id)
        .ghost()
        .small()
        .flex_1()
        .min_w_0()
        .h_full()
        .px_3()
        .pl(px(12. + depth.min(6) as f32 * 12.))
        .justify_start()
        .accessibility_label(name.clone())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .min_w_0()
                .child(
                    Icon::new(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .small()
                    .text_color(theme.muted_foreground),
                )
                .child(
                    Icon::new(IconName::Folder)
                        .small()
                        .text_color(theme.muted_foreground),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                )
                .child(
                    div()
                        .id(count_id)
                        .test_support()
                        .aria_value(count.to_string())
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(count.to_string()),
                ),
        )
}
