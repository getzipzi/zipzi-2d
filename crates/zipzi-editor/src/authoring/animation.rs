//! Animation palette commands share captured context and result presentation.
use super::*;
mod management;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use zipzi_editor_core::{AnimationRemoval, MotionCopyKind};
use zipzi_editor_core::{CommandId, PalettePosition};

#[derive(Clone)]
pub(crate) enum AnimationAction {
    Create(ObjectKind, Option<ObjectRef>),
    Step(ObjectRef, bool),
    Move(ObjectRef, Option<ObjectRef>, PalettePosition),
    RemoveEntry(ObjectRef),
    MoveTo(ObjectRef),
    CopyMotion(ObjectRef, MotionCopyKind),
    AddMotionReference(Option<ObjectRef>),
    DeleteAnimation(ObjectRef, AnimationRemoval),
}
pub(crate) fn creation_kind(id: CommandId) -> Option<ObjectKind> {
    Some(match id {
        CommandId::CreateMotion => ObjectKind::Motion,
        CommandId::CreateMotionGroup => ObjectKind::MotionGroup,
        CommandId::CreateExpression => ObjectKind::Expression,
        CommandId::CreateExpressionEntry => ObjectKind::ExpressionEntry,
        _ => return None,
    })
}
pub(crate) fn selection(editor: &Editor, motion: bool) -> Option<ObjectRef> {
    editor.animation_creation_selection(motion)
}

pub(crate) fn create_label(kind: ObjectKind) -> SharedString {
    t!(match kind {
        ObjectKind::Motion => "timeline.new_motion",
        ObjectKind::MotionGroup => "authoring.new_motion_group",
        ObjectKind::Expression => "authoring.new_expression",
        ObjectKind::ExpressionEntry => "authoring.new_expression_entry",
        _ => unreachable!(),
    })
    .into()
}
impl AnimationAction {
    pub(crate) fn availability(&self, editor: &Editor) -> Option<ActionAvailability> {
        let doc = editor.document()?;
        match self {
            Self::Create(kind, selection) => {
                match doc.animation_resource_creation_capability(*kind, selection.as_ref()) {
                    Ok(state) => ActionAvailability::from_core(&state),
                    Err(error) => Some(ActionAvailability {
                        enabled: false,
                        reason: Some(authoring_error(&error).into()),
                    }),
                }
            }
            Self::Step(object, _) => ActionAvailability::for_object(
                doc,
                object,
                zipzi_editor_core::AuthoringAction::Reorder,
            ),
            Self::Move(object, _, _) | Self::MoveTo(object) => ActionAvailability::for_object(
                doc,
                object,
                zipzi_editor_core::AuthoringAction::Move,
            ),
            Self::CopyMotion(object, kind) => match doc.motion_copy_capability(object, *kind) {
                Ok(state) => ActionAvailability::from_core(&state),
                Err(error) => Some(ActionAvailability {
                    enabled: false,
                    reason: Some(authoring_error(&error).into()),
                }),
            },
            Self::AddMotionReference(at) => {
                match doc.animation_resource_creation_capability(ObjectKind::Motion, at.as_ref()) {
                    Ok(state) => ActionAvailability::from_core(&state),
                    Err(error) => Some(ActionAvailability {
                        enabled: false,
                        reason: Some(authoring_error(&error).into()),
                    }),
                }
            }
            Self::DeleteAnimation(object, operation) => ActionAvailability::for_object(
                doc,
                object,
                if *operation == AnimationRemoval::DissolveGroup {
                    zipzi_editor_core::AuthoringAction::Dissolve
                } else {
                    zipzi_editor_core::AuthoringAction::Delete
                },
            ),
            Self::RemoveEntry(object) => ActionAvailability::for_object(
                doc,
                object,
                zipzi_editor_core::AuthoringAction::Delete,
            ),
        }
    }
}

