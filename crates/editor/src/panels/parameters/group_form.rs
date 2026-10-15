use super::dialog_session::ParameterDialogSession;
use super::*;
use crate::ui::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use editor_core::{ParameterGroupDraft, ParameterManagementError};

pub(super) struct ParameterGroupForm {
    session: ParameterDialogSession,
    dialog: FormDialogState,
    target: Option<String>,
    initial: ParameterGroupDraft,
    inputs: [Entity<InputState>; 2],
    validation: anyhow::Result<()>,
    diagnostics: Vec<editor_core::ParameterExportDiagnostic>,
    _subscriptions: Vec<Subscription>,
}

impl ParameterGroupForm {
    pub fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        target: Option<String>,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner.upgrade().unwrap();
        let doc = entity.read(cx).editor.document().unwrap();
        let initial = match &target {
            Some(id) => doc.parameter_group_draft(id)?,
            None => ParameterGroupDraft {
                name: t!("parameter.default_group_name").into_owned(),
                runtime_id: doc.suggested_parameter_group_id(),
                parent_id: None,
            },
        };
        let session = ParameterDialogSession::new(owner, document, cx);
        let form = cx.new(|cx| {
            let inputs = [initial.name.clone(), initial.runtime_id.clone()]
                .map(|v| cx.new(|cx| InputState::new(window, cx).default_value(v)));
            let mut subscriptions = Vec::new();
            for input in &inputs {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this: &mut Self, _, event: &InputEvent, window, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.dialog.edited();
                            this.refresh(cx);
                            cx.notify();
                        }
                        form_dialog::input_enter(event, window, cx);
                    },
                ));
            }
            subscriptions.push(cx.observe(&entity, |this: &mut Self, _, cx| {
                if !this.dialog.closed() && this.session.source_changed(cx) {
                    this.refresh(cx);
                    cx.notify();
                }
            }));
            Self {
                session,
                dialog: FormDialogState::new(cx),
                target,
                initial,
                inputs,
                validation: Ok(()),
                diagnostics: Vec::new(),
                _subscriptions: subscriptions,
            }
        });
        form.update(cx, |form, cx| form.refresh(cx));
        let state = form.read(cx);
        let options = FormDialogOptions {
            title: state.action().label().trim_end_matches('…').to_owned(),
            submit_label: if state.target.is_some() {
                t!("parameter.save")
            } else {
                t!("parameter.create")
            }
            .into(),
            footer_id: "parameter-management-footer",
            cancel_id: "parameter-management-cancel",
            submit_id: "parameter-management-submit",
            initial_focus: state.inputs[0].read(cx).focus_handle(cx),
        };
        form_dialog::open(form, options, window, cx);
        Ok(())
    }

    fn action(&self) -> ParameterAction {
        self.target
            .clone()
            .map(ParameterAction::EditGroup)
            .unwrap_or(ParameterAction::CreateGroup)
    }

    fn draft(&self, cx: &App) -> ParameterGroupDraft {
        ParameterGroupDraft {
            name: self.inputs[0].read(cx).value().trim().into(),
            runtime_id: self.inputs[1].read(cx).value().trim().into(),
            parent_id: self.initial.parent_id.clone(),
        }
    }

    fn validate(&self, cx: &App) -> anyhow::Result<()> {
        self.session.validate(cx)?;
        self.session
            .owner
            .upgrade()
            .unwrap()
            .read(cx)
            .editor
            .document()
            .unwrap()
            .validate_parameter_group(self.target.as_deref(), &self.draft(cx))
    }

    fn command(&self, cx: &App) -> EditorCommand {
        let draft = self.draft(cx);
        let expected_version = self.session.version;
        match &self.target {
            Some(id) => EditorCommand::UpdateParameterGroup {
                id: id.clone(),
                draft,
                expected_version,
            },
            None => EditorCommand::CreateParameterGroup {
                draft,
                expected_version,
            },
        }
    }

    fn refresh(&mut self, cx: &App) {
        self.validation = self.validate(cx);
        self.diagnostics = if self.validation.is_ok() {
            self.session.diagnostics(&self.command(cx), cx)
        } else {
            Vec::new()
        };
        self.dialog.set_validation(
            self.validation.is_ok(),
            self.target.is_none() || self.draft(cx) != self.initial,
        );
    }
}

impl FormDialogView for ParameterGroupForm {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        management::management_error(error)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.refresh(cx);
        self.validate(cx)?;
        self.session.execute(self.command(cx), cx)?;
        if let Some(owner) = self.session.owner.upgrade() {
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
        Ok(())
    }

    fn focus_invalid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = match self
            .validation
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref())
        {
            Some(ParameterManagementError::GroupNameRequired) => Some(0),
            Some(
                ParameterManagementError::GroupIdRequired
                | ParameterManagementError::DuplicateGroupId,
            ) => Some(1),
            _ => None,
        };
        if let Some(index) = index {
            window.focus(&self.inputs[index].read(cx).focus_handle(cx), cx);
        }
    }
}

impl Render for ParameterGroupForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .id("parameter-management-form")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2();
        for (i, label) in [t!("editor.name"), t!("parameter.group_id")]
            .into_iter()
            .enumerate()
        {
            body = body.child(
                div().text_xs().child(label.clone()).child(
                    crate::ui::compact::input(Input::new(&self.inputs[i]))
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
        let error = self.dialog.error.clone().or_else(|| {
            self.validation
                .as_ref()
                .err()
                .map(management::management_error)
        });
        body.children(management::export_warning_elements(
            self.diagnostics.clone(),
            cx,
        ))
        .when_some(error, |body, error| {
            body.child(
                div()
                    .id("parameter-management-error")
                    .test_support()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
        })
        .when(self.session.needs_reload(cx), |body| {
            body.child(
                crate::ui::compact::button(Button::new("parameter-management-reload"))
                    .ghost()
                    .label(t!("parameter.reload"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.session.reload(this.action(), window, cx)
                    })),
            )
        })
    }
}
