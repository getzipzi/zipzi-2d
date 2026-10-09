use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, ObjectRef};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    menu::{ContextMenuExt, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};

fn execute(
    owner: &WeakEntity<EditorView>,
    document: DocumentId,
    command: EditorCommand,
    window: &mut Window,
    cx: &mut App,
) {
    if window.has_active_dialog(cx) {
        return;
    }
    if let Some(owner) = owner.upgrade() {
        owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() || owner.editor.active_id() != Some(document) {
                return;
            }
            let result = owner.editor.execute_for(document, command);
            owner.complete_edit(result, cx);
        });
    }
}

pub(super) fn button(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    object: ObjectRef,
    id: SharedString,
    disabled: bool,
    cx: &App,
) -> impl IntoElement {
    let entity = owner.upgrade().unwrap();
    let editor = &entity.read(cx).editor;
    let (state, names) = editor
        .document()
        .map(|doc| {
            let state = doc.lock_index().state(&object).clone();
            let names = state
                .inherited
                .iter()
                .map(|r| doc.object_index().name(r).to_owned())
                .collect::<Vec<_>>()
                .join(", ");
            (state, names)
        })
        .unwrap_or_default();
    let explicit = state.explicit;
    let locked = state.locked();
    let tooltip = if !names.is_empty() {
        if explicit {
            t!("editor.lock_self_and_inherited", sources = names).into_owned()
        } else {
            t!("editor.lock_inherited", sources = names).into_owned()
        }
    } else if explicit {
        t!("editor.unlock_object").into_owned()
    } else {
        t!("editor.lock_object").into_owned()
    };
    let color = if explicit {
        cx.theme().foreground
    } else if locked {
        cx.theme().muted_foreground.opacity(0.4)
    } else {
        cx.theme().muted_foreground
    };
    div()
        .w(px(24.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            Button::new(id)
                .ghost()
                .small()
                .w(px(24.))
                .h(px(24.))
                // Keep inherited locks dim even when the ghost button is hovered.
                .icon(
                    Icon::new(if locked {
                        IconName::Lock
                    } else {
                        IconName::LockOpen
                    })
                    .text_color(color),
                )
                .disabled(disabled)
                .tooltip(tooltip)
                .when(!locked, |button| {
                    button
                        .opacity(0.)
                        .group_hover("layer-row", |s| s.opacity(1.))
                        .focus(|s| s.opacity(1.))
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    // Resolve current state again: menus/buttons may outlive their render.
                    let Some(entity) = owner.upgrade() else {
                        return;
                    };
                    let Some(doc) = entity.read(cx).editor.document() else {
                        return;
                    };
                    let locks = doc.lock_index();
                    let state = locks.state(&object);
                    let command = if !state.explicit && !state.inherited.is_empty() {
                        EditorCommand::SelectObject(state.inherited[0].clone())
                    } else {
                        EditorCommand::SetObjectsLocked {
                            objects: vec![object.clone()],
                            locked: !state.explicit,
                        }
                    };
                    execute(&owner, document, command, window, cx);
                }),
        )
}

pub(super) fn menu<E: ContextMenuExt + IntoElement + 'static>(
    element: E,
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    object: ObjectRef,
) -> impl IntoElement {
    element.context_menu(move |mut menu, window, cx| {
        let Some(entity) = owner.upgrade() else {
            return menu;
        };
        if window.has_active_dialog(cx) {
            return menu;
        }
        let state = entity.read(cx);
        if state.modal || state.editor.is_busy() || state.editor.active_id() != Some(document) {
            return menu;
        }
        let Some(doc) = state.editor.document() else {
            return menu;
        };
        let locks = doc.lock_index();
        let objects = if doc.context().selections.contains(&object) {
            doc.context().selections.clone()
        } else {
            vec![object.clone()]
        };
        let objects: Vec<_> = objects.into_iter().filter(|r| locks.lockable(r)).collect();
        for (locked, label) in [
            (true, t!("editor.lock_selected")),
            (false, t!("editor.unlock_selected")),
        ] {
            let enabled = objects.iter().any(|r| locks.state(r).explicit != locked);
            let owner = owner.clone();
            let objects = objects.clone();
            menu = menu.item(PopupMenuItem::new(label).disabled(!enabled).on_click(
                move |_, window, cx| {
                    execute(
                        &owner,
                        document,
                        EditorCommand::SetObjectsLocked {
                            objects: objects.clone(),
                            locked,
                        },
                        window,
                        cx,
                    );
                },
            ));
        }
        for source in &locks.state(&object).inherited {
            let label = t!(
                "editor.locate_lock_source",
                name = doc.object_index().name(source)
            )
            .into_owned();
            let owner = owner.clone();
            let source = source.clone();
            menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                execute(
                    &owner,
                    document,
                    EditorCommand::SelectObject(source.clone()),
                    window,
                    cx,
                );
            }));
        }
        menu
    })
}