impl EditorView {
    pub(crate) fn handle_animation_creation(
        &mut self,
        command: zipzi_editor_core::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if command.document() != self.editor.active_id() {
            return;
        }
        let Some(kind) = creation_kind(command.id) else {
            return;
        };
        let selection = selection(
            &self.editor,
            matches!(kind, ObjectKind::Motion | ObjectKind::MotionGroup),
        );
        if let Some(context) = AuthoringContext::capture(&self.editor) {
            self.run_animation_action(
                context,
                AnimationAction::Create(kind, selection),
                window,
                cx,
            );
        }
    }
    pub(crate) fn run_animation_action(
        &mut self,
        context: AuthoringContext,
        action: AnimationAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let AnimationAction::RemoveEntry(object) = &action {
            self.run_object_deletion(context, std::slice::from_ref(object), window, cx);
            return;
        }
        if let AnimationAction::DeleteAnimation(object, operation) = &action {
            if *operation != AnimationRemoval::DissolveGroup {
                self.run_object_deletion(context, std::slice::from_ref(object), window, cx);
                return;
            }
        }
        let document = context.editing.document;
        if AuthoringContext::capture(&self.editor) != Some(context) {
            self.set_edit_error(t!("authoring.move_stale").into_owned(), cx);
            return;
        }
        match &action {
            AnimationAction::CopyMotion(source, kind) => {
                self.copy_motion(context, source.clone(), *kind, window, cx);
                return;
            }
            AnimationAction::DeleteAnimation(source, operation) => {
                self.review_animation_deletion(context, source.clone(), *operation, window, cx);
                return;
            }
            AnimationAction::AddMotionReference(at) => {
                if !self.prepare_property_inputs(document, window, cx) {
                    return;
                }
                let context = AuthoringContext::capture(&self.editor).unwrap();
                let owner = cx.weak_entity();
                let at = at.clone();
                window.defer(cx, move |window, cx| {
                    management::open_reference(owner, context, at, window, cx)
                });
                return;
            }
            _ => {}
        }
        if let AnimationAction::MoveTo(source) = action {
            if !self.prepare_property_inputs(document, window, cx) {
                return;
            }
            let Some(context) = AuthoringContext::capture(&self.editor) else {
                return;
            };
            let owner = cx.weak_entity();
            window.defer(cx, move |window, cx| {
                crate::panels::open_resource_destination(
                    owner, document, context, source, window, cx,
                )
            });
            return;
        }
        let created = match &action {
            AnimationAction::Create(kind, _) => Some(*kind),
            _ => None,
        };
        let moved = match &action {
            AnimationAction::Step(o, _) | AnimationAction::Move(o, _, _) => Some(o.clone()),
            _ => None,
        };
        let deleted = match &action {
            AnimationAction::RemoveEntry(object) => {
                self.editor.document().unwrap().authoring_neighbors(object)
            }
            _ => vec![],
        };
        let result = self
            .execute_authoring(context, window, cx, |editor, document| {
                let doc = editor.document().unwrap();
                if let AnimationAction::RemoveEntry(object) = &action {
                    anyhow::ensure!(
                        object.kind == ObjectKind::ExpressionEntry,
                        "Expected expression member"
                    );
                    let plan =
                        doc.plan_expression_authoring(zipzi_sdk::ExpressionEdit::DeleteEntries {
                            ids: vec![object.id.clone()],
                        })?;
                    return editor.execute_expression_authoring(document, &plan);
                }
                let plan = match action {
                    AnimationAction::Create(kind, selection) => doc
                        .plan_animation_resource_creation(
                            kind,
                            t!(match kind {
                                ObjectKind::Motion => "authoring.motion_default_name",
                                ObjectKind::MotionGroup => "authoring.motion_group_default_name",
                                ObjectKind::Expression => "authoring.expression_default_name",
                                ObjectKind::ExpressionEntry => "authoring.expression_member",
                                _ => unreachable!(),
                            })
                            .into_owned(),
                            selection.as_ref(),
                        )?,
                    AnimationAction::Step(object, down) => {
                        doc.plan_animation_resource_step(&object, down)?
                    }
                    AnimationAction::Move(object, target, position) => {
                        doc.plan_animation_resource_move(&object, target.as_ref(), position)?
                    }
                    _ => unreachable!(),
                };
                editor.execute_animation_library(document, &plan)
            })
            .and_then(|result| {
                if let Some(kind) = created {
                    // Creation is the only structural action that explicitly activates.
                    if let Some(object) = result.suggested_selection.iter().find(|o| o.kind == kind)
                    {
                        match kind {
                            ObjectKind::Motion => {
                                let selection = self
                                    .editor
                                    .document()
                                    .unwrap()
                                    .motion_resources()
                                    .into_iter()
                                    .find(|r| r.selection.clip_id == object.id)
                                    .unwrap()
                                    .selection;
                                self.editor.select_motion(document, selection)?;
                            }
                            ObjectKind::Expression => {
                                self.editor.select_expression(document, object.id.clone())?
                            }
                            _ => {}
                        }
                    }
                    self.present_created_object(document, &result, kind, window, cx)?;
                } else if let Some(object) = moved {
                    self.defer_authoring_location(document, object, None, false, false, window, cx);
                } else if !deleted.is_empty() {
                    self.present_authoring_result(
                        document,
                        &result,
                        AuthoringPresentation::AfterDelete {
                            candidates: deleted,
                        },
                        window,
                        cx,
                    )?;
                }
                Ok(())
            });
        match result {
            Ok(()) => self.redraw(cx),
            Err(error) => self.set_edit_error(authoring_error(&error), cx),
        }
    }
}

