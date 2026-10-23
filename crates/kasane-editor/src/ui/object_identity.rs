//! Shared object title for inspectors and relation views.
use crate::{properties, t, ui::compact};
use editor_core::{Document, ObjectRef};
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::{prelude::*, *};

pub(crate) fn render(
    prefix: &'static str,
    doc: &Document,
    object: &ObjectRef,
    cx: &App,
) -> AnyElement {
    let index = doc.object_index();
    let name = index.name(object).to_owned();
    let kind = (properties::provider(object.kind).type_name)(doc, object);
    let locked = doc.authoring_capabilities(object).is_ok_and(|c| {
        matches!(
            c.state(editor_core::AuthoringAction::Patch),
            kasane_sdk::CapabilityState::Unavailable {
                code: "OBJECT_LOCKED"
            }
        )
    });
    render_title(prefix, name, kind, locked, cx)
}

pub(crate) fn render_title(
    prefix: &'static str,
    name: String,
    kind: SharedString,
    locked: bool,
    cx: &App,
) -> AnyElement {
    let tooltip = name.clone();
    div()
        .id(SharedString::from(format!("{prefix}-identity")))
        .test_support()
        .min_h(compact::rem(compact::DOCK_HEADER))
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .px(compact::rem(compact::PADDING))
        .py(compact::rem(compact::GAP))
        .border_b_1()
        .border_color(cx.theme().border)
        .flex()
        .items_center()
        .gap(compact::rem(compact::PADDING))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(compact::rem(compact::PADDING))
                .child(
                    div()
                        .id(SharedString::from(format!("{prefix}-title-name")))
                        .test_support()
                        .aria_label(name.clone())
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                .build(window, cx)
                        })
                        .child(name),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("{prefix}-title-type")))
                        .test_support()
                        .aria_label(kind.clone())
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(kind),
                ),
        )
        .when(locked, |element| {
            element.child(
                div()
                    .id(SharedString::from(format!("{prefix}-lock")))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(
                        gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Lock).xsmall(),
                    )
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(
                            t!("editor.object_locked").into_owned(),
                        )
                        .build(window, cx)
                    }),
            )
        })
        .into_any_element()
}
