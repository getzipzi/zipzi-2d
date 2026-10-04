use super::*;
use editor_core::{NewParameter, ParameterCreateError as Error};
use gpui_kit::component::{
    IndexPath,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    dialog::{Cancel, Confirm},
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

pub(crate) struct NewParameterForm {
    owner: WeakEntity<EditorView>,
    document_id: DocumentId,
    focus: FocusHandle,
    inputs: [Entity<InputState>; 6],
    kind: Entity<SelectState<Vec<Choice>>>,
    group: Entity<SelectState<SearchableVec<Choice>>>,
    touched: [bool; 6],
    advanced: bool,
    repeat: bool,
    completed: bool,
    submit_error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl ParametersPanel {
    pub(crate) fn open_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let document_id = {
            let state = owner.read(cx);
            let command = state
                .editor
                .command(editor_core::CommandId::CreateParameter);
            if state.modal || !state.editor.command_available(command) {
                return;
            }
            command.document.unwrap()
        };
        self.finish_slider_drag(cx);
        let result = owner.update(cx, |state, cx| {
            let result = state
                .editor
                .execute_for(document_id, EditorCommand::FinishEdit);
            let ok = result.is_ok();
            state.complete_edit(result, cx);
            ok
        });
        if !result {
            return;
        }
        window.focus(&self.focus, cx);
        let form = NewParameterForm::new(self.owner.clone(), document_id, window, cx);
        let input_focus = form.read(cx).inputs[NAME].read(cx).focus_handle(cx);
        let on_ok = form.clone();
        let on_close = form.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let on_ok = on_ok.clone();
            let on_close = on_close.clone();
            dialog
                .title(t!("parameter.new"))
                .w(px(520.))
                .overlay_closable(false)
                .child(form.clone())
                .footer(div())
                .on_ok(move |_, window, cx| on_ok.update(cx, |form, cx| form.submit(window, cx)))
                .on_close(move |_, _, cx| {
                    on_close.update(cx, |form, _| form.completed = true);
                })
        });
        cx.defer_in(window, move |_, window, cx| window.focus(&input_focus, cx));
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
            self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        }
        cx.notify();
    }
}

impl NewParameterForm {
    fn new(
        owner: WeakEntity<EditorView>,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let state = owner.upgrade().unwrap();
        let doc = state.read(cx).editor.document().unwrap();
        let runtime_id = doc.suggested_parameter_id();
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
        cx.new(|cx| {
            let inputs = [
                t!("parameter.default_name").to_string(),
                runtime_id,
                "-1".into(),
                "1".into(),
                "0".into(),
                "6".into(),
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
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                )
            });
            let group = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(groups),
                    Some(IndexPath::new(0)),
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
                            this.submit_error = None;
                            cx.notify();
                        }
                        InputEvent::PressEnter {
                            secondary: false,
                            shift: false,
                        } => {
                            let focus = this.focus.clone();
                            window.defer(cx, move |window, cx| {
                                focus.dispatch_action(&Confirm { secondary: false }, window, cx)
                            });
                        }
                        _ => {}
                    },
                ));
            }
            subscriptions.push(cx.subscribe(
                &kind,
                |this: &mut Self, _, _: &SelectEvent<Vec<Choice>>, cx| {
                    this.submit_error = None;
                    cx.notify();
                },
            ));
            subscriptions.push(cx.subscribe(
                &group,
                |this: &mut Self, _, _: &SelectEvent<SearchableVec<Choice>>, cx| {
                    this.submit_error = None;
                    cx.notify();
                },
            ));
            subscriptions.push(cx.observe(&state, |_, _, cx| cx.notify()));
            Self {
                owner,
                document_id,
                focus: cx.focus_handle(),
                inputs,
                kind,
                group,
                touched: [false; 6],
                advanced: false,
                repeat: false,
                completed: false,
                submit_error: None,
                _subscriptions: subscriptions,
            }
        })
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
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("parameter.document_changed")))?;
        let state = owner.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(self.document_id),
            t!("parameter.document_changed")
        );
        anyhow::ensure!(
            !state.modal && !state.editor.is_busy() && !state.editor.exiting(),
            t!("parameter.unavailable")
        );
        let draft = self.draft(cx)?;
        state
            .editor
            .document()
            .unwrap()
            .validate_new_parameter(&draft)?;
        Ok(draft)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.completed {
            return false;
        }
        self.touched = [true; 6];
        let result = self.validated_draft(cx).and_then(|draft| {
            let owner = self.owner.upgrade().unwrap();
            owner.update(cx, |state, cx| {
                state
                    .editor
                    .execute_for(self.document_id, EditorCommand::CreateParameter(draft))?;
                state.edit_error = None;
                state.redraw(cx);
                Ok(())
            })
        });
        match result {
            Ok(()) => {
                self.completed = true;
                if let Some(owner) = self.owner.upgrade() {
                    let panel = owner
                        .read(cx)
                        .dock
                        .as_ref()
                        .map(|dock| dock.parameters.clone());
                    if let Some(panel) = panel {
                        panel.update(cx, |panel, cx| panel.locate_created_parameter(window, cx));
                    }
                }
                true
            }
            Err(error) => {
                if error.downcast_ref::<Error>().is_some_and(|error| {
                    matches!(error, Error::PrecisionInvalid | Error::MissingGroup)
                }) {
                    self.advanced = true;
                }
                self.submit_error = Some(error_text(&error));
                cx.notify();
                false
            }
        }
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
                Input::new(&self.inputs[index])
                    .id(field_id(index))
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
        .unwrap_or_else(|| error.to_string())
}

