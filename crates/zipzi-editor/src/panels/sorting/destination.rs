//! One chosen destination, one core plan. Searching and painting choices never
//! build a plan per container or retain document snapshots for every choice.
use super::*;
use crate::{
    t,
    ui::{
        compact,
        form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView},
    },
};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};

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

struct MoveDestinationPicker {
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    context: AuthoringContext,
    projection: PaletteProjection,
    resource: bool,
    playback: Option<crate::authoring::PlaybackDialogGuard>,
    sources: Vec<ObjectRef>,
    choices: Vec<Choice>,
    destination: Entity<SelectState<SearchableVec<Choice>>>,
    validation: Result<SortPlan, String>,
    dialog: FormDialogState,
    _event: Subscription,
    _owner_observe: Subscription,
    #[cfg(test)]
    plans_built: usize,
}

impl MoveDestinationPicker {
    fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        expected: AuthoringContext,
        projection: PaletteProjection,
        sources: Vec<ObjectRef>,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<Entity<Self>> {
        anyhow::ensure!(!window.has_active_dialog(cx), t!("authoring.move_stale"));
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_stale")))?;
        let resource = sources.first().is_some_and(|o| {
            matches!(
                o.kind,
                ObjectKind::Motion
                    | ObjectKind::MotionGroup
                    | ObjectKind::MotionRegistration
                    | ObjectKind::Expression
                    | ObjectKind::ExpressionEntry
            )
        });
        let context = entity.update(cx, |state, cx| {
            anyhow::ensure!(
                !state.modal
                    && AuthoringContext::capture(&state.editor) == Some(expected)
                    && state.editor.active_id() == Some(document)
                    && state.editor.editing_context()
                        == Some(if resource {
                            zipzi_editor_core::EditingContext::Animation
                        } else {
                            zipzi_editor_core::EditingContext::Modeling
                        }),
                t!("authoring.move_stale")
            );
            anyhow::ensure!(
                state.prepare_property_inputs(document, window, cx),
                t!("authoring.move_stale")
            );
            // Modal editing has its own token, captured after the shared input
            // boundary; the dialog must not accept an earlier menu incarnation.
            if !resource {
                state
                    .editor
                    .end_editing(document, zipzi_editor_core::EditingLifecycle::BlockingUi)?;
            }
            state.redraw(cx);
            Ok::<_, anyhow::Error>(AuthoringContext::capture(&state.editor).unwrap())
        })?;
        let doc = entity.read(cx).editor.document().unwrap();
        let destinations = if resource {
            doc.animation_resource_destinations(&sources[0])?
        } else {
            doc.palette_move_destinations(projection)?
        };
        let mut counts = std::collections::HashMap::new();
        for destination in &destinations {
            *counts.entry(destination.path.as_str()).or_insert(0) += 1;
        }
        let choices: Vec<_> = destinations
            .iter()
            .map(|destination| {
                let path = if destination.object.is_none() {
                    t!("authoring.move_root").into_owned()
                } else {
                    destination.path.clone()
                };
                let label = if counts[destination.path.as_str()] > 1 {
                    format!("{path} · {}", destination.object.as_ref().unwrap().id)
                } else {
                    path
                };
                Choice {
                    key: destination
                        .object
                        .as_ref()
                        .map_or_else(|| "root".into(), |object| object.id.clone()),
                    object: destination.object.clone(),
                    label: label.into(),
                }
            })
            .collect();
        let playback = resource.then(|| crate::authoring::PlaybackDialogGuard::new(context));
        if let Some(guard) = &playback {
            entity.update(cx, |state, _| state.playback_dialog = Some(guard.clone()));
        }
        let view = cx.new(|cx| {
            let destination = cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices.clone()), None, window, cx)
                    .searchable(true)
            });
            let event =
                cx.subscribe_in(&destination, window, |this: &mut Self, _, event, _, cx| {
                    if matches!(event, SelectEvent::Confirm(_)) && !this.dialog.closed() {
                        this.dialog.edited();
                        this.validation = this.plan(cx).map_err(|error| move_error(&error));
                        this.sync_validation();
                        cx.notify();
                    }
                });
            let observe = cx.observe(&entity, |_: &mut Self, _, cx| cx.notify());
            let mut dialog = FormDialogState::new(cx);
            dialog.set_validation(false, true);
            Self {
                owner,
                document,
                context,
                projection,
                resource,
                playback,
                sources,
                choices,
                destination,
                validation: Err(t!("authoring.move_choose").into_owned()),
                dialog,
                _event: event,
                _owner_observe: observe,
                #[cfg(test)]
                plans_built: 0,
            }
        });
        let focus = view.read(cx).destination.read(cx).focus_handle(cx);
        form_dialog::open(
            view.clone(),
            FormDialogOptions {
                title: t!("authoring.move_to").into_owned(),
                submit_label: t!("authoring.move_submit").into(),
                footer_id: "move-destination-footer",
                cancel_id: "move-destination-cancel",
                submit_id: "move-destination-submit",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(view)
    }

    fn request(&self, cx: &App) -> anyhow::Result<PaletteMove> {
        let key = self.destination.read(cx).selected_value();
        let choice = self
            .choices
            .iter()
            .find(|choice| Some(&choice.key) == key)
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_choose")))?;
        Ok(PaletteMove {
            projection: self.projection,
            sources: self.sources.clone(),
            target: choice.object.clone(),
            position: DropPosition::Inside,
        })
    }

    fn plan(&mut self, cx: &App) -> anyhow::Result<SortPlan> {
        let entity = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_stale")))?;
        let state = entity.read(cx);
        anyhow::ensure!(
            !state.modal && AuthoringContext::capture(&state.editor) == Some(self.context),
            t!("authoring.move_stale")
        );
        let request = self.request(cx)?;
        #[cfg(test)]
        {
            self.plans_built += 1;
        }
        let doc = state.editor.document().unwrap();
        if self.resource {
            doc.plan_animation_resource_move(
                &self.sources[0],
                request.target.as_ref(),
                DropPosition::Inside,
            )
            .map(SortPlan::Resource)
        } else {
            doc.plan_palette_move(request).map(SortPlan::Palette)
        }
    }

    fn sync_validation(&mut self) {
        self.dialog.set_validation(
            self.validation.as_ref().is_ok_and(|plan| !plan.is_noop()),
            true,
        );
    }
}

