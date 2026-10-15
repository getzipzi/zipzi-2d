use super::dialog_session::ParameterDialogSession;
use super::*;
use crate::ui::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use editor_core::{NewParameter, ParameterCreateError as Error};
use gpui_kit::component::{
    IndexPath,
    button::Button,
    checkbox::Checkbox,
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
};

#[derive(Clone)]
struct Choice {
    label: SharedString,
    value: String,
}

impl SelectItem for Choice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &String {
        &self.value
    }
}

const NAME: usize = 0;
const ID: usize = 1;
const MIN: usize = 2;
const MAX: usize = 3;
const DEFAULT: usize = 4;
const PRECISION: usize = 5;

pub(crate) struct ParameterForm {
    session: ParameterDialogSession,
    target: Option<String>,
    initial: NewParameter,
    dialog: FormDialogState,
    validation: anyhow::Result<NewParameter>,
    diagnostics: Vec<editor_core::ParameterExportDiagnostic>,
    inputs: [Entity<InputState>; 6],
    kind: Entity<SelectState<Vec<Choice>>>,
    group: Entity<SelectState<SearchableVec<Choice>>>,
    touched: [bool; 6],
    repeat: bool,
    _subscriptions: Vec<Subscription>,
}

impl ParametersPanel {
    pub(crate) fn open_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_parameter_dialog(None, None, window, cx);
    }

    pub(crate) fn open_parameter_dialog(
        &mut self,
        target: Option<String>,
        group: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.prepare_parameter_dialog(window, cx) else {
            return;
        };
        let owner = self.owner.upgrade().unwrap();
        let doc = owner.read(cx).editor.document().unwrap();
        let initial = if let Some(id) = &target {
            let Ok(draft) = doc.parameter_draft(id) else {
                return;
            };
            draft
        } else {
            NewParameter {
                name: t!("parameter.default_name").into_owned(),
                runtime_id: doc.suggested_parameter_id(),
                group_id: group,
                ..Default::default()
            }
        };
        let editing = target.is_some();
        let form = ParameterForm::new(self.owner.clone(), document_id, target, initial, window, cx);
        let initial_focus = form.read(cx).inputs[NAME].read(cx).focus_handle(cx);
        form_dialog::open(
            form,
            FormDialogOptions {
                title: if editing {
                    t!("parameter.edit").trim_end_matches('…').to_owned()
                } else {
                    t!("parameter.new").into_owned()
                },
                submit_label: if editing {
                    t!("parameter.save")
                } else {
                    t!("parameter.create")
                }
                .into(),
                footer_id: "new-parameter-footer",
                cancel_id: "new-parameter-cancel",
                submit_id: "new-parameter-create",
                initial_focus,
            },
            window,
            cx,
        );
        cx.notify();
    }

    pub(crate) fn locate_created_parameter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_with_editor(window, cx);
        let focus = self.owner.upgrade().and_then(|owner| {
            owner
                .read(cx)
                .editor
                .document()?
                .context()
                .parameter_focus
                .clone()
        });
        if let Some(index) = focus.and_then(|focus| {
            self.rows
                .iter()
                .position(|row| row.parameter.id == focus.id)
        }) {
            self.layout.reveal(index, &mut self.collapsed_groups);
            self.refresh_visible_rows(cx);
            if let Some(visible_index) = self.visible_rows.iter().position(|row| {
                matches!(row, ListRow::Parameter { index: row_index, .. } if *row_index == index)
            }) {
                self.scroll.scroll_to_item(visible_index, ScrollStrategy::Center);
            }
        }
        cx.notify();
    }
}