pub(crate) fn row_menu<E: ContextMenuExt + IntoElement + 'static>(
    element: E,
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    object: ObjectRef,
) -> impl IntoElement {
    element.context_menu(move |menu, window, cx| {
        let mut menu = crate::ui::compact::menu(menu);
        let Some(entity) = owner.upgrade() else {
            return menu;
        };
        if !entity.update(cx, |state, cx| {
            state.editor.active_id() == Some(document)
                && !state.modal
                && state.prepare_property_inputs(document, window, cx)
        }) {
            return menu;
        }
        let Some(context) = AuthoringContext::capture(&entity.read(cx).editor) else {
            return menu;
        };
        let motion = matches!(
            object.kind,
            ObjectKind::MotionGroup | ObjectKind::MotionRegistration
        );
        let mut actions = vec![];
        for kind in if motion {
            vec![ObjectKind::Motion, ObjectKind::MotionGroup]
        } else {
            vec![ObjectKind::Expression, ObjectKind::ExpressionEntry]
        } {
            actions.push((
                create_label(kind).to_string(),
                AnimationAction::Create(kind, Some(object.clone())),
            ));
        }
        if object.kind == ObjectKind::ExpressionEntry {
            actions.push((
                t!("authoring.remove_expression_entry").into_owned(),
                AnimationAction::RemoveEntry(object.clone()),
            ));
        }
        if motion {
            actions.push((
                t!("authoring.add_motion_reference").into_owned(),
                AnimationAction::AddMotionReference(Some(object.clone())),
            ));
            if object.kind == ObjectKind::MotionGroup {
                actions.extend([
                    (
                        t!("authoring.copy_motion_group").into_owned(),
                        AnimationAction::CopyMotion(object.clone(), MotionCopyKind::Group),
                    ),
                    (
                        t!("authoring.dissolve_motion_group").into_owned(),
                        AnimationAction::DeleteAnimation(
                            object.clone(),
                            AnimationRemoval::DissolveGroup,
                        ),
                    ),
                    (
                        t!("authoring.delete_motion_group").into_owned(),
                        AnimationAction::DeleteAnimation(object.clone(), AnimationRemoval::Group),
                    ),
                ]);
            } else if let Some((_, entry)) = entity
                .read(cx)
                .editor
                .document()
                .unwrap()
                .motion_registration(&object.id)
            {
                actions.extend([
                    (
                        t!("authoring.copy_motion_content").into_owned(),
                        AnimationAction::CopyMotion(object.clone(), MotionCopyKind::Content),
                    ),
                    (
                        t!("authoring.remove_motion_registration").into_owned(),
                        AnimationAction::DeleteAnimation(
                            object.clone(),
                            AnimationRemoval::Registration,
                        ),
                    ),
                    (
                        t!("authoring.delete_motion_resource").into_owned(),
                        AnimationAction::DeleteAnimation(
                            ObjectRef::new(ObjectKind::Motion, &entry.clip_id),
                            AnimationRemoval::Motion,
                        ),
                    ),
                ]);
            }
        } else if object.kind == ObjectKind::Expression {
            actions.push((
                t!("authoring.delete_expression_resource").into_owned(),
                AnimationAction::DeleteAnimation(object.clone(), AnimationRemoval::Expression),
            ));
        }
        if matches!(
            object.kind,
            ObjectKind::MotionRegistration | ObjectKind::ExpressionEntry
        ) {
            actions.push((
                t!("authoring.move_to").into_owned(),
                AnimationAction::MoveTo(object.clone()),
            ));
        }
        let mut items = Vec::new();
        for (label, action) in actions {
            let Some(availability) = action.availability(&entity.read(cx).editor) else {
                continue;
            };
            let section = match &action {
                AnimationAction::Create(..) | AnimationAction::AddMotionReference(_) => 1,
                AnimationAction::CopyMotion(..) | AnimationAction::MoveTo(_) => 2,
                AnimationAction::RemoveEntry(_) | AnimationAction::DeleteAnimation(..) => 3,
                _ => unreachable!("not a context menu action"),
            };
            let reason = availability.reason.map(|reason| reason.to_string());
            let owner = owner.clone();
            items.push((
                section,
                PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |state, cx| {
                        if state.authoring_menu_feedback(context, reason.as_deref(), window, cx) {
                            return;
                        }
                        state.run_animation_action(context, action.clone(), window, cx)
                    });
                }),
            ));
        }
        items.sort_by_key(|(section, _)| *section);
        let mut previous = None;
        for (section, item) in items {
            if previous.is_some_and(|previous| previous != section) {
                menu = menu.separator();
            }
            menu = menu.item(item);
            previous = Some(section);
        }
        menu
    })
}
