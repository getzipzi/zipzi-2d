//! Ordinary bindings use captured contexts and explicitly reviewed native plans.
use super::*;
use crate::properties::Choice;
use crate::ui::{
    compact,
    form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView},
};
use editor_core::{OrdinaryBindingAction as A, OrdinaryBindingPlan};
use gpui_kit::component::{
    ActiveTheme, Disableable,
    button::Button,
    input::{Input, InputEvent, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectState},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Control {
    Create,
    Inspect,
    Open,
    Target,
    AddAxis,
    Parameter,
    InsertSample,
    MoveSample,
    DeleteSample,
    RemoveAxis,
    Delete,
    PartDrawOrder,
}
impl Control {
    pub fn key(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Inspect => "inspect",
            Self::Open => "open",
            Self::Target => "target",
            Self::AddAxis => "add_axis",
            Self::Parameter => "parameter",
            Self::InsertSample => "insert_sample",
            Self::MoveSample => "move_sample",
            Self::DeleteSample => "delete_sample",
            Self::RemoveAxis => "remove_axis",
            Self::Delete => "delete",
            Self::PartDrawOrder => "part_draw_order",
        }
    }
    pub fn label(self) -> SharedString {
        let key = format!("binding.{}", self.key());
        t!(&key).into()
    }
    fn parameter(self) -> bool {
        matches!(self, Self::AddAxis | Self::Parameter)
    }
    fn number(self) -> bool {
        matches!(
            self,
            Self::Parameter
                | Self::InsertSample
                | Self::MoveSample
                | Self::RemoveAxis
                | Self::PartDrawOrder
        )
    }
}
impl EditorView {
    pub(crate) fn run_binding_control(
        &mut self,
        context: AuthoringContext,
        source: ObjectRef,
        control: Control,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            anyhow::ensure!(
                !self.modal
                    && !window.has_active_dialog(cx)
                    && AuthoringContext::capture(&self.editor) == Some(context),
                t!("authoring.animation_stale")
            );
            anyhow::ensure!(
                control == Control::Inspect
                    || self.editor.editing_context() == Some(editor_core::EditingContext::Modeling),
                "Modeling context changed"
            );
            anyhow::ensure!(
                self.prepare_property_inputs(context.editing.document, window, cx),
                t!("authoring.candidate_active")
            );
            let context = AuthoringContext::capture(&self.editor).unwrap();
            if control == Control::Inspect {
                self.inspect_authoring_object(context.editing.document, source, false, window, cx)?;
                self.redraw(cx);
            } else if matches!(control, Control::Open | Control::Target) {
                let b = self
                    .editor
                    .document()
                    .unwrap()
                    .ordinary_binding(&source)
                    .ok_or_else(|| anyhow::anyhow!("Ordinary binding no longer exists"))?;
                let object = if control == Control::Open {
                    b.reference
                } else {
                    b.target
                };
                self.inspect_authoring_object(context.editing.document, object, false, window, cx)?;
                self.redraw(cx);
            } else if control == Control::Create {
                let plan = self
                    .editor
                    .document()
                    .unwrap()
                    .plan_ordinary_binding(&A::Create(source))?;
                self.apply_binding(context, &plan, window, cx)?;
            } else {
                let owner = cx.weak_entity();
                window.defer(cx, move |window, cx| {
                    if let Err(error) =
                        BindingDialog::open(owner.clone(), context, source, control, window, cx)
                    {
                        let _ =
                            owner.update(cx, |v, cx| v.set_edit_error(authoring_error(&error), cx));
                    }
                });
            }
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(error) = result {
            self.set_edit_error(authoring_error(&error), cx);
        }
    }
    fn apply_binding(
        &mut self,
        context: AuthoringContext,
        plan: &OrdinaryBindingPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.modal && AuthoringContext::capture(&self.editor) == Some(context),
            t!("authoring.animation_stale")
        );
        self.editor
            .execute_ordinary_binding(context.editing.document, plan)?;
        // Open the owner first so the canvas and keyform pad share the binding.
        if !plan.deleted_binding {
            self.inspect_authoring_object(
                context.editing.document,
                plan.binding.clone(),
                false,
                window,
                cx,
            )?;
        }
        self.inspect_authoring_object(
            context.editing.document,
            plan.selection.clone(),
            false,
            window,
            cx,
        )?;
        self.clear_edit_error(cx);
        self.redraw(cx);
        Ok(())
    }
}
struct BindingDialog {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    source: ObjectRef,
    control: Control,
    dialog: FormDialogState,
    parameter: Entity<SelectState<SearchableVec<Choice>>>,
    number: Entity<InputState>,
    plan: Option<OrdinaryBindingPlan>,
    rows: std::sync::Arc<[String]>,
    reason: Option<String>,
    preview_revision: u64,
    conversion: kasane_sdk::ConversionControl,
    serial: u64,
    working: bool,
    task: Option<Task<()>>,
    inspected: Option<ObjectRef>,
    _subscriptions: Vec<Subscription>,
}
impl BindingDialog {
    fn current(&self, cx: &App) -> bool {
        !self.dialog.closed()
            && self.owner.upgrade().is_some_and(|owner| {
                let view = owner.read(cx);
                AuthoringContext::capture(&view.editor) == Some(self.context)
                    && !view.modal
                    && view.editor.active_inspection().and_then(|i| i.object()) == self.inspected
                    && view
                        .editor
                        .document()
                        .is_some_and(|d| d.preview_revision() == self.preview_revision)
            })
    }
    fn cancel_work(&mut self) {
        self.conversion.cancel();
        self.serial += 1;
        self.task = None;
        self.working = false;
    }
    fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.cancel_work();
        self.plan = None;
        self.rows = [].into();
        self.reason = None;
        self.dialog.edited();
        self.dialog.set_validation(false, true);
        cx.notify();
    }
    fn action(&self, cx: &App) -> anyhow::Result<A> {
        let number = || -> anyhow::Result<f32> {
            let v: f32 = self
                .number
                .read(cx)
                .value()
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!(t!("binding.invalid_number")))?;
            anyhow::ensure!(v.is_finite(), t!("binding.invalid_number"));
            Ok(v)
        };
        let parameter = || {
            self.parameter
                .read(cx)
                .selected_value()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!(t!("binding.choose_parameter")))
        };
        let source = self.source.clone();
        Ok(match self.control {
            Control::AddAxis => A::AddAxis {
                binding: source,
                parameter: parameter()?,
            },
            Control::Parameter => A::SetParameter {
                axis: source,
                parameter: parameter()?,
                reduction: Some(number()?),
            },
            Control::InsertSample => A::InsertSample {
                axis: source,
                value: number()?,
            },
            Control::MoveSample => A::MoveSample {
                sample: source,
                value: number()?,
            },
            Control::DeleteSample => A::DeleteSample(source),
            Control::RemoveAxis => A::RemoveAxis {
                axis: source,
                reduction: Some(number()?),
            },
            Control::Delete => A::Delete(source),
            Control::PartDrawOrder => A::PartDrawOrder {
                binding: source,
                value: number()?,
            },
            _ => anyhow::bail!("Expected a binding edit"),
        })
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        self.invalidate(cx);
        let capture = (|| {
            anyhow::ensure!(self.current(cx), t!("authoring.animation_stale"));
            self.owner
                .upgrade()
                .unwrap()
                .read(cx)
                .editor
                .document()
                .unwrap()
                .capture_ordinary_binding(&self.action(cx)?)
        })();
        let capture = match capture {
            Ok(c) => c,
            Err(e) => {
                self.reason = Some(authoring_error(&e));
                cx.notify();
                return;
            }
        };
        self.conversion = kasane_sdk::ConversionControl::default();
        let control = self.conversion.clone();
        let serial = self.serial;
        self.working = true;
        let task = cx
            .background_executor()
            .spawn(async move { capture.plan(&control) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.serial != serial || !this.current(cx) {
                    return;
                }
                this.working = false;
                this.task = None;
                this.complete_review(result, cx);
            });
        }));
        cx.notify();
    }
    fn complete_review(
        &mut self,
        result: anyhow::Result<OrdinaryBindingPlan>,
        cx: &mut Context<Self>,
    ) {
        self.plan = None;
        self.rows = [].into();
        self.reason = None;
        match result {
            Ok(plan) => {
                let mut rows = vec![format!("{}: {}", t!("binding.affected"), plan.target.id)];
                if let Some(report) = &plan.report {
                    rows.push(format!(
                        "{}: {} / {} / {}",
                        t!("binding.change_counts"),
                        report.generated.len(),
                        report.rewritten.len(),
                        report.deleted.len()
                    ));
                    if let Some(guarantee) = &report.grid_guarantee {
                        rows.push(format!("{}: {guarantee:?}", t!("binding.guarantee")));
                    }
                    rows.extend(report.losses.iter().cloned());
                    rows.extend(
                        report
                            .affected
                            .iter()
                            .map(|id| format!("{}: {id}", t!("binding.affected"))),
                    );
                    rows.extend(report.deleted.iter().map(|address| {
                        format!(
                            "{}: {} · {:?}",
                            t!("binding.deleted"),
                            address.owner,
                            address.element
                        )
                    }));
                    rows.push(format!(
                        "{}: {} · {} / {}",
                        t!("binding.cost"),
                        plan.cost.sample_combinations,
                        plan.cost.temporary_bytes,
                        plan.cost.history_bytes
                    ));
                }
                self.rows = rows.into();
                self.plan = Some(plan);
            }
            Err(error) => self.reason = Some(authoring_error(&error)),
        }
        self.dialog.edited();
        self.dialog.set_validation(self.plan.is_some(), true);
        cx.notify();
    }
    fn open(
        owner: WeakEntity<EditorView>,
        context: AuthoringContext,
        source: ObjectRef,
        control: Control,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        anyhow::ensure!(
            AuthoringContext::capture(&entity.read(cx).editor) == Some(context),
            t!("authoring.animation_stale")
        );
        let doc = entity.read(cx).editor.document().unwrap();
        let b = doc
            .ordinary_binding(&source)
            .ok_or_else(|| anyhow::anyhow!("Ordinary binding no longer exists"))?;
        let axis = b
            .axes
            .iter()
            .find(|a| a.id == source.id || a.samples.iter().any(|s| s.id == source.id));
        let parameter = axis.map(|a| a.parameter_id.clone()).unwrap_or_default();
        let parameters = doc.parameters();
        let number = axis
            .and_then(|a| {
                a.samples
                    .iter()
                    .find(|s| s.id == source.id)
                    .map(|s| s.value)
                    .or_else(|| a.keys.first().copied())
            })
            .unwrap_or(0.);
        let number = if control == Control::PartDrawOrder {
            doc.ordinary_part_draw_order(&b.reference)?
        } else {
            number
        };
        let preview_revision = doc.preview_revision();
        let inspected = entity
            .read(cx)
            .editor
            .active_inspection()
            .and_then(|i| i.object());
        let mut choices = vec![Choice {
            value: String::new(),
            label: t!("binding.unconfigured").into(),
        }];
        choices.extend(parameters.iter().map(|p| Choice {
            value: p.id.clone(),
            label: format!("{} · {}", p.name, p.id).into(),
        }));
        if !parameter.is_empty() && !choices.iter().any(|c| c.value == parameter) {
            choices.push(Choice {
                value: parameter.clone(),
                label: format!("{} · {}", t!("editor.missing_reference"), parameter).into(),
            });
        }
        let selected = choices
            .iter()
            .position(|c| c.value == parameter)
            .map(gpui_kit::component::IndexPath::new);
        let view = cx.new(|cx| {
            let parameter = cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices), selected, window, cx).searchable(true)
            });
            let number = cx.new(|cx| InputState::new(window, cx).default_value(number.to_string()));
            let parameter_event =
                cx.subscribe_in(&parameter, window, |this: &mut Self, _, event, _, cx| {
                    if matches!(event, SelectEvent::Confirm(_)) {
                        this.invalidate(cx);
                    }
                });
            let number_event = cx.subscribe_in(
                &number,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.invalidate(cx);
                    }
                },
            );
            let observe = cx.observe(&entity, |this: &mut Self, _, cx| {
                if !this.current(cx) {
                    this.cancel_work();
                    this.plan = None;
                    this.dialog.set_validation(false, true);
                    cx.notify();
                }
            });
            Self {
                owner,
                context,
                source,
                control,
                dialog: FormDialogState::new(cx),
                parameter,
                number,
                plan: None,
                rows: [].into(),
                reason: None,
                preview_revision,
                conversion: Default::default(),
                serial: 0,
                working: false,
                task: None,
                inspected,
                _subscriptions: vec![parameter_event, number_event, observe],
            }
        });
        let focus = view.read(cx).dialog.cancel_focus.clone();
        if matches!(control, Control::Delete | Control::DeleteSample) {
            view.update(cx, |v, cx| v.review(cx));
        }
        form_dialog::open(
            view,
            FormDialogOptions {
                title: control.label().to_string(),
                submit_label: t!("binding.apply").into(),
                footer_id: "binding-dialog-footer",
                cancel_id: "binding-dialog-cancel",
                submit_id: "binding-dialog-submit",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
}
impl Drop for BindingDialog {
    fn drop(&mut self) {
        self.conversion.cancel();
    }
}
impl FormDialogView for BindingDialog {
    fn closing(&mut self) {
        self.cancel_work();
    }

    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        authoring_error(error)
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(self.current(cx), t!("authoring.animation_stale"));
        let plan = self
            .plan
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(t!("binding.review_required")))?;
        self.owner
            .update(cx, |v, cx| v.apply_binding(self.context, plan, window, cx))??;
        Ok(())
    }
}
impl Render for BindingDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current(cx);
        let mut body = div()
            .id("binding-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .w_full();
        if self.control == Control::Delete {
            body = body.child(t!("binding.delete_hint"));
        }
        if self.control.parameter() {
            body = body
                .child(div().child(t!("binding.choose_parameter")))
                .child(
                    compact::select(Select::new(&self.parameter))
                        .id("binding-parameter")
                        .disabled(!current),
                );
        }
        if self.control.number() {
            body = body
                .child(div().child(t!(if matches!(
                    self.control,
                    Control::RemoveAxis | Control::Parameter
                ) {
                    "binding.reduction"
                } else if self.control == Control::PartDrawOrder {
                    "properties.draw_order"
                } else {
                    "binding.value"
                })))
                .child(
                    compact::input(Input::new(&self.number))
                        .id("binding-value")
                        .disabled(!current),
                );
        }
        body = body.child(
            compact::button(Button::new("binding-review"))
                .label(t!("binding.review"))
                .disabled(!current || self.working)
                .on_click(cx.listener(|v, _, _, cx| v.review(cx))),
        );
        if self.working {
            body = body.child(t!("binding.planning"));
        }
        let rows = self.rows.clone();
        if !rows.is_empty() {
            body = body.child(
                uniform_list("binding-review-rows", rows.len(), move |range, _, _| {
                    range
                        .map(|i| div().text_xs().overflow_hidden().child(rows[i].clone()))
                        .collect()
                })
                .h(compact::rem(150.)),
            );
        }
        if let Some(error) = self.reason.as_ref().or(self.dialog.error.as_ref()) {
            body = body.child(div().text_color(cx.theme().red).child(error.clone()));
        }
        if !current {
            body = body.child(t!("authoring.animation_stale"));
        }
        body
    }
}