impl ParameterForm {
    fn new(
        owner: WeakEntity<EditorView>,
        document_id: DocumentId,
        target: Option<String>,
        initial: NewParameter,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let state = owner.upgrade().unwrap();
        let doc = state.read(cx).editor.document().unwrap();
        let session = ParameterDialogSession::new(owner.clone(), document_id, cx);
        let mut groups = vec![Choice {
            label: t!("parameter.ungrouped").into(),
            value: String::new(),
        }];
        groups.extend(
            doc.parameter_group_options()
                .into_iter()
                .map(|group| Choice {
                    label: group.path.into(),
                    value: group.id,
                }),
        );
        let group_index = groups
            .iter()
            .position(|g| Some(g.value.as_str()) == initial.group_id.as_deref())
            .unwrap_or(0);
        let form = cx.new(|cx| {
            let inputs = [
                initial.name.clone(),
                initial.runtime_id.clone(),
                initial.minimum.to_string(),
                initial.maximum.to_string(),
                initial.default_value.to_string(),
                initial.decimal_places.to_string(),
            ]
            .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value)));
            let kind = cx.new(|cx| {
                SelectState::new(
                    vec![
                        Choice {
                            label: t!("parameter.normal").into(),
                            value: "normal".into(),
                        },
                        Choice {
                            label: "BlendShape".into(),
                            value: "blend_shape".into(),
                        },
                    ],
                    Some(IndexPath::new(usize::from(
                        initial.kind == kasane_core::ParameterKind::BlendShape,
                    ))),
                    window,
                    cx,
                )
            });
            let group = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(groups),
                    Some(IndexPath::new(group_index)),
                    window,
                    cx,
                )
                .searchable(true)
            });
            let mut subscriptions = Vec::new();
            for (index, input) in inputs.iter().enumerate() {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    move |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                        InputEvent::Change | InputEvent::Blur => {
                            this.touched[index] = true;
                            if matches!(event, InputEvent::Change) {
                                this.dialog.edited();
                                this.refresh(cx);
                            }
                            cx.notify();
                        }
                        InputEvent::PressEnter { .. } => {
                            form_dialog::input_enter(event, window, cx)
                        }
                        _ => {}
                    },
                ));
            }
            subscriptions.push(cx.subscribe(
                &kind,
                |this: &mut Self, _, _: &SelectEvent<Vec<Choice>>, cx| {
                    this.dialog.edited();
                    this.refresh(cx);
                    cx.notify();
                },
            ));
            subscriptions.push(cx.subscribe(
                &group,
                |this: &mut Self, _, _: &SelectEvent<SearchableVec<Choice>>, cx| {
                    this.dialog.edited();
                    this.refresh(cx);
                    cx.notify();
                },
            ));
            subscriptions.push(cx.observe(&state, |this: &mut Self, _, cx| {
                if !this.dialog.closed() && this.session.source_changed(cx) {
                    this.refresh(cx);
                    cx.notify();
                }
            }));
            Self {
                session,
                dialog: FormDialogState::new(cx),
                validation: Ok(initial.clone()),
                diagnostics: Vec::new(),
                inputs,
                kind,
                group,
                touched: [false; 6],
                repeat: initial.repeat,
                target,
                initial,
                _subscriptions: subscriptions,
            }
        });
        form.update(cx, |form, cx| form.refresh(cx));
        form
    }

    fn draft(&self, cx: &App) -> Result<NewParameter, Error> {
        let value = |i: usize| self.inputs[i].read(cx).value().to_string();
        let number = |i, error| {
            value(i)
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or(error)
        };
        Ok(NewParameter {
            name: value(NAME),
            runtime_id: value(ID),
            minimum: number(MIN, Error::MinimumInvalid)?,
            maximum: number(MAX, Error::MaximumInvalid)?,
            default_value: number(DEFAULT, Error::DefaultInvalid)?,
            decimal_places: value(PRECISION)
                .trim()
                .parse()
                .map_err(|_| Error::PrecisionInvalid)?,
            kind: if self
                .kind
                .read(cx)
                .selected_value()
                .is_some_and(|value| value == "blend_shape")
            {
                kasane_core::ParameterKind::BlendShape
            } else {
                kasane_core::ParameterKind::Normal
            },
            repeat: self.repeat,
            group_id: self
                .group
                .read(cx)
                .selected_value()
                .filter(|value| !value.is_empty())
                .cloned(),
        })
    }

    fn validated_draft(&self, cx: &App) -> anyhow::Result<NewParameter> {
        self.session.validate(cx)?;
        let owner = self.session.owner.upgrade().unwrap();
        let state = owner.read(cx);
        let doc = state.editor.document().unwrap();
        let draft = self.draft(cx)?;
        if let Some(id) = &self.target {
            doc.validate_parameter_update(id, &draft)?;
            return Ok(draft);
        }
        state
            .editor
            .document()
            .unwrap()
            .validate_new_parameter(&draft)?;
        Ok(draft)
    }

    fn command(&self, draft: NewParameter) -> EditorCommand {
        if let Some(id) = &self.target {
            EditorCommand::UpdateParameter {
                id: id.clone(),
                draft,
                expected_version: self.session.version,
            }
        } else {
            EditorCommand::CreateParameterChecked {
                draft,
                expected_version: self.session.version,
            }
        }
    }

    fn refresh(&mut self, cx: &App) {
        self.validation = self.validated_draft(cx);
        self.diagnostics = self
            .validation
            .as_ref()
            .ok()
            .map(|draft| self.session.diagnostics(&self.command(draft.clone()), cx))
            .unwrap_or_default();
        self.dialog
            .set_validation(self.validation.is_ok(), self.changed(cx));
    }

    fn changed(&self, cx: &App) -> bool {
        self.target.is_none()
            || self.draft(cx).map_or(true, |mut draft| {
                draft.name = draft.name.trim().into();
                draft.runtime_id = draft.runtime_id.trim().into();
                draft != self.initial
            })
    }

    fn field(&self, index: usize, error: Option<Error>, cx: &App) -> AnyElement {
        let label = field_label(index);
        let message = error
            .filter(|error| error_field(*error) == Some(index))
            .filter(|_| {
                self.touched[index]
                    || matches!(index, MIN | MAX) && (self.touched[MIN] || self.touched[MAX])
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(div().text_xs().child(label.clone()))
            .child(
                crate::ui::compact::input(Input::new(&self.inputs[index]))
                    .id(field_id(index))
                    .small()
                    .aria_label(label),
            )
            .when_some(message, |field, message| {
                field.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error_label(message)),
                )
            })
            .into_any_element()
    }
}

