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
            .document()
            .and_then(|d| d.context().inspected.as_ref())
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
                    .is_some_and(|owner| owner.read(cx).editor.active_id() == command.document)
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
        match action {
            ParameterAction::Create(group) => {
                return self.open_parameter_dialog(None, group, window, cx);
            }
            ParameterAction::Edit(id) => {
                return self.open_parameter_dialog(Some(id), None, window, cx);
            }
            _ => {}
        }
        let Some(document) = self.prepare_parameter_dialog(window, cx) else {
            return;
        };
        let result = match action {
            ParameterAction::CreateGroup => super::group_form::ParameterGroupForm::open(
                self.owner.clone(),
                document,
                None,
                window,
                cx,
            ),
            ParameterAction::EditGroup(id) => super::group_form::ParameterGroupForm::open(
                self.owner.clone(),
                document,
                Some(id),
                window,
                cx,
            ),
            ParameterAction::Delete(id) => super::delete_confirmation::DeleteConfirmation::open(
                self.owner.clone(),
                document,
                super::delete_confirmation::DeleteTarget::Parameter(id),
                window,
                cx,
            ),
            ParameterAction::DeleteGroup(id) => {
                super::delete_confirmation::DeleteConfirmation::open(
                    self.owner.clone(),
                    document,
                    super::delete_confirmation::DeleteTarget::Group(id),
                    window,
                    cx,
                )
            }
            _ => unreachable!(),
        };
        if let Err(error) = result {
            if let Some(owner) = self.owner.upgrade() {
                owner.update(cx, |state, cx| {
                    state.edit_error = Some(management_error(&error));
                    cx.notify();
                });
            }
        }
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

pub(super) fn export_warning_elements(
    diagnostics: Vec<editor_core::ParameterExportDiagnostic>,
    cx: &App,
) -> Vec<AnyElement> {
    diagnostics
        .into_iter()
        .map(|d| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "{} · {} · {}: {}",
                    if d.introduced {
                        t!("parameter.export_new_warning")
                    } else {
                        t!("parameter.export_existing_warning")
                    },
                    d.resource,
                    d.code,
                    d.message
                ))
                .into_any_element()
        })
        .collect()
}

pub(super) fn reference_elements<V: crate::ui::form_dialog::FormDialogView>(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    refs: &[String],
    cx: &mut Context<V>,
) -> Vec<AnyElement> {
    let Some(entity) = owner.upgrade() else {
        return Vec::new();
    };
    let Some(doc) = entity.read(cx).editor.document() else {
        return Vec::new();
    };
    let index = doc.object_index();
    refs.iter()
        .enumerate()
        .map(|(i, id)| {
            let reference = index
                .objects
                .iter()
                .find(|o| &o.reference.id == id)
                .map(|o| o.reference.clone());
            let label = reference
                .as_ref()
                .map(|r| format!("{} · {:?}", index.name(r), r.kind))
                .unwrap_or_else(|| id.clone());
            let owner = owner.clone();
            let view = cx.weak_entity();
            div()
                .text_xs()
                .child(label)
                .when_some(reference, |row, reference| {
                    row.child(
                        Button::new(SharedString::from(format!("parameter-conflict-{i}")))
                            .ghost()
                            .label(t!("parameter.locate"))
                            .on_click(move |_, window, cx| {
                                let Some(owner) = owner.upgrade() else {
                                    return;
                                };
                                if owner.read(cx).editor.active_id() != Some(document) {
                                    return;
                                }
                                let _ = view.update(cx, |view, _| view.dialog_mut().close());
                                window.close_dialog(cx);
                                owner.update(cx, |state, cx| {
                                    let result = state.editor.execute_for(
                                        document,
                                        EditorCommand::Navigate(reference.clone()),
                                    );
                                    state.complete_edit(result, cx);
                                });
                            }),
                    )
                })
                .into_any_element()
        })
        .collect()
}
