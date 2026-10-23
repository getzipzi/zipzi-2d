use super::*;
use crate::authoring::AuthoringContext;
use crate::authoring::scene_creation::{
    COMMANDS, DEFORMATION_COMMANDS, creation_error, default_name,
};
use editor_core::ObjectRef;
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
    crate::ui::compact::header_button(Button::new(if DEFORMER {
        "deformer-tree-create"
    } else {
        "mesh-tree-create"
    }))
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
    for &command in if DEFORMER {
        &DEFORMATION_COMMANDS[..]
    } else {
        &COMMANDS[..]
    } {
        let kind = command.scene_creation_type().unwrap();
        let availability = doc.validate_scene_object_creation_in(
            command.scene_creation_projection().unwrap(),
            kind,
            &selection,
        );
        let label_command = COMMANDS
            .iter()
            .find(|id| id.scene_creation_type() == Some(kind))
            .unwrap();
        let label = t!(label_command.spec().label);
        let label: SharedString = if let Err(error) = &availability {
            format!("{label} · {}", creation_error(error)).into()
        } else {
            label.into()
        };
        let owner = owner.downgrade();
        let selection = selection.clone();
        menu = menu.item(
            PopupMenuItem::new(label)
                .action(Box::new(crate::actions::RunCommand::new(command)))
                .disabled(availability.is_err())
                .on_click(move |_, window, cx| {
                    let Some(owner) = owner.upgrade() else { return };
                    owner.update(cx, |state, cx| {
                        if AuthoringContext::capture(&state.editor) != Some(context) {
                            state.set_edit_error(t!("authoring.move_stale").into_owned(), cx);
                            return;
                        }
                        match state
                            .editor
                            .document()
                            .unwrap()
                            .plan_scene_object_creation_in(
                                command.scene_creation_projection().unwrap(),
                                kind,
                                default_name(kind),
                                &selection,
                            ) {
                            Ok(plan) => {
                                state.apply_scene_creation(context, &plan, kind, window, cx)
                            }
                            Err(error) => state.set_edit_error(creation_error(&error), cx),
                        }
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
