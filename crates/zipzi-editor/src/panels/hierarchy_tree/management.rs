use super::*;
use crate::authoring::AuthoringContext;
use crate::authoring::scene_creation::{COMMANDS, DEFORMATION_COMMANDS, creation_error};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};
use zipzi_editor_core::ObjectRef;

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
            || state.editor.editing_context() != Some(zipzi_editor_core::EditingContext::Modeling)
    });
    super::super::toolbar::new_object_button(
        if DEFORMER {
            "deformer-tree-create"
        } else {
            "parts-tree-create"
        },
        disabled,
    )
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
    if window.has_active_dialog(cx) {
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
            && state.editor.editing_context() == Some(zipzi_editor_core::EditingContext::Modeling)
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
    for &command in if DEFORMER {
        &DEFORMATION_COMMANDS[..]
    } else {
        &COMMANDS[..]
    } {
        let kind = command.scene_creation_type().unwrap();
        let selection = if matches!(
            kind,
            zipzi_editor_core::SceneObjectType::Warp | zipzi_editor_core::SceneObjectType::Rotation
        ) && !doc.context().selections.is_empty()
        {
            doc.context().selections.clone()
        } else {
            selection.clone()
        };
        let availability = doc.validate_scene_object_creation_in(
            command.scene_creation_projection().unwrap(),
            kind,
            &selection,
        );
        let label_command = COMMANDS
            .iter()
            .find(|id| id.scene_creation_type() == Some(kind))
            .unwrap();
        let label = t!(label_command.spec().label).into_owned();
        let reason = availability.as_ref().err().map(creation_error);
        let owner = owner.downgrade();
        let selection = selection.clone();
        menu = menu.item(
            PopupMenuItem::new(label)
                .action(Box::new(crate::actions::RunCommand::new(command)))
                .on_click(move |_, window, cx| {
                    let Some(owner) = owner.upgrade() else { return };
                    owner.update(cx, |state, cx| {
                        if state.authoring_menu_feedback(context, reason.as_deref(), window, cx) {
                            return;
                        }
                        if AuthoringContext::capture(&state.editor) != Some(context) {
                            state.set_edit_error(t!("authoring.move_stale").into_owned(), cx);
                            return;
                        }
                        state.create_scene_object(
                            context,
                            command.scene_creation_projection().unwrap(),
                            kind,
                            &selection,
                            window,
                            cx,
                        );
                    });
                }),
        );
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
