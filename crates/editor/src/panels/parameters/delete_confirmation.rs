use super::dialog_session::ParameterDialogSession;
use super::*;
use crate::ui::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use editor_core::{ObjectKind, ObjectRef};

pub(super) enum DeleteTarget {
    Parameter(String),
    Group(String),
}
impl DeleteTarget {
    fn action(&self) -> ParameterAction {
        match self {
            Self::Parameter(id) => ParameterAction::Delete(id.clone()),
            Self::Group(id) => ParameterAction::DeleteGroup(id.clone()),
        }
    }
}

/// A confirmation has no input entities, initial draft or dirty comparison.
pub(super) struct DeleteConfirmation {
    session: ParameterDialogSession,
    dialog: FormDialogState,
    target: DeleteTarget,
    summary: String,
    references: Vec<(String, Option<ObjectRef>)>,
    validation: anyhow::Result<()>,
    diagnostics: Vec<editor_core::ParameterExportDiagnostic>,
    _observe: Subscription,
}

impl DeleteConfirmation {
    pub fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        target: DeleteTarget,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner.upgrade().unwrap();
        let doc = entity.read(cx).editor.document().unwrap();
        let session = ParameterDialogSession::new(owner, document, cx);
        let mut references = Vec::new();
        let summary = match &target {
            DeleteTarget::Parameter(id) => {
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
            DeleteTarget::Group(id) => {
                let initial = doc.parameter_group_draft(id)?;
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
        };
        let form = cx.new(|cx| Self {
            session,
            dialog: FormDialogState::new(cx),
            target,
            summary,
            references,
            validation: Ok(()),
            diagnostics: Vec::new(),
            _observe: cx.observe(&entity, |this: &mut Self, _, cx| {
                if !this.dialog.closed() && this.session.source_changed(cx) {
                    this.refresh(cx);
                    cx.notify();
                }
            }),
        });
        form.update(cx, |form, cx| form.refresh(cx));
        let state = form.read(cx);
        let options = FormDialogOptions {
            title: state
                .target
                .action()
                .label()
                .trim_end_matches('…')
                .to_owned(),
            submit_label: t!("parameter.delete_confirm").into(),
            footer_id: "parameter-management-footer",
            cancel_id: "parameter-management-cancel",
            submit_id: "parameter-management-submit",
            initial_focus: state.dialog.cancel_focus.clone(),
        };
        form_dialog::open(form, options, window, cx);
        Ok(())
    }

    fn validate(&self, cx: &App) -> anyhow::Result<()> {
        self.session.validate(cx)?;
        let owner = self.session.owner.upgrade().unwrap();
        let doc = owner.read(cx).editor.document().unwrap();
        match &self.target {
            DeleteTarget::Parameter(id) => {
                doc.ensure_editable(&[ObjectRef::new(ObjectKind::Parameter, id)])?;
                anyhow::ensure!(
                    doc.parameter_delete_references(id)?.is_empty(),
                    t!("parameter.in_use")
                );
            }
            DeleteTarget::Group(id) => {
                doc.parameter_group_draft(id)?;
                doc.ensure_editable(&[ObjectRef::new(ObjectKind::ParameterGroup, id)])?;
            }
        }
        Ok(())
    }

    fn command(&self) -> EditorCommand {
        let expected_version = self.session.version;
        match &self.target {
            DeleteTarget::Parameter(id) => EditorCommand::DeleteParameter {
                id: id.clone(),
                expected_version,
            },
            DeleteTarget::Group(id) => EditorCommand::DeleteParameterGroup {
                id: id.clone(),
                expected_version,
            },
        }
    }

    fn refresh(&mut self, cx: &App) {
        self.validation = self.validate(cx);
        self.diagnostics = if self.validation.is_ok() {
            self.session.diagnostics(&self.command(), cx)
        } else {
            Vec::new()
        };
        self.dialog.set_validation(self.validation.is_ok(), true);
    }
}

impl FormDialogView for DeleteConfirmation {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        management::management_error(error)
    }
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.refresh(cx);
        self.validate(cx)?;
        self.session.execute(self.command(), cx)
    }
}

impl Render for DeleteConfirmation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .id("parameter-management-form")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .child(self.summary.clone());
        for (i, (label, reference)) in self.references.iter().enumerate() {
            body = body.child(div().text_xs().child(label.clone()).when_some(
                reference.clone(),
                |row, reference| {
                    let owner = self.session.owner.clone();
                    let document = self.session.document;
                    row.child(
                        Button::new(SharedString::from(format!("parameter-reference-{i}")))
                            .ghost()
                            .label(t!("parameter.locate"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let Some(owner) = owner.upgrade() else { return };
                                if owner.read(cx).editor.active_id() != Some(document) {
                                    return;
                                }
                                this.dialog.close();
                                window.close_dialog(cx);
                                owner.update(cx, |state, cx| {
                                    let r = state.editor.execute_for(
                                        document,
                                        EditorCommand::Navigate(reference.clone()),
                                    );
                                    state.complete_edit(r, cx);
                                });
                            })),
                    )
                },
            ));
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
                Button::new("parameter-management-reload")
                    .ghost()
                    .label(t!("parameter.reload"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.session.reload(this.target.action(), window, cx)
                    })),
            )
        })
    }
}