impl Drop for MoveDestinationPicker {
    fn drop(&mut self) {
        if let Some(guard) = &self.playback {
            guard.close();
        }
    }
}

impl FormDialogView for MoveDestinationPicker {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn closing(&mut self) {
        if let Some(guard) = &self.playback {
            guard.close();
        }
    }
    fn error_message(error: &anyhow::Error) -> String {
        move_error(error)
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        // Replan once at execution, even if the footer was previously enabled.
        let plan = self.plan(cx)?;
        anyhow::ensure!(!plan.is_noop(), t!("authoring.move_noop"));
        let entity = self.owner.upgrade().unwrap();
        entity.update(cx, |state, cx| {
            anyhow::ensure!(
                AuthoringContext::capture(&state.editor) == Some(self.context),
                t!("authoring.move_stale")
            );
            match plan {
                SortPlan::Business(plan) => {
                    state.editor.execute_business(self.document, &plan)?;
                }
                SortPlan::Palette(plan) => {
                    let result = state.editor.execute_palette_move(self.document, &plan)?;
                    if let Some(object) = self.sources.last().cloned() {
                        state.present_authoring_result(
                            self.document,
                            &result,
                            crate::authoring::AuthoringPresentation::Moved {
                                object,
                                projection: self.projection,
                            },
                            window,
                            cx,
                        )?;
                    }
                }
                SortPlan::Resource(plan) => {
                    let result = state
                        .editor
                        .execute_animation_library(self.document, &plan)?;
                    if result.receipt.changed {
                        state.defer_authoring_location(
                            self.document,
                            self.sources[0].clone(),
                            None,
                            false,
                            false,
                            window,
                            cx,
                        );
                    }
                }
            }
            state.clear_edit_error(cx);
            state.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })?;
        Ok(())
    }
}

