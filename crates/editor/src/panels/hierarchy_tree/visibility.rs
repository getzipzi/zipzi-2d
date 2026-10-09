use super::locking::execute;
use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, ObjectKind, ObjectRef};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::{Button, ButtonVariants},
    popover::Popover,
};
use gpui_kit::{prelude::*, *};

/// Resolving an inherited restriction is an explicit choice, never a row click.
pub(super) fn source_menu(
    trigger: Button,
    id: SharedString,
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    sources: Vec<ObjectRef>,
    visibility: bool,
) -> impl IntoElement {
    Popover::new(SharedString::from(format!("state-sources-{id}")))
        .p_1()
        .trigger(trigger)
        .content(move |_, _, cx| {
            let mut content = div()
                .id("state-source-list")
                .test_support()
                .flex()
                .flex_col()
                .w(px(216.))
                .max_h(px(208.))
                .overflow_y_scroll();
            let Some(entity) = owner.upgrade() else {
                return content;
            };
            let Some(doc) = entity.read(cx).editor.document() else {
                return content;
            };
            for source in &sources {
                let name = doc.object_index().name(source).to_owned();
                let locate_label =
                    t!("editor.locate_state_source", name = name.clone()).into_owned();
                let change_label = if visibility {
                    t!("editor.show_source")
                } else {
                    t!("editor.unlock_object")
                }
                .into_owned();
                let locate_owner = owner.clone();
                let locate = source.clone();
                let change_owner = owner.clone();
                let target = source.clone();
                let popup = cx.entity().downgrade();
                let popup2 = popup.clone();
                content = content.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(28.))
                        .flex_shrink_0()
                        .gap(px(2.))
                        .child(
                            Button::new(SharedString::from(format!(
                                "locate-state-source-{}",
                                source.id
                            )))
                            .ghost()
                            .small()
                            .h(px(26.))
                            .px_1()
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::new(if source.kind == ObjectKind::Offscreen {
                                            IconName::Layers
                                        } else {
                                            IconName::Folder
                                        })
                                        .xsmall()
                                        .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(div().flex_1().min_w_0().text_ellipsis().child(name)),
                            )
                            .accessibility_label(locate_label.clone())
                            .tooltip(locate_label)
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                execute(
                                    &locate_owner,
                                    document,
                                    EditorCommand::SelectObject(locate.clone()),
                                    window,
                                    cx,
                                );
                                _ = popup.update(cx, |state, cx| state.dismiss(window, cx));
                            }),
                        )
                        .child(
                            Button::new(SharedString::from(format!(
                                "change-state-source-{}",
                                source.id
                            )))
                            .ghost()
                            .small()
                            .w(px(26.))
                            .h(px(26.))
                            .flex_shrink_0()
                            .icon(
                                Icon::new(if visibility {
                                    IconName::Eye
                                } else {
                                    IconName::LockOpen
                                })
                                .xsmall(),
                            )
                            .accessibility_label(change_label.clone())
                            .tooltip(change_label)
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                let command = if visibility {
                                    EditorCommand::SetObjectVisible {
                                        object: target.clone(),
                                        visible: true,
                                    }
                                } else {
                                    EditorCommand::SetObjectsLocked {
                                        objects: vec![target.clone()],
                                        locked: false,
                                    }
                                };
                                execute(&change_owner, document, command, window, cx);
                                _ = popup2.update(cx, |state, cx| state.dismiss(window, cx));
                            }),
                        ),
                );
            }
            content
        })
}

pub(super) fn button(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    object: ObjectRef,
    id: SharedString,
    disabled: bool,
    cx: &App,
) -> AnyElement {
    let entity = owner.upgrade().unwrap();
    let Some(doc) = entity.read(cx).editor.document() else {
        return div().into_any_element();
    };
    let state = doc.object_state(&object);
    let inherited = !state.hidden_by.is_empty();
    let names = state
        .hidden_by
        .iter()
        .map(|r| doc.object_index().name(r).to_owned())
        .collect::<Vec<_>>()
        .join(", ");
    let scope = match object.kind {
        ObjectKind::Deformer => t!("editor.visibility_controls"),
        ObjectKind::Offscreen => t!("editor.visibility_composite"),
        _ => t!("editor.visibility_viewport"),
    };
    let mut tooltip = if inherited && state.explicit.hidden_in_editor {
        t!(
            "editor.visibility_self_and_inherited",
            sources = names,
            scope = scope
        )
        .into_owned()
    } else if inherited {
        t!(
            "editor.visibility_inherited",
            sources = names,
            scope = scope
        )
        .into_owned()
    } else {
        scope.into_owned()
    };
    if doc
        .object_index()
        .get(&object)
        .is_some_and(|info| info.enabled == Some(false))
    {
        tooltip.push_str(" · ");
        tooltip.push_str(&t!("editor.visibility_runtime_disabled"));
    }
    let icon = Icon::new(if state.visible() {
        IconName::Eye
    } else {
        IconName::EyeOff
    })
    .text_color(if !state.explicit.hidden_in_editor && inherited {
        cx.theme().muted_foreground.opacity(0.4)
    } else {
        cx.theme().foreground
    });
    let inherited_only = !state.explicit.hidden_in_editor && inherited;
    let trigger = Button::new(id.clone())
        .ghost()
        .small()
        .w(px(24.))
        .h(px(24.))
        .icon(icon)
        .tooltip(tooltip)
        .disabled(disabled);
    if inherited_only {
        return source_menu(trigger, id, owner, document, state.hidden_by, true).into_any_element();
    }
    trigger
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            let Some(entity) = owner.upgrade() else {
                return;
            };
            let Some(doc) = entity.read(cx).editor.document() else {
                return;
            };
            let visible = doc.object_state(&object).explicit.hidden_in_editor;
            execute(
                &owner,
                document,
                EditorCommand::SetObjectVisible {
                    object: object.clone(),
                    visible,
                },
                window,
                cx,
            );
        })
        .into_any_element()
}