fn field_label(index: usize) -> SharedString {
    t!(match index {
        NAME => "editor.name",
        ID => "parameter.id",
        MIN => "parameter.minimum",
        MAX => "parameter.maximum",
        DEFAULT => "parameter.default_value",
        _ => "parameter.precision",
    })
    .into()
}

fn field_id(index: usize) -> &'static str {
    [
        "new-parameter-name",
        "new-parameter-id",
        "new-parameter-minimum",
        "new-parameter-maximum",
        "new-parameter-default",
        "new-parameter-precision",
    ][index]
}

fn error_field(error: Error) -> Option<usize> {
    Some(match error {
        Error::NameRequired => NAME,
        Error::IdRequired | Error::DuplicateId => ID,
        Error::MinimumInvalid => MIN,
        Error::MaximumInvalid | Error::RangeInvalid | Error::RangeOverflow => MAX,
        Error::DefaultInvalid | Error::DefaultOutOfRange => DEFAULT,
        Error::PrecisionInvalid => PRECISION,
        Error::MissingGroup => return None,
    })
}

fn error_label(error: Error) -> SharedString {
    t!(match error {
        Error::NameRequired => "parameter.name_required",
        Error::IdRequired => "parameter.id_required",
        Error::DuplicateId => "parameter.duplicate_id",
        Error::MinimumInvalid | Error::MaximumInvalid | Error::DefaultInvalid =>
            "parameter.invalid_number",
        Error::RangeInvalid => "parameter.invalid_range",
        Error::RangeOverflow => "parameter.range_overflow",
        Error::DefaultOutOfRange => "parameter.default_out_of_range",
        Error::PrecisionInvalid => "parameter.invalid_precision",
        Error::MissingGroup => "parameter.missing_group",
    })
    .into()
}