impl Render for NewParameterForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let validation = self.validated_draft(cx);
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
        let focus_confirm = self.focus.clone();
        let focus_cancel = self.focus.clone();
        div()
            .id("new-parameter-form")
            .test_support()
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_3()
            .child(self.field(NAME, field_error, cx))
            .child(self.field(ID, field_error, cx))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.field(MIN, field_error, cx))
                    .child(self.field(MAX, field_error, cx))
                    .child(self.field(DEFAULT, field_error, cx)),
            )
            .child(
                Button::new("new-parameter-advanced")
                    .ghost()
                    .label(t!("parameter.advanced"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.advanced = !this.advanced;
                        cx.notify();
                    })),
            )
            .when(self.advanced, |body| {
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(div().text_xs().child(t!("parameter.kind")))
                        .child(
                            Select::new(&self.kind)
                                .id("new-parameter-kind")
                                .accessibility_label(t!("parameter.kind"))
                                .w_full(),
                        )
                        .child(
                            Checkbox::new("new-parameter-repeat")
                                .label(t!("parameter.repeat").to_string())
                                .checked(self.repeat)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.repeat = *checked;
                                    this.submit_error = None;
                                    cx.notify();
                                })),
                        )
                        .child(self.field(PRECISION, field_error, cx))
                        .child(div().text_xs().child(t!("parameter.group")))
                        .child(
                            Select::new(&self.group)
                                .id("new-parameter-group")
                                .accessibility_label(t!("parameter.group"))
                                .w_full(),
                        ),
                )
            })
            .when_some(self.submit_error.clone().or(global_error), |body, error| {
                body.child(
                    div()
                        .id("new-parameter-error")
                        .test_support()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .py_2()
                    .child(
                        Button::new("new-parameter-cancel")
                            .label(t!("project.cancel"))
                            .on_click(move |_, window, cx| {
                                focus_cancel.dispatch_action(&Cancel, window, cx)
                            }),
                    )
                    .child(
                        Button::new("new-parameter-create")
                            .primary()
                            .label(t!("parameter.create"))
                            .disabled(self.completed || validation.is_err())
                            .on_click(move |_, window, cx| {
                                focus_confirm.dispatch_action(
                                    &Confirm { secondary: false },
                                    window,
                                    cx,
                                )
                            }),
                    ),
            )
    }
}