impl Render for MoveDestinationPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let valid_context = self.owner.upgrade().is_some_and(|owner| {
            AuthoringContext::capture(&owner.read(cx).editor) == Some(self.context)
        });
        if !valid_context {
            self.validation = Err(t!("authoring.move_stale").into_owned());
            self.sync_validation();
        }
        let reason = self
            .dialog
            .error
            .clone()
            .or_else(|| match &self.validation {
                Err(reason) => Some(reason.clone()),
                Ok(plan) if plan.is_noop() => Some(t!("authoring.move_noop").into_owned()),
                _ => None,
            });
        div()
            .id("move-destination-picker")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .text_sm()
            .child(t!("authoring.move_selection", count = self.sources.len()).into_owned())
            .child(
                compact::select(Select::new(&self.destination))
                    .id("move-destination-select")
                    .accessibility_label(t!("authoring.move_destination"))
                    .placeholder(t!("authoring.move_choose"))
                    .disabled(!valid_context)
                    .w_full(),
            )
            .when_some(reason, |element, reason| {
                element.child(
                    div()
                        .id("move-destination-reason")
                        .test_support()
                        .aria_label(reason.clone())
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(reason),
                )
            })
            .when(
                self.validation.as_ref().is_ok_and(|plan| !plan.is_noop()),
                |element| {
                    element.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(t!("authoring.move_append")),
                    )
                },
            )
    }
}

pub(super) fn open(
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    context: AuthoringContext,
    projection: PaletteProjection,
    sources: Vec<ObjectRef>,
    window: &mut Window,
    cx: &mut App,
) {
    if projection == PaletteProjection::Deformation {
        super::reparent::open(owner, document, context, sources, None, window, cx);
        return;
    }
    if let Err(error) = MoveDestinationPicker::open(
        owner.clone(),
        document,
        context,
        projection,
        sources,
        window,
        cx,
    ) {
        if let Some(owner) = owner.upgrade() {
            owner.update(cx, |state, cx| state.set_edit_error(move_error(&error), cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthoringContext, MoveDestinationPicker, ObjectKind, ObjectRef, PaletteProjection,
    };
    use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};

    #[gpui_kit::test]
    fn search_and_repaint_do_not_build_destination_plans(cx: &mut TestAppContext) {
        let (document, assets) =
            crate::headless_tests::document_fixture::sample_document_with_parameter_groups()
                .unwrap();
        let object = ObjectRef::new(ObjectKind::Parameter, uuid::Uuid::from_u128(1).to_string());
        let h = crate::headless_tests::setup_with_document(cx, document, assets);
        cx.update(|cx| cx.set_reduce_motion(true));
        let picker = cx
            .update_window(h.window, |_, window, cx| {
                let state = h.view.read(cx);
                MoveDestinationPicker::open(
                    h.view.downgrade(),
                    state.editor.active_id().unwrap(),
                    AuthoringContext::capture(&state.editor).unwrap(),
                    PaletteProjection::Parameters,
                    vec![object],
                    window,
                    cx,
                )
                .unwrap()
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(picker.read_with(cx, |picker, _| picker.plans_built), 0);
        cx.update_window(h.window, |_, window, cx| {
            window.within("move-destination-select").click("input", cx);
            window.input("Body", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(160));
        cx.run_until_parked();
        assert_eq!(picker.read_with(cx, |picker, _| picker.plans_built), 0);
        cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(picker.read_with(cx, |picker, _| picker.plans_built), 1);
        for _ in 0..8 {
            cx.update_window(h.window, |_, window, cx| {
                h.view.update(cx, |_, cx| cx.notify());
                window.render_frame(cx);
            })
            .unwrap();
            cx.run_until_parked();
        }
        assert_eq!(picker.read_with(cx, |picker, _| picker.plans_built), 1);
        cx.update_window(h.window, |_, window, cx| {
            window.click("move-destination-submit", cx)
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(picker.read_with(cx, |picker, _| picker.plans_built), 2);
        assert_eq!(
            h.view.read_with(cx, |view, _| view
                .editor
                .document()
                .unwrap()
                .history_lengths()),
            (1, 0)
        );
    }
}
