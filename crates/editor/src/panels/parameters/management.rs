use super::*;
use crate::authoring::AuthoringContext;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};

#[derive(Clone)]
pub(crate) enum ParameterAction {
    Create(Option<String>),
    Edit(String),
    Delete(String),
    CreateGroup,
    CreateAt {
        kind: editor_core::ObjectKind,
        selection: Vec<editor_core::ObjectRef>,
    },
    EditGroup(String),
    DeleteGroup(String),
}
impl ParameterAction {
    fn capture(self, editor: &editor_core::Editor) -> Self {
        use editor_core::{ObjectKind, ObjectRef};
        let inspected = || {
            editor
                .inspection(editor_core::EditingContext::Modeling)
                .and_then(|inspection| inspection.object())
                .into_iter()
                .collect()
        };
        match self {
            Self::Create(group) => Self::CreateAt {
                kind: ObjectKind::Parameter,
                selection: group.map_or_else(inspected, |id| {
                    vec![ObjectRef::new(ObjectKind::ParameterGroup, id)]
                }),
            },
            Self::CreateGroup => Self::CreateAt {
                kind: ObjectKind::ParameterGroup,
                selection: inspected(),
            },
            other => other,
        }
    }
    pub(crate) fn label(&self) -> SharedString {
        t!(match self {
            Self::Create(_)
            | Self::CreateAt {
                kind: editor_core::ObjectKind::Parameter,
                ..
            } => "parameter.new",
            Self::Edit(_) => "parameter.edit",
            Self::Delete(_) => "parameter.delete",
            Self::CreateGroup | Self::CreateAt { .. } => "parameter.new_group",
            Self::EditGroup(_) => "parameter.edit_group",
            Self::DeleteGroup(_) => "parameter.delete_group",
        })
        .into()
    }
}

impl EditorView {
    /// Resolve the inspected target before deferring the parameter management UI.
    pub(crate) fn handle_parameter_command(
        &mut self,
        command: editor_core::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use editor_core::CommandId;
        let target = self
            .editor
            .inspection(editor_core::EditingContext::Modeling)
            .and_then(|source| source.object())
            .map(|r| r.id.clone());
        let action = match command.id {
            CommandId::CreateParameter => Some(ParameterAction::Create(None)),
            CommandId::CreateParameterGroup => Some(ParameterAction::CreateGroup),
            CommandId::EditParameter => target.map(ParameterAction::Edit),
            CommandId::DeleteParameter => target.map(ParameterAction::Delete),
            CommandId::EditParameterGroup => target.map(ParameterAction::EditGroup),
            CommandId::DeleteParameterGroup => target.map(ParameterAction::DeleteGroup),
            _ => None,
        };
        if let (Some(action), Some(panel)) = (
            action,
            self.dock.as_ref().map(|dock| dock.parameters.clone()),
        ) {
            let action = action.capture(&self.editor);
            if let Some(document) = command.document() {
                if !self.prepare_property_inputs(document, window, cx) {
                    return;
                }
            }
            let context = AuthoringContext::capture(&self.editor);
            let owner = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                if owner.upgrade().is_some_and(|owner| {
                    owner.read(cx).editor.active_id() == command.document()
                        && AuthoringContext::capture(&owner.read(cx).editor) == context
                }) {
                    panel.update(cx, |panel, cx| panel.open_management(action, window, cx));
                }
            });
        }
    }
}

impl ParametersPanel {
    pub(crate) fn open_management(
        &mut self,
        action: ParameterAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        if state.modal
            || !state.editor.command_available(
                state
                    .editor
                    .command(editor_core::CommandId::CreateParameter),
            )
        {
            return;
        }
        let Some(document) = state.editor.active_id() else {
            return;
        };
        let Some(context) = AuthoringContext::capture(&state.editor) else {
            return;
        };
        let action = action.capture(&state.editor);
        let result = owner.update(cx, |state, cx| {
            if let ParameterAction::CreateAt { kind, selection } = action {
                let name = t!(if kind == editor_core::ObjectKind::Parameter {
                    "parameter.default_name"
                } else {
                    "parameter.default_group_name"
                })
                .into_owned();
                let result = state.execute_authoring(context, window, cx, |editor, document| {
                    let plan = editor
                        .document()
                        .unwrap()
                        .plan_parameter_object_creation(kind, name, &selection)?;
                    editor.execute_organization_authoring(document, &plan)
                })?;
                state.present_created_object(document, &result, kind, window, cx)?;
                state.redraw(cx);
                return Ok(());
            }
            state.clear_edit_error(cx);
            if !state.prepare_property_inputs(document, window, cx) {
                return Ok(());
            }
            let expected_version = state.editor.document().unwrap().parameter_model_revision();
            let command = match action {
                ParameterAction::Edit(id) => EditorCommand::Navigate(editor_core::ObjectRef::new(
                    editor_core::ObjectKind::Parameter,
                    id,
                )),
                ParameterAction::EditGroup(id) => EditorCommand::Navigate(
                    editor_core::ObjectRef::new(editor_core::ObjectKind::ParameterGroup, id),
                ),
                ParameterAction::Delete(id) => EditorCommand::DeleteParameter {
                    id,
                    expected_version,
                },
                ParameterAction::DeleteGroup(id) => EditorCommand::DeleteParameterGroup {
                    id,
                    expected_version,
                },
                _ => unreachable!("creation context already captured"),
            };
            state.editor.execute_for(document, command)?;
            state.redraw(cx);
            Ok::<_, anyhow::Error>(())
        });
        if let Err(error) = result {
            owner.update(cx, |state, cx| {
                state.set_edit_error(management_error(&error), cx)
            });
            return;
        }
        let group = owner
            .read(cx)
            .editor
            .document()
            .and_then(|doc| doc.context().inspected.as_ref())
            .filter(|object| object.kind == editor_core::ObjectKind::ParameterGroup)
            .map(|object| object.id.clone());
        if let Some(id) = group {
            self.locate_parameter_group(&id, window, cx);
        } else {
            self.locate_created_parameter(window, cx);
        }
        owner.update(cx, |state, cx| {
            if let Some(dock) = &state.dock {
                dock.show(editor_core::PanelKind::Properties, None, window, cx);
            }
        });
    }

