//! Preservation review: one frozen capture, one selected target, one cancellable
//! worker. Confirmation publishes the reviewed plan without rebuilding it.
use super::*;
use crate::{
    t,
    ui::{
        compact,
        form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView},
    },
};
use gpui_kit::component::{
    Disableable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
};
use zipzi_editor_core::{
    PalettePreservation as Scope, PaletteReparentCapture, PaletteReparentPlan,
};
use zipzi_sdk::ConversionControl;

#[derive(Clone)]
struct Choice {
    key: String,
    object: Option<ObjectRef>,
    label: SharedString,
}
impl SelectItem for Choice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &String {
        &self.key
    }
}
struct ReparentReview {
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    context: AuthoringContext,
    preview_revision: u64,
    capture: PaletteReparentCapture,
    sources: Vec<ObjectRef>,
    fixed: Option<PaletteMove>,
    choices: Vec<Choice>,
    destination: Entity<SelectState<SearchableVec<Choice>>>,
    scope: Scope,
    tolerance: [Entity<InputState>; 2],
    _tolerance_events: Vec<Subscription>,
    plan: Option<PaletteReparentPlan>,
    reason: Option<String>,
    names: std::collections::HashMap<String, String>,
    control: ConversionControl,
    serial: u64,
    working: bool,
    task: Option<Task<()>>,
    dialog: FormDialogState,
    _event: Subscription,
    _owner: Subscription,
    #[cfg(test)]
    plans_built: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl Drop for ReparentReview {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
impl ReparentReview {
    fn current(&self, cx: &App) -> bool {
        !self.dialog.closed()
            && self.owner.upgrade().is_some_and(|owner| {
                let state = owner.read(cx);
                !state.modal
                    && AuthoringContext::capture(&state.editor) == Some(self.context)
                    && state
                        .editor
                        .document()
                        .is_some_and(|d| d.preview_revision() == self.preview_revision)
            })
    }
    fn cancel_work(&mut self) {
        self.control.cancel();
        self.serial += 1;
        self.task = None;
        self.working = false;
        self.plan = None;
        self.dialog.set_validation(false, true);
    }
    fn request(&self, cx: &App) -> anyhow::Result<PaletteMove> {
        if let Some(fixed) = &self.fixed {
            return Ok(fixed.clone());
        }
        let key = self.destination.read(cx).selected_value();
        let choice = self
            .choices
            .iter()
            .find(|c| Some(&c.key) == key)
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_choose")))?;
        Ok(PaletteMove {
            projection: PaletteProjection::Deformation,
            sources: self.sources.clone(),
            target: choice.object.clone(),
            position: DropPosition::Inside,
        })
    }
    fn refresh_plan(&mut self, cx: &mut Context<Self>) {
        self.cancel_work();
        self.dialog.edited();
        if !self.current(cx) {
            self.reason = Some(t!("authoring.move_stale").into_owned());
            cx.notify();
            return;
        }
        let request = match self.request(cx) {
            Ok(request) => request,
            Err(error) => {
                self.reason = Some(move_error(&error));
                cx.notify();
                return;
            }
        };
        let tolerance = self
            .tolerance
            .each_ref()
            .map(|input| input.read(cx).value().trim().parse::<f64>());
        let [Ok(absolute), Ok(relative)] = tolerance else {
            self.reason = Some(t!("reparent.invalid_tolerance").into_owned());
            cx.notify();
            return;
        };
        if !absolute.is_finite() || absolute <= 0. || !relative.is_finite() || relative < 0. {
            self.reason = Some(t!("reparent.invalid_tolerance").into_owned());
            cx.notify();
            return;
        }
        self.reason = None;
        self.working = true;
        self.control = ConversionControl::default();
        let control = self.control.clone();
        let capture = self.capture.clone();
        let scope = self.scope;
        let serial = self.serial;
        #[cfg(test)]
        let plans_built = self.plans_built.clone();
        let executor = cx.background_executor().clone();
        let debounce = executor.timer(Duration::from_millis(120));
        let background = executor.spawn(async move {
            debounce.await;
            if control.is_cancelled() {
                return Err(anyhow::anyhow!("Conversion cancelled"));
            }
            #[cfg(test)]
            plans_built.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            capture.plan_with_tolerance(request, scope, absolute, relative, &control)
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            _ = this.update(cx, |this, cx| {
                if this.serial != serial || !this.current(cx) {
                    return;
                }
                this.task = None;
                this.working = false;
                match result {
                    Ok(plan) if !plan.scene().is_noop() => {
                        this.plan = Some(plan);
                        this.reason = None;
                    }
                    Ok(_) => {
                        this.reason = Some(t!("authoring.move_noop").into_owned());
                    }
                    Err(error) => {
                        this.reason = Some(move_error(&error));
                    }
                }
                this.dialog.edited();
                this.dialog.set_validation(this.plan.is_some(), true);
                cx.notify();
            });
        }));
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let running = this
                    .update(cx, |this, cx| {
                        if this.serial != serial || !this.working {
                            return false;
                        }
                        if !this.current(cx) {
                            this.cancel_work();
                            this.reason = Some(t!("authoring.move_stale").into_owned());
                        }
                        cx.notify();
                        this.working
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        expected: AuthoringContext,
        sources: Vec<ObjectRef>,
        fixed: Option<PaletteMove>,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<Entity<Self>> {
        anyhow::ensure!(!window.has_active_dialog(cx), t!("authoring.move_stale"));
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_stale")))?;
        let context = entity.update(cx, |state, cx| {
            anyhow::ensure!(
                !state.modal
                    && AuthoringContext::capture(&state.editor) == Some(expected)
                    && state.editor.active_id() == Some(document)
                    && state.editor.editing_context()
                        == Some(zipzi_editor_core::EditingContext::Modeling),
                t!("authoring.move_stale")
            );
            anyhow::ensure!(
                state.prepare_property_inputs(document, window, cx),
                t!("authoring.move_stale")
            );
            state
                .editor
                .end_editing(document, zipzi_editor_core::EditingLifecycle::BlockingUi)?;
            state.redraw(cx);
            Ok::<_, anyhow::Error>(AuthoringContext::capture(&state.editor).unwrap())
        })?;
        let doc = entity.read(cx).editor.document().unwrap();
        let preview_revision = doc.preview_revision();
        let capture = doc.capture_palette_reparent()?;
        let destinations = doc.palette_move_destinations(PaletteProjection::Deformation)?;
        let mut counts = std::collections::HashMap::new();
        for destination in &destinations {
            *counts.entry(destination.path.as_str()).or_insert(0) += 1;
        }
        let choices: Vec<_> = destinations
            .iter()
            .map(|d| {
                let label = match &d.object {
                    None => t!("authoring.move_root").into_owned(),
                    Some(object) if counts[d.path.as_str()] > 1 => {
                        format!("{} · {}", d.path, object.id)
                    }
                    Some(_) => d.path.clone(),
                };
                Choice {
                    key: d
                        .object
                        .as_ref()
                        .map_or_else(|| "root".into(), |o| o.id.clone()),
                    object: d.object.clone(),
                    label: label.into(),
                }
            })
            .collect();
        let index = doc.object_index();
        let names = index
            .objects
            .iter()
            .map(|info| {
                (
                    info.reference.id.clone(),
                    index.name(&info.reference).to_owned(),
                )
            })
            .collect();
        let view = cx.new(|cx| {
            let destination = cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices.clone()), None, window, cx)
                    .searchable(true)
            });
            let event =
                cx.subscribe_in(&destination, window, |this: &mut Self, _, event, _, cx| {
                    if matches!(event, SelectEvent::Confirm(_)) && !this.dialog.closed() {
                        this.refresh_plan(cx);
                    }
                });
            let observe = cx.observe(&entity, |this: &mut Self, _, cx| {
                if !this.current(cx) {
                    this.cancel_work();
                    this.reason = Some(t!("authoring.move_stale").into_owned());
                }
                cx.notify();
            });
            let tolerance = ["0.00001", "0.000001"]
                .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value)));
            let tolerance_events = tolerance
                .iter()
                .map(|input| {
                    cx.subscribe_in(input, window, |this: &mut Self, _, event, _, cx| {
                        if matches!(event, InputEvent::Change) && !this.dialog.closed() {
                            this.refresh_plan(cx);
                        }
                    })
                })
                .collect();
            let mut dialog = FormDialogState::new(cx);
            dialog.set_validation(false, true);
            Self {
                owner,
                document,
                context,
                preview_revision,
                capture,
                sources,
                fixed,
                choices,
                destination,
                scope: Scope::EditState,
                tolerance,
                _tolerance_events: tolerance_events,
                plan: None,
                reason: None,
                names,
                control: ConversionControl::default(),
                serial: 0,
                working: false,
                task: None,
                dialog,
                _event: event,
                _owner: observe,
                #[cfg(test)]
                plans_built: Default::default(),
            }
        });
        let focus = view.read(cx).dialog.focus.clone();
        form_dialog::open(
            view.clone(),
            FormDialogOptions {
                title: t!("reparent.title").into_owned(),
                submit_label: t!("authoring.move_submit").into(),
                footer_id: "reparent-footer",
                cancel_id: "reparent-cancel",
                submit_id: "reparent-submit",
                initial_focus: focus,
            },
            window,
            cx,
        );
        view.update(cx, |this, cx| this.refresh_plan(cx));
        Ok(view)
    }
}
impl FormDialogView for ReparentReview {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn closing(&mut self) {
        self.cancel_work();
    }
    fn error_message(error: &anyhow::Error) -> String {
        move_error(error)
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(self.current(cx), t!("authoring.move_stale"));
        let plan = self.plan.as_ref().ok_or_else(|| {
            anyhow::anyhow!(self.reason.clone().unwrap_or_else(|| {
                t!("reparent.working", count = self.control.checkpoints()).into_owned()
            }))
        })?;
        anyhow::ensure!(
            !plan.scene().is_noop() && !self.working,
            t!("authoring.move_noop")
        );
        let owner = self.owner.upgrade().unwrap();
        owner.update(cx, |state, cx| {
            let result = state.editor.execute_palette_reparent(self.document, plan)?;
            if let Some(object) = self.sources.last().cloned() {
                state.present_authoring_result(
                    self.document,
                    &result,
                    crate::authoring::AuthoringPresentation::Moved {
                        object,
                        projection: PaletteProjection::Deformation,
                    },
                    window,
                    cx,
                )?;
            }
            state.clear_edit_error(cx);
            state.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })?;
        Ok(())
    }
}
impl Render for ReparentReview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current(cx);
        if !current && !self.dialog.closed() {
            self.cancel_work();
            self.reason = Some(t!("authoring.move_stale").into_owned());
        }
        let mut body = div()
            .id("reparent-review")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .text_sm()
            .child(t!("authoring.move_selection", count = self.sources.len()).into_owned());
        if let Some(request) = &self.fixed {
            let name = request
                .target
                .as_ref()
                .map(|o| {
                    self.names
                        .get(&o.id)
                        .cloned()
                        .unwrap_or_else(|| o.id.clone())
                })
                .unwrap_or_else(|| t!("authoring.move_root").into_owned());
            let position = match request.position {
                DropPosition::Before => t!("reparent.before"),
                DropPosition::After => t!("reparent.after"),
                DropPosition::Inside => t!("reparent.inside"),
            };
            body = body.child(format!("{position}: {name}"));
        } else {
            body = body.child(
                compact::select(Select::new(&self.destination))
                    .id("reparent-destination")
                    .accessibility_label(t!("authoring.move_destination"))
                    .placeholder(t!("authoring.move_choose"))
                    .disabled(!current)
                    .w_full(),
            );
        }
        let mut modes = div().flex().gap_2();
        for (scope, id, label) in [
            (
                Scope::ParameterDomain,
                "reparent-domain",
                t!("reparent.domain"),
            ),
            (Scope::EditState, "reparent-state", t!("reparent.state")),
        ] {
            modes = modes.child(
                compact::button(Button::new(id))
                    .label(label)
                    .when(self.scope == scope, |b| b.primary())
                    .disabled(!current)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.scope != scope {
                            this.scope = scope;
                            this.refresh_plan(cx);
                        }
                    })),
            );
        }
        body = body.child(modes).child(div().text_xs().child(
            if self.scope == Scope::ParameterDomain {
                t!("reparent.domain_hint")
            } else {
                t!("reparent.state_hint")
            },
        ));
        let mut tolerances = div().flex().gap_2();
        for (i, id, label) in [
            (0, "reparent-absolute", t!("reparent.absolute")),
            (1, "reparent-relative", t!("reparent.relative")),
        ] {
            tolerances = tolerances.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .child(label.clone().into_owned())
                    .child(
                        compact::input(Input::new(&self.tolerance[i]))
                            .id(id)
                            .aria_label(label)
                            .disabled(!current)
                            .w_full(),
                    ),
            );
        }
        body = body.child(tolerances);
        if self.working {
            body =
                body.child(t!("reparent.working", count = self.control.checkpoints()).into_owned());
        }
        if let Some(reason) = self.dialog.error.clone().or(self.reason.clone()) {
            body = body.child(
                div()
                    .id("reparent-reason")
                    .test_support()
                    .aria_label(reason.clone())
                    .text_xs()
                    .child(reason),
            );
        }
        if let Some(plan) = &self.plan {
            if let Some(report) = plan.scene().conversion_report() {
                let mut review = div()
                    .id("reparent-report")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .text_xs();
                let affected = report
                    .affected
                    .iter()
                    .map(|id| self.names.get(id).cloned().unwrap_or_else(|| id.clone()))
                    .collect::<Vec<_>>()
                    .join(", ");
                review = review
                    .child(
                        t!(
                            "reparent.affected",
                            count = report.affected.len(),
                            names = affected
                        )
                        .into_owned(),
                    )
                    .child(
                        t!(
                            "reparent.inventory",
                            preserved = report.preserved.len(),
                            rewritten = report.rewritten.len(),
                            generated = report.generated.len(),
                            deleted = report.deleted.len(),
                            remapped = report.remapped.len()
                        )
                        .into_owned(),
                    );
                if let Some(g) = &report.reparent_guarantee {
                    review = review
                        .child(t!("reparent.geometry_scope").into_owned())
                        .child(g.position_scope.clone())
                        .child(
                            t!(
                                "reparent.error",
                                measured = format!("{:.3e}", g.measured_error),
                                bound = format!("{:.3e}", g.error_bound),
                                absolute = g.tolerance,
                                relative = g.relative_tolerance,
                                scale = g.reference_scale
                            )
                            .into_owned(),
                        );
                    if let Some(context) = &g.context {
                        review = review.child(
                            t!("reparent.time", time = context.edit_time_seconds).into_owned(),
                        );
                        let mut values = context.parameter_values.iter().collect::<Vec<_>>();
                        values.sort_by_key(|(id, _)| *id);
                        review = review.child(
                            t!(
                                "reparent.frozen",
                                values = values
                                    .iter()
                                    .map(|(id, v)| format!("{id}={v}"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                            .into_owned(),
                        );
                        if !context.parameter_domain.is_empty() {
                            review = review.child(
                                t!(
                                    "reparent.ranges",
                                    values = context
                                        .parameter_domain
                                        .iter()
                                        .map(|d| format!(
                                            "{}=[{},{}]",
                                            d.parameter_id, d.minimum, d.maximum
                                        ))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                                .into_owned(),
                            );
                        }
                    }
                }
                if let Some(cost) = plan.scene().cost() {
                    review = review.child(
                        t!(
                            "reparent.cost",
                            samples = cost.sample_combinations,
                            payload = cost.payload_bytes,
                            temporary = cost.temporary_bytes,
                            history = cost.history_bytes
                        )
                        .into_owned(),
                    );
                }
                review = review.child(t!("reparent.effects").into_owned());
                for loss in &report.losses {
                    review = review.child(effect_message(loss));
                }
                body = body.child(
                    div()
                        .id("reparent-report-scroll")
                        .max_h(px(300.))
                        .overflow_y_scroll()
                        .child(review),
                );
            }
        }
        body
    }
}
fn effect_message(message: &str) -> String {
    for (prefix, translated) in [
        ("Opacity inheritance:", t!("reparent.effect_opacity")),
        (
            "Multiply color inheritance:",
            t!("reparent.effect_multiply"),
        ),
        ("Screen color inheritance:", t!("reparent.effect_screen")),
        ("Enabled state:", t!("reparent.effect_enabled")),
        (
            "Parameter-domain visibility:",
            t!("reparent.effect_visibility"),
        ),
        ("Glue postprocessing:", t!("reparent.effect_glue")),
        ("Stateful animation:", t!("reparent.effect_animation")),
    ] {
        if message.starts_with(prefix) {
            return translated.into_owned();
        }
    }
    message.to_owned()
}

pub(super) fn open(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    context: AuthoringContext,
    sources: Vec<ObjectRef>,
    fixed: Option<PaletteMove>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Err(error) =
        ReparentReview::open(owner.clone(), document, context, sources, fixed, window, cx)
    {
        if let Some(owner) = owner.upgrade() {
            owner.update(cx, |state, cx| state.set_edit_error(move_error(&error), cx));
        }
    }
}

#[cfg(test)]
mod tests;