fn error_text(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<Error>()
        .map(|error| error_label(*error).to_string())
        .unwrap_or_else(|| super::management::management_error(error))
}

impl FormDialogView for ParameterForm {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        error_text(error)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.touched = [true; 6];
        self.refresh(cx);
        let draft = self.validated_draft(cx)?;
        self.session.execute(self.command(draft), cx)?;
        if let Some(owner) = self.session.owner.upgrade() {
            let panel = owner
                .read(cx)
                .dock
                .as_ref()
                .map(|dock| dock.parameters.clone());
            if let Some(panel) = panel {
                panel.update(cx, |panel, cx| panel.locate_created_parameter(window, cx));
            }
        }
        Ok(())
    }

    fn focus_invalid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .validation
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<Error>())
            .copied()
            .and_then(error_field)
        {
            window.focus(&self.inputs[index].read(cx).focus_handle(cx), cx);
        }
    }
}

impl Render for ParameterForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let validation = &self.validation;
        let mut details = super::management::export_warning_elements(self.diagnostics.clone(), cx);
        if let Some(editor_core::ParameterManagementError::Dependency { references, .. }) =
            validation.as_ref().err().and_then(|e| e.downcast_ref())
        {
            details = super::management::reference_elements(
                self.session.owner.clone(),
                self.session.document,
                references,
                cx,
            );
        }
        let field_error = validation
            .as_ref()
            .err()
            .and_then(|error| error.downcast_ref::<Error>())
            .copied();
        let global_error = validation
            .as_ref()
            .err()
            .filter(|error| {
                error.downcast_ref::<Error>().is_none() || field_error == Some(Error::MissingGroup)
            })
            .map(error_text);
        div()
            .id(if self.target.is_some() {
                "edit-parameter-form"
            } else {
                "new-parameter-form"
            })
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(self.field(NAME, field_error, cx))
                    .child(self.field(ID, field_error, cx)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(self.field(MIN, field_error, cx))
                    .child(self.field(MAX, field_error, cx))
                    .child(self.field(DEFAULT, field_error, cx))
                    .child(self.field(PRECISION, field_error, cx)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_xs().child(t!("parameter.kind")))
                            .child(
                                crate::ui::compact::select(Select::new(&self.kind))
                                    .id("new-parameter-kind")
                                    .accessibility_label(t!("parameter.kind"))
                                    .w_full(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_xs().child(t!("parameter.group")))
                            .child(
                                crate::ui::compact::select(Select::new(&self.group))
                                    .id("new-parameter-group")
                                    .accessibility_label(t!("parameter.group"))
                                    .w_full(),
                            ),
                    ),
            )
            .children(details)
            .when_some(self.dialog.error.clone().or(global_error), |body, error| {
                body.child(
                    div()
                        .id("new-parameter-error")
                        .test_support()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .when(self.target.is_some(), |body| {
                body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("parameter.id_hint")),
                )
            })
            .when(self.session.needs_reload(cx), |body| {
                body.child(
                    crate::ui::compact::button(Button::new("parameter-reload"))
                        .label(t!("parameter.reload"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            let action = this
                                .target
                                .clone()
                                .map(ParameterAction::Edit)
                                .unwrap_or_else(|| {
                                    ParameterAction::Create(this.initial.group_id.clone())
                                });
                            this.session.reload(action, window, cx);
                        })),
                )
            })
            .child(
                Checkbox::new("new-parameter-repeat")
                    .small()
                    .mt_1()
                    .label(t!("parameter.repeat").to_string())
                    .checked(self.repeat)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.repeat = *checked;
                        this.dialog.edited();
                        this.refresh(cx);
                        cx.notify();
                    })),
            )
    }
}