    pub(crate) fn locate_parameter_group(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_with_editor(window, cx);
        if let Some(owner) = self.owner.upgrade() {
            if let Some(doc) = owner.read(cx).editor.document() {
                let mut current = Some(id.to_owned());
                while let Some(id) = current {
                    self.collapsed_groups.remove(&Some(id.clone()));
                    current = doc
                        .parameter_group_draft(&id)
                        .ok()
                        .and_then(|g| g.parent_id);
                }
            }
        }
        self.refresh_visible_rows(cx);
        if let Some(i) = self
            .visible_rows
            .iter()
            .position(|r| matches!(r, ListRow::Group { id: Some(g), .. } if g == id))
        {
            self.scroll.scroll_to_item(i, ScrollStrategy::Center);
        }
        cx.notify();
    }
}

pub(super) fn row_menu<E: ContextMenuExt + IntoElement + 'static>(
    element: E,
    panel: WeakEntity<ParametersPanel>,
    document: Option<DocumentId>,
    actions: Vec<ParameterAction>,
) -> impl IntoElement {
    element.context_menu(move |mut menu, window, cx| {
        menu = crate::ui::compact::menu(menu);
        if window.has_active_dialog(cx) {
            return menu;
        }
        let owner = panel
            .upgrade()
            .and_then(|panel| panel.read(cx).owner.upgrade());
        let Some(owner) = owner else {
            return menu;
        };
        let ready = owner.update(cx, |state, cx| {
            document.is_some_and(|document| {
                state.editor.active_id() == Some(document)
                    && state.prepare_property_inputs(document, window, cx)
            })
        });
        if !ready {
            return menu;
        }
        let context = AuthoringContext::capture(&owner.read(cx).editor);
        for action in &actions {
            let panel = panel.clone();
            let action = action.clone();
            menu = menu.item(
                PopupMenuItem::new(action.label()).on_click(move |_, window, cx| {
                    let _ = panel.update(cx, |p, cx| {
                        if p.document_id == document
                            && p.owner.upgrade().is_some_and(|owner| {
                                AuthoringContext::capture(&owner.read(cx).editor) == context
                            })
                        {
                            p.open_management(action.clone(), window, cx);
                        }
                    });
                }),
            );
        }
        if let Some(object) = actions.iter().find_map(|action| match action {
            ParameterAction::Edit(id) => Some(editor_core::ObjectRef::new(
                editor_core::ObjectKind::Parameter,
                id,
            )),
            ParameterAction::EditGroup(id) => Some(editor_core::ObjectRef::new(
                editor_core::ObjectKind::ParameterGroup,
                id,
            )),
            _ => None,
        }) {
            if let Some(document) = document {
                menu = super::sorting::move_menu(
                    menu.separator(),
                    owner.downgrade(),
                    document,
                    editor_core::PaletteProjection::Parameters,
                    object,
                    cx,
                );
            }
        }
        menu
    })
}

pub(super) fn management_error(error: &anyhow::Error) -> String {
    use editor_core::ParameterManagementError as E;
    let Some(error) = error.downcast_ref::<E>() else {
        return error.to_string();
    };
    match error {
        E::Stale => t!("parameter.stale").into_owned(),
        E::MissingParameter => t!("parameter.missing_parameter").into_owned(),
        E::MissingGroup | E::MissingParent => t!("parameter.missing_group").into_owned(),
        E::GroupNameRequired => t!("parameter.group_name_required").into_owned(),
        E::GroupIdRequired => t!("parameter.group_id_required").into_owned(),
        E::DuplicateGroupId => t!("parameter.duplicate_group_id").into_owned(),
        E::GroupCycle => t!("parameter.group_cycle").into_owned(),
        E::Referenced(_) => t!("parameter.in_use").into_owned(),
        E::Dependency { message, .. } => {
            format!("{}: {}", t!("parameter.dependency_conflict"), message)
        }
    }
}
