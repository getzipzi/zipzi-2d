use super::super::toolbar::PanelToolbarPopover;
use super::track::{cursor_color, cursor_marker, key_marker_style};
use crate::{t, ui::compact};
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt;
use gpui_kit::component::{ActiveTheme, Theme, button::Button};
use gpui_kit::{prelude::*, *};
use zipzi_core::ParameterKind::{BlendShape, Normal};

#[derive(gpui_kit::IntoElement)]
pub(super) struct LegendButton;

impl RenderOnce for LegendButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let viewport_height = window.viewport_size().height;
        let placement = window.use_keyed_state("parameter-legend-placement", cx, |_, _| {
            (Anchor::TopRight, viewport_height / 2.)
        });
        let (anchor, space) = *placement.read(cx);
        let label = t!("parameter.legend_title");
        div()
            .on_prepaint(move |bounds, window, cx| {
                let above = bounds.top();
                let below = window.viewport_size().height - bounds.bottom();
                // Keep the trigger outside the popover's hit region after viewport clamping.
                let next = if below >= above {
                    (Anchor::TopRight, below)
                } else {
                    (Anchor::BottomRight, above)
                };
                if next != *placement.read(cx) {
                    let placement = placement.clone();
                    let view = window.current_view();
                    window.defer(cx, move |_, cx| {
                        placement.update(cx, |current, _| *current = next);
                        cx.notify(view);
                    });
                }
            })
            .child(
                PanelToolbarPopover::new(
                    "parameter-legend-popover",
                    Button::new("parameter-legend-button")
                        .icon(IconName::CircleQuestionMark)
                        .accessibility_label(label.clone())
                        .tooltip(label),
                )
                .anchor(anchor)
                .content(move |_, window, cx| content(window, cx, space)),
            )
    }
}

fn content(window: &Window, cx: &App, space: Pixels) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let cursor = |repeat, has_keys| cursor_marker(cursor_color(repeat, has_keys, theme), false);
    let key = |kind, base, selected, partial| {
        key_marker_style(div(), kind, base, selected, partial, theme)
    };
    let selected_key = || key(Normal, false, true, false);
    let red_ring = || cursor_marker(cursor_color(false, true, theme), true);
    let cursor_at_key = div()
        .flex()
        .items_center()
        .gap(compact::rem(4.))
        .child(red_ring().id("parameter-legend-red-ring").test_support())
        .child(
            div()
                .relative()
                .child(selected_key())
                .child(div().absolute().top_0().left_0().child(red_ring())),
        );
    let blend_shape_keys = div()
        .flex()
        .items_center()
        .gap(compact::rem(4.))
        .child(key(BlendShape, false, true, false))
        .child(key(BlendShape, true, true, false));

    div()
        .id("parameter-legend-content")
        .test_support()
        .w(compact::scaled(328., cx).min(window.viewport_size().width - px(32.)))
        .max_h(space - compact::scaled(32., cx))
        .overflow_y_scroll()
        .text_size(compact::rem(compact::BODY))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(compact::rem(3.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t!("parameter.legend_title")),
                )
                .child(heading(t!("parameter.legend_current_value"), theme))
                .child(entry(cursor(false, false), t!("parameter.legend_no_keys")))
                .child(entry(cursor(false, true), t!("parameter.legend_has_keys")))
                .child(entry(cursor_at_key, t!("parameter.legend_at_key")))
                .child(entry(cursor(true, true), t!("parameter.legend_repeat")))
                .child(heading(t!("parameter.legend_key_points"), theme))
                .child(entry(selected_key(), t!("parameter.legend_all_targets")))
                .child(entry(
                    key(Normal, false, true, true),
                    t!("parameter.legend_some_targets"),
                ))
                .child(entry(
                    key(Normal, false, false, false),
                    t!("parameter.legend_other_targets"),
                ))
                .child(entry(blend_shape_keys, t!("parameter.legend_blend_shape"))),
        )
}

fn heading(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .mt(compact::rem(5.))
        .text_color(theme.muted_foreground)
        .font_weight(FontWeight::MEDIUM)
        .child(text.into())
}

fn entry(sample: impl IntoElement, text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_start()
        .gap(compact::rem(6.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .w(compact::rem(28.))
                .h(compact::rem(16.))
                .child(sample),
        )
        .child(div().flex_1().min_w_0().child(text.into()))
}
