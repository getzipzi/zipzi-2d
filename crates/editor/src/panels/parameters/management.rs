use super::*;
use editor_core::{ObjectKind, ObjectRef, ParameterGroupDraft};
use gpui_kit::component::{
    dialog::{Cancel, Confirm, DialogFooter},
    menu::{ContextMenuExt, PopupMenuItem},
};

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
    fn target(&self) -> Option<&str> {
        match self {
            Self::Edit(id) | Self::Delete(id) | Self::EditGroup(id) | Self::DeleteGroup(id) => {
                Some(id)
            }
            _ => None,
        }
    }
    fn deleting(&self) -> bool {
        matches!(self, Self::Delete(_) | Self::DeleteGroup(_))
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
        let document = state.editor.active_id().unwrap();
        self.finish_slider_drag(cx);
        let ok = owner.update(cx, |state, cx| {
            let result = state
                .editor
                .execute_for(document, EditorCommand::FinishEdit);
            let ok = result.is_ok();
            state.complete_edit(result, cx);
            ok
        });
        if !ok {
            return;
        }
        window.focus(&self.focus, cx);
        let Ok(form) =
            ManagementForm::new(self.owner.clone(), document, action.clone(), window, cx)
        else {
            return;
        };
        let footer = cx.new(|cx| ManagementFooter {
            form: form.clone(),
            _observe: cx.observe(&form, |_, _, cx| cx.notify()),
        });
        let initial_focus = if action.deleting() {
            form.read(cx).cancel_focus.clone()
        } else {
            form.read(cx).inputs[0].read(cx).focus_handle(cx)
        };
        let on_ok = form.clone();
        let close = form.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let on_ok = on_ok.clone();
            let close = close.clone();
            dialog
                .title(action.label().trim_end_matches('…').to_owned())
                .w(px(520.))
                .overlay_closable(false)
                .child(form.clone())
                .footer(footer.clone())
                .on_ok(move |_, window, cx| on_ok.update(cx, |f, cx| f.submit(window, cx)))
                .on_close(move |_, _, cx| close.update(cx, |f, _| f.completed = true))
        });
        cx.defer_in(window, move |_, window, cx| {
            window.focus(&initial_focus, cx)
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

struct ManagementForm {
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    action: ParameterAction,
    version: u64,
    initial: ParameterGroupDraft,
    inputs: [Entity<InputState>; 2],
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    completed: bool,
    error: Option<String>,
    summary: String,
    references: Vec<(String, Option<ObjectRef>)>,
    _subscriptions: Vec<Subscription>,
}
impl ManagementForm {
    fn new(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        action: ParameterAction,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<Entity<Self>> {
        let entity = owner.upgrade().unwrap();
        let doc = entity.read(cx).editor.document().unwrap();
        let initial = match &action {
            ParameterAction::EditGroup(id) | ParameterAction::DeleteGroup(id) => {
                doc.parameter_group_draft(id)?
            }
            _ => ParameterGroupDraft {
                name: t!("parameter.default_group_name").into_owned(),
                runtime_id: doc.suggested_parameter_group_id(),
                parent_id: None,
            },
        };
        let version = doc.parameter_model_revision();
        let mut references = Vec::new();
        let summary = match &action {
            ParameterAction::Delete(id) => {
                let p = doc.parameter_draft(id)?;
                let index = doc.object_index();
                for reference in doc.parameter_delete_references(id)? {
                    let object = index
                        .objects
                        .iter()
                        .find(|o| o.reference.id == reference)
                        .map(|o| o.reference.clone());
                    let label = object
                        .as_ref()
                        .map(|o| format!("{} · {:?}", index.name(o), o.kind))
                        .unwrap_or(reference);
                    references.push((label, object));
                }
                format!(
                    "{} · {}\n{}",
                    p.name,
                    p.runtime_id,
                    t!("parameter.delete_hint")
                )
            }
            ParameterAction::DeleteGroup(id) => {
                let (parameters, unresolved, groups) = doc.parameter_group_delete_counts(id)?;
                let destination = initial
                    .parent_id
                    .as_deref()
                    .and_then(|parent| doc.parameter_group_draft(parent).ok())
                    .map(|g| g.name)
                    .unwrap_or_else(|| t!("parameter.top_level_destination").into_owned());
                format!(
                    "{} · {}\n{}",
                    initial.name,
                    initial.runtime_id,
                    t!(
                        "parameter.delete_group_hint",
                        parameters = parameters,
                        unresolved = unresolved,
                        groups = groups,
                        destination = destination
                    )
                )
            }
            _ => String::new(),
        };
        Ok(cx.new(|cx| {
            let inputs = [initial.name.clone(), initial.runtime_id.clone()]
                .map(|v| cx.new(|cx| InputState::new(window, cx).default_value(v)));
            let mut subscriptions = Vec::new();
            for input in &inputs {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this: &mut Self, _, event: &InputEvent, window, cx| {
                        this.error = None;
                        if matches!(
                            event,
                            InputEvent::PressEnter {
                                secondary: false,
                                shift: false
                            }
                        ) {
                            let focus = this.focus.clone();
                            window.defer(cx, move |window, cx| {
                                focus.dispatch_action(&Confirm { secondary: false }, window, cx)
                            });
                        }
                        cx.notify();
                    },
                ));
            }
            subscriptions.push(cx.observe(&entity, |_, _, cx| cx.notify()));
            Self {
                owner,
                document,
                action,
                version,
                initial,
                inputs,
                focus: cx.focus_handle(),
                cancel_focus: cx.focus_handle(),
                completed: false,
                error: None,
                summary,
                references,
                _subscriptions: subscriptions,
            }
        }))
    }
    fn draft(&self, cx: &App) -> ParameterGroupDraft {
        ParameterGroupDraft {
            name: self.inputs[0].read(cx).value().trim().into(),
            runtime_id: self.inputs[1].read(cx).value().trim().into(),
            parent_id: self.initial.parent_id.clone(),
        }
    }
    fn validate(&self, cx: &App) -> anyhow::Result<()> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("parameter.document_changed")))?;
        let state = owner.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(self.document),
            t!("parameter.document_changed")
        );
        anyhow::ensure!(
            !state.modal && !state.editor.is_busy() && !state.editor.exiting(),
            t!("parameter.unavailable")
        );
        let doc = state.editor.document().unwrap();
        doc.check_parameter_version(self.version)?;
        match &self.action {
            ParameterAction::Delete(id) => {
                doc.ensure_editable(&[ObjectRef::new(ObjectKind::Parameter, id)])?;
                anyhow::ensure!(
                    doc.parameter_delete_references(id)?.is_empty(),
                    t!("parameter.in_use")
                );
            }
            ParameterAction::DeleteGroup(id) => {
                doc.parameter_group_draft(id)?;
                doc.ensure_editable(&[ObjectRef::new(ObjectKind::ParameterGroup, id)])?;
            }
            _ => doc.validate_parameter_group(self.action.target(), &self.draft(cx))?,
        }
        Ok(())
    }
    fn changed(&self, cx: &App) -> bool {
        !matches!(self.action, ParameterAction::EditGroup(_)) || self.draft(cx) != self.initial
    }
    fn command(&self, cx: &App) -> EditorCommand {
        match &self.action {
            ParameterAction::CreateGroup => EditorCommand::CreateParameterGroup {
                draft: self.draft(cx),
                expected_version: self.version,
            },
            ParameterAction::EditGroup(id) => EditorCommand::UpdateParameterGroup {
                id: id.clone(),
                draft: self.draft(cx),
                expected_version: self.version,
            },
            ParameterAction::DeleteGroup(id) => EditorCommand::DeleteParameterGroup {
                id: id.clone(),
                expected_version: self.version,
            },
            ParameterAction::Delete(id) => EditorCommand::DeleteParameter {
                id: id.clone(),
                expected_version: self.version,
            },
            _ => unreachable!(),
        }
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.completed || !self.changed(cx) {
            return false;
        }
        let result = self.validate(cx).and_then(|_| {
            let command = self.command(cx);
            self.owner.upgrade().unwrap().update(cx, |state, cx| {
                state.editor.execute_for(self.document, command)?;
                state.edit_error = None;
                state.redraw(cx);
                Ok(())
            })
        });
        match result {
            Err(error) => {
                self.error = Some(management_error(&error));
                cx.notify();
                false
            }
            Ok(()) => {
                self.completed = true;
                if !self.action.deleting() {
                    if let Some(owner) = self.owner.upgrade() {
                        let state = owner.read(cx);
                        let id = state
                            .editor
                            .document()
                            .and_then(|d| d.context().inspected.as_ref())
                            .map(|r| r.id.clone());
                        let panel = state.dock.as_ref().map(|d| d.parameters.clone());
                        if let (Some(id), Some(panel)) = (id, panel) {
                            panel.update(cx, |p, cx| p.locate_parameter_group(&id, window, cx));
                        }
                    }
                }
                true
            }
        }
    }
}
struct ManagementFooter {
    form: Entity<ManagementForm>,
    _observe: Subscription,
}
impl Render for ManagementFooter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let f = self.form.read(cx);
        let cancel = f.focus.clone();
        let confirm = f.focus.clone();
        DialogFooter::new()
            .child(
                Button::new("parameter-management-cancel")
                    .track_focus(&f.cancel_focus)
                    .label(t!("project.cancel"))
                    .on_click(move |_, window, cx| cancel.dispatch_action(&Cancel, window, cx)),
            )
            .child(
                Button::new("parameter-management-submit")
                    .primary()
                    .label(if f.action.deleting() {
                        t!("parameter.delete_confirm")
                    } else if f.action.target().is_some() {
                        t!("parameter.save")
                    } else {
                        t!("parameter.create")
                    })
                    .disabled(f.completed || f.validate(cx).is_err() || !f.changed(cx))
                    .on_click(move |_, window, cx| {
                        confirm.dispatch_action(&Confirm { secondary: false }, window, cx)
                    }),
            )
    }
}
impl Render for ManagementForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .id("parameter-management-form")
            .test_support()
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2();
        if self.action.deleting() {
            body = body.child(self.summary.clone());
            for (i, (label, reference)) in self.references.iter().enumerate() {
                body = body.child(div().text_xs().child(label.clone()).when_some(
                    reference.clone(),
                    |row, reference| {
                        let owner = self.owner.clone();
                        let document = self.document;
                        row.child(
                            Button::new(SharedString::from(format!("parameter-reference-{i}")))
                                .ghost()
                                .label(t!("parameter.locate"))
                                .on_click(move |_, window, cx| {
                                    let Some(owner) = owner.upgrade() else {
                                        return;
                                    };
                                    if owner.read(cx).editor.active_id() != Some(document) {
                                        return;
                                    }
                                    window.close_dialog(cx);
                                    owner.update(cx, |state, cx| {
                                        let r = state.editor.execute_for(
                                            document,
                                            EditorCommand::Navigate(reference.clone()),
                                        );
                                        state.complete_edit(r, cx);
                                    });
                                }),
                        )
                    },
                ));
            }
        } else {
            for (i, label) in [t!("editor.name"), t!("parameter.group_id")]
                .into_iter()
                .enumerate()
            {
                body = body.child(
                    div().text_xs().child(label.clone()).child(
                        Input::new(&self.inputs[i])
                            .id(if i == 0 {
                                "parameter-group-name"
                            } else {
                                "parameter-group-id"
                            })
                            .small()
                            .aria_label(label),
                    ),
                );
            }
        }
        let error = self
            .error
            .clone()
            .or_else(|| self.validate(cx).err().map(|e| management_error(&e)));
        if self.validate(cx).is_ok() {
            if let Some(owner) = self.owner.upgrade() {
                if let Some(doc) = owner.read(cx).editor.document() {
                    body = body.children(export_warning_elements(
                        doc.parameter_export_diagnostics(&self.command(cx)),
                        cx,
                    ));
                }
            }
        }
        body.when_some(error, |body, error| {
            body.child(
                div()
                    .id("parameter-management-error")
                    .test_support()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
        })
        .when(
            super::management::needs_reload(&self.owner, self.document, self.version, cx),
            |body| {
                body.child(
                    Button::new("parameter-management-reload")
                        .ghost()
                        .label(t!("parameter.reload"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            let owner = this.owner.clone();
                            let document = this.document;
                            let action = this.action.clone();
                            window.defer(cx, move |window, cx| {
                                let Some(owner) = owner.upgrade() else {
                                    return;
                                };
                                if owner.read(cx).editor.active_id() != Some(document) {
                                    return;
                                }
                                let panel =
                                    owner.read(cx).dock.as_ref().map(|d| d.parameters.clone());
                                if let Some(panel) = panel {
                                    window.close_dialog(cx);
                                    panel.update(cx, |p, cx| p.open_management(action, window, cx));
                                }
                            });
                        })),
                )
            },
        )
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

pub(super) fn reference_elements(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    refs: &[String],
    cx: &App,
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

pub(super) fn needs_reload(
    owner: &WeakEntity<EditorView>,
    document: DocumentId,
    version: u64,
    cx: &App,
) -> bool {
    owner.upgrade().is_some_and(|owner| {
        let state = owner.read(cx);
        state.editor.active_id() == Some(document)
            && state
                .editor
                .document()
                .is_some_and(|d| d.parameter_model_revision() != version)
    })
}
