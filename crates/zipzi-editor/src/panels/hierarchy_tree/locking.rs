use super::StateColumn;
use crate::{EditorView, t};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, WindowExt,
    button::{Button, ButtonVariants},
    menu::{ContextMenuExt, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};
use std::collections::HashSet;
use zipzi_editor_core::{DocumentId, EditingContext, EditorCommand, ObjectRef, TreeNode};

pub(super) fn execute(
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
            if owner.modal
                || owner.editor.is_busy()
                || owner.editor.active_id() != Some(document)
                || owner.editor.editing_context() != Some(EditingContext::Modeling)
            {
                return;
            }
            owner.clear_edit_error(cx);
            if !owner.prepare_property_inputs(document, window, cx) {
                return;
            }
            let result = owner.editor.execute_for(document, command);
            // In particular, commit text before applying a lock that would make
            // its queued blur read-only. Keep any input-recovery explanation.
            match result {
                Ok(()) => owner.redraw(cx),
                Err(error) => owner.set_edit_error(error.to_string(), cx),
            }
        });
    }
}

pub(super) fn button(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    object: ObjectRef,
    id: SharedString,
    disabled: bool,
    preview: Option<bool>,
    cx: &App,
) -> impl IntoElement {
    let entity = owner.upgrade().unwrap();
    let editor = &entity.read(cx).editor;
    let (mut state, names) = editor
        .document()
        .map(|doc| {
            let state = doc.object_state_index().state(&object).clone();
            let names = state
                .locked_by
                .iter()
                .map(|r| doc.object_index().name(r).to_owned())
                .collect::<Vec<_>>()
                .join(", ");
            (state, names)
        })
        .unwrap_or_default();
    if let Some(locked) = preview {
        state.explicit.locked = locked;
    }
    let explicit = state.explicit.locked;
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
    let sources = state.locked_by.clone();
    let source_owner = owner.clone();
    let source_id = id.clone();
    let trigger = crate::ui::compact::header_button(Button::new(id))
        .ghost()
        .w(crate::ui::compact::rem(crate::ui::compact::HEADER_CONTROL));
    div()
        .w(crate::ui::compact::rem(crate::ui::compact::HEADER_CONTROL))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            trigger
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
                .when(!locked && preview.is_none(), |button| {
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
                    let locks = doc.object_state_index();
                    let state = locks.state(&object);
                    let command = if !state.explicit.locked && !state.locked_by.is_empty() {
                        return;
                    } else {
                        EditorCommand::SetObjectsLocked {
                            objects: vec![object.clone()],
                            locked: !state.explicit.locked,
                        }
                    };
                    execute(&owner, document, command, window, cx);
                })
                .map(|button| {
                    if !explicit && !sources.is_empty() {
                        super::visibility::source_menu(
                            button,
                            source_id,
                            source_owner,
                            document,
                            sources,
                            false,
                        )
                        .into_any_element()
                    } else {
                        button.into_any_element()
                    }
                }),
        )
}

pub(super) fn menu<const DEFORMER: bool, E: ContextMenuExt + IntoElement + 'static>(
    element: E,
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    object: ObjectRef,
    panel: WeakEntity<super::HierarchyTreePanel<DEFORMER>>,
) -> impl IntoElement {
    element.context_menu(move |mut menu, window, cx| {
        menu = crate::ui::compact::menu(menu);
        let Some(entity) = owner.upgrade() else {
            return menu;
        };
        if window.has_active_dialog(cx) {
            return menu;
        }
        if !entity.update(cx, |owner, cx| {
            owner.prepare_property_inputs(document, window, cx)
        }) {
            return menu;
        }
        let state = entity.read(cx);
        if state.modal
            || state.editor.is_busy()
            || state.editor.active_id() != Some(document)
            || state.editor.editing_context() != Some(EditingContext::Modeling)
        {
            return menu;
        }
        let Some(doc) = state.editor.document() else {
            return menu;
        };
        let states = doc.object_state_index();
        let objects = if doc.context().selections.contains(&object) {
            doc.context().selections.clone()
        } else {
            vec![object.clone()]
        };
        let creation_selection = objects.clone();
        let deletion_selection = objects.clone();
        let objects: Vec<_> = objects.into_iter().filter(|r| states.contains(r)).collect();
        let nodes = if DEFORMER {
            doc.deformer_tree()
        } else {
            doc.part_tree()
        };
        let selected: HashSet<_> = objects.iter().cloned().collect();
        let mut others = Vec::new();
        for node in &nodes {
            collect_other_objects(node, &selected, &mut others);
        }
        for (column, toggle, restrict_others, restore_others) in [
            (
                StateColumn::Visibility,
                t!("editor.toggle_visibility"),
                t!("editor.hide_others"),
                t!("editor.show_others"),
            ),
            (
                StateColumn::Lock,
                t!("editor.toggle_lock"),
                t!("editor.lock_others"),
                t!("editor.unlock_others"),
            ),
        ] {
            for (label, targets, restricted) in [
                (toggle, &objects, !column.restricted(states.state(&object))),
                (restrict_others, &others, true),
                (restore_others, &others, false),
            ] {
                let targets: Vec<_> = targets
                    .iter()
                    .filter(|r| column.restricted(states.state(r)) != restricted)
                    .cloned()
                    .collect();
                let owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .disabled(targets.is_empty())
                        .on_click(move |_, window, cx| {
                            execute(
                                &owner,
                                document,
                                column.command(targets.clone(), restricted),
                                window,
                                cx,
                            );
                        }),
                );
            }
        }
        for source in &states.state(&object).locked_by {
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
        {
            menu = super::management::creation_menu(
                menu.separator(),
                panel.clone(),
                Some(document),
                Some(creation_selection),
                window,
                cx,
            );
        }
        let menu = super::sorting::move_menu(
            menu.separator(),
            owner.clone(),
            document,
            if DEFORMER {
                zipzi_editor_core::PaletteProjection::Deformation
            } else {
                zipzi_editor_core::PaletteProjection::Organization
            },
            object.clone(),
            cx,
        );
        crate::authoring::object_deletion::append_menu(
            menu,
            owner.clone(),
            zipzi_editor_core::Command {
                id: zipzi_editor_core::CommandId::DeleteObject,
                source: zipzi_editor_core::CommandSource::ContextMenu,
                target: zipzi_editor_core::CommandTarget::Hierarchy {
                    document,
                    projection: if DEFORMER {
                        zipzi_editor_core::PaletteProjection::Deformation
                    } else {
                        zipzi_editor_core::PaletteProjection::Organization
                    },
                },
            },
            &deletion_selection,
            cx,
        )
    })
}

fn collect_other_objects(
    node: &TreeNode,
    selected: &HashSet<ObjectRef>,
    others: &mut Vec<ObjectRef>,
) -> bool {
    let object = node.reference();
    let mut contains_selected = object
        .as_ref()
        .is_some_and(|object| selected.contains(object));
    for child in &node.children {
        contains_selected |= collect_other_objects(child, selected, others);
    }
    if !contains_selected && let Some(object) = object {
        others.push(object);
    }
    contains_selected
}
