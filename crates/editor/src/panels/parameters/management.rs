use super::*;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};

#[derive(Clone)]
pub(crate) enum ParameterAction {
    Create(Option<String>),
    Edit(String),
    Delete(String),
    CreateGroup,
    EditGroup(String),
    DeleteGroup(String),
}
impl ParameterAction {
    pub(crate) fn label(&self) -> SharedString {
        t!(match self {
            Self::Create(_) => "parameter.new",
            Self::Edit(_) => "parameter.edit",
            Self::Delete(_) => "parameter.delete",
            Self::CreateGroup => "parameter.new_group",
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
            let owner = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                if owner
                    .upgrade()
                    .is_some_and(|owner| owner.read(cx).editor.active_id() == command.document())
                {
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
        self.finish_slider_drag(cx);
        let result = owner.update(cx, |state, cx| {
            state
                .editor
                .execute_for(document, EditorCommand::FinishEdit)?;
            let doc = state.editor.document().unwrap();
            let expected_version = doc.parameter_model_revision();
            let command = match action {
                ParameterAction::Create(group_id) => {
                    EditorCommand::CreateParameter(editor_core::NewParameter {
                        name: t!("parameter.default_name").into_owned(),
                        runtime_id: doc.suggested_parameter_id(),
                        group_id,
                        ..Default::default()
                    })
                }
                ParameterAction::CreateGroup => EditorCommand::CreateParameterGroup {
                    draft: editor_core::ParameterGroupDraft {
                        name: t!("parameter.default_group_name").into_owned(),
                        runtime_id: doc.suggested_parameter_group_id(),
                        parent_id: None,
                    },
                    expected_version,
                },
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
            };
            state.editor.execute_for(document, command)?;
            state.clear_edit_error(cx);
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
        for action in &actions {
            let panel = panel.clone();
            let action = action.clone();
            menu = menu.item(
                PopupMenuItem::new(action.label()).on_click(move |_, window, cx| {
                    let _ = panel.update(cx, |p, cx| {
                        if p.document_id == document {
                            p.open_management(action.clone(), window, cx);
                        }
                    });
                }),
            );
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
