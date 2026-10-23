//! Render provider-supplied details without interpreting their domain.
use crate::properties::DetailRow;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{prelude::*, *};

pub(crate) fn render(row: &DetailRow, cx: &App) -> AnyElement {
    match row {
        DetailRow::Hint(value) => div()
            .text_color(cx.theme().muted_foreground)
            .child(value.clone())
            .into_any_element(),
        DetailRow::Heading(value) => heading(value.clone(), cx).into_any_element(),
        DetailRow::Text { id, value } => {
            let element = div().child(value.clone());
            if let Some(id) = id {
                element.id(id.clone()).test_support().into_any_element()
            } else {
                element.into_any_element()
            }
        }
        DetailRow::Value {
            id,
            label,
            value,
            stacked,
        } => if *stacked {
            field(id.clone(), label.clone(), value.clone(), cx)
        } else {
            property(id.clone(), label.clone(), value.clone(), cx)
        }
        .into_any_element(),
    }
}

pub(crate) fn heading(title: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .mt_2()
        .pb_1()
        .font_weight(FontWeight::SEMIBOLD)
        .border_b_1()
        .border_color(cx.theme().border)
        .child(title.into())
}

pub(crate) fn field(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let value = value.into();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(
            div()
                .id(id)
                .test_support()
                .aria_label(value.clone())
                .child(value),
        )
}

pub(crate) fn property(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let value = value.into();
    div()
        .flex()
        .justify_between()
        .items_start()
        .gap_2()
        .flex_shrink_0()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(
            div()
                .id(id)
                .test_support()
                .aria_label(value.clone())
                .flex_1()
                .min_w_0()
                .text_right()
                .child(value),
        )
}
