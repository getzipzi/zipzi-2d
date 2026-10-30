use super::*;
use crate::authoring::AuthoringContext;
use crate::authoring::scene_creation::{COMMANDS, DEFORMATION_COMMANDS, creation_error};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};
use zipzi_editor_core::ObjectRef;

pub(super) fn new_part_button<const DEFORMER: bool>(
    panel: &HierarchyTreePanel<DEFORMER>,
    cx: &Context<HierarchyTreePanel<DEFORMER>>,
) -> Button {
    use zipzi_editor_core::{CommandId, EditingContext, PaletteProjection, SceneObjectType};
    let owner = panel.owner.clone();
    let label = t!(CommandId::CreatePart.spec().label).into_owned();
    let mut hint = label.clone();
    let mut disabled = true;
    let mut context = None;
    let mut selection = Vec::new();
    if let Some(entity) = owner.upgrade() {
        let state = entity.read(cx);
        if !state.modal
            && !state.editor.is_busy()
            && state.editor.active_id() == panel.document_id
            && state.editor.editing_context() == Some(EditingContext::Modeling)
        {
            context = AuthoringContext::capture(&state.editor);
            if let Some(document) = state.editor.document() {
                selection = document.context().selections.clone();
                let available = document.validate_scene_object_creation_in(
                    PaletteProjection::Organization,
                    SceneObjectType::Part,
                    &selection,
                );
                if let Err(error) = &available {
                    hint = creation_error(error);
                }
                disabled = context.is_none() || available.is_err();
            }
        }
    }
    crate::ui::compact::new_group_button(Button::new("parts-tree-new-part"))
        .accessibility_label(label)
        .tooltip(hint)
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            if let Some(context) = context {
                let _ = owner.update(cx, |view, cx| {
                    view.create_scene_object(
                        context,
                        PaletteProjection::Organization,
                        SceneObjectType::Part,
                        &selection,
                        window,
                        cx,
                    );
                });
            }
        })
}

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
    crate::ui::compact::header_button(Button::new(if DEFORMER {
        "deformer-tree-create"
    } else {
        "parts-tree-create"
    }))
    .icon(IconName::Plus)
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
