use super::*;
use crate::authoring::AuthoringContext;
use editor_core::{ObjectRef, SceneObjectType};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};

pub(super) fn button<const DEFORMER: bool>(
    panel: &HierarchyTreePanel<DEFORMER>,
    cx: &Context<HierarchyTreePanel<DEFORMER>>,
) -> impl IntoElement {
    let weak = cx.weak_entity();
    let document = panel.document_id;
    let disabled = panel.owner.upgrade().is_none_or(|owner| {
        let state = owner.read(cx);
        state.modal
            || state.editor.is_busy()
            || document.is_none()
            || state.editor.editing_context() != Some(editor_core::EditingContext::Modeling)
    });
    crate::ui::compact::header_button(Button::new("mesh-tree-create"))
        .icon(IconName::Plus)
        .ghost()
        .disabled(disabled)
        .accessibility_label(t!("authoring.new_object"))
        .tooltip(t!("authoring.new_object"))
        .dropdown_menu(move |menu, window, cx| {
            creation_menu(menu, weak.clone(), document, None, window, cx)
        })
}

/// Row context is explicit; toolbar context is captured once when it opens.
pub(super) fn creation_menu<const DEFORMER: bool>(
    mut menu: PopupMenu,
    panel: WeakEntity<HierarchyTreePanel<DEFORMER>>,
    document: Option<DocumentId>,
    selection: Option<Vec<ObjectRef>>,
    window: &mut Window,
    cx: &mut App,
) -> PopupMenu {
    menu = crate::ui::compact::menu(menu);
    if DEFORMER || window.has_active_dialog(cx) {
        return menu;
    }
    let Some(document) = document else {
        return menu;
    };
    let Some(entity) = panel.upgrade() else {
        return menu;
    };
    let Some(owner) = entity.read(cx).owner.upgrade() else {
        return menu;
    };
    let ready = owner.update(cx, |state, cx| {
        state.editor.active_id() == Some(document)
            && state.editor.editing_context() == Some(editor_core::EditingContext::Modeling)
            && state.prepare_property_inputs(document, window, cx)
    });
    if !ready {
        return menu;
    }
    let state = owner.read(cx);
    let Some(context) = AuthoringContext::capture(&state.editor) else {
        return menu;
    };
    let doc = state.editor.document().unwrap();
    let selection = selection.unwrap_or_else(|| doc.context().selections.clone());
    for (kind, name_key, label_key) in [
        (
            SceneObjectType::Part,
            "editor.tree_type_part",
            "authoring.new_part",
        ),
        (
            SceneObjectType::Mesh,
            "editor.tree_type_mesh",
            "authoring.new_mesh",
        ),
        (
            SceneObjectType::Warp,
            "editor.tree_type_warp",
            "authoring.new_warp",
        ),
        (
            SceneObjectType::Rotation,
            "editor.tree_type_rotation",
            "authoring.new_rotation",
        ),
        (
            SceneObjectType::Offscreen,
            "editor.tree_type_offscreen",
            "authoring.new_offscreen",
        ),
    ] {
        let plan = doc.plan_scene_object_creation(kind, t!(name_key).into_owned(), &selection);
        let label = t!(label_key);
        let label: SharedString = if let Err(error) = &plan {
            format!("{label} · {}", creation_error(error)).into()
        } else {
            label.into()
        };
        let panel = panel.clone();
        let owner = owner.downgrade();
        menu = menu.item(PopupMenuItem::new(label).disabled(plan.is_err()).on_click(
            move |_, window, cx| {
                let (Ok(plan), Some(owner)) = (&plan, owner.upgrade()) else {
                    return;
                };
                let created = owner.update(cx, |state, cx| {
                    let result = state
                        .execute_authoring(context, window, cx, |editor, document| {
                            editor.execute_scene_object_creation(document, plan)
                        })
                        .and_then(|result| {
                            state.present_created_object(
                                document,
                                &result,
                                kind.object_kind(),
                                window,
                                cx,
                            )
                        });
                    match result {
                        Ok(object) => {
                            state.clear_edit_error(cx);
                            state.redraw(cx);
                            object
                        }
                        Err(error) => {
                            state.set_edit_error(creation_error(&error), cx);
                            None
                        }
                    }
                });
                if let Some(object) = created {
                    _ = panel.update(cx, |panel, cx| {
                        if panel.document_id == Some(document) {
                            // Retain the search/type conditions and temporarily show
                            // this item and its ancestors through the same tree.
                            panel.view.filter.revealed = Some(object);
                            panel.last_selection = None;
                            panel.rows_dirty = true;
                            cx.notify();
                        }
                    });
                }
            },
        ));
    }
    menu
}

pub(super) fn root_menu<const DEFORMER: bool, E: ContextMenuExt + IntoElement + 'static>(
    element: E,
    panel: WeakEntity<HierarchyTreePanel<DEFORMER>>,
    document: Option<DocumentId>,
) -> impl IntoElement {
    element.context_menu(move |menu, window, cx| {
        creation_menu(menu, panel.clone(), document, Some(Vec::new()), window, cx)
    })
}

fn creation_error(error: &anyhow::Error) -> String {
    let message = error.to_string();
    if message.contains("OBJECT_LOCKED") {
        t!("authoring.locked").into_owned()
    } else if message.contains("OFFSCREEN_REQUIRES_PART") {
        t!("authoring.offscreen_requires_part").into_owned()
    } else {
        message
    }
}
