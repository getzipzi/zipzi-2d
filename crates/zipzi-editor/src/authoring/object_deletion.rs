//! Source-bound asynchronous deletion review shared by menus and shortcuts.
use super::*;
use crate::properties::Choice;
use crate::ui::{
    compact,
    form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView},
};
use gpui_kit::component::{
    Disableable,
    button::Button,
    input::{Input, InputState},
    menu::{PopupMenu, PopupMenuItem},
    select::{SearchableVec, Select, SelectState},
};
use zipzi_editor_core::{
    Command, CommandTarget, EditingContext, TreeDeletionCancellation, TreeDeletionPlan,
};
use zipzi_sdk::{
    DeletionChoice as C, DeletionIntent, DeletionKind as K, DeletionObject,
    DeletionReviewState as State, DeletionRow as R, DeletionSection as S,
};

pub(crate) fn append_menu(
    menu: PopupMenu,
    owner: WeakEntity<EditorView>,
    command: Command,
    objects: &[ObjectRef],
    cx: &App,
) -> PopupMenu {
    let Some(entity) = owner.upgrade() else {
        return menu;
    };
    let state = entity.read(cx);
    if state.editor.editing_context() != Some(EditingContext::Modeling) {
        return menu;
    }
    let Some(context) = AuthoringContext::capture(&state.editor) else {
        return menu;
    };
    let objects = objects.to_vec();
    // Menu construction freezes selection only. No candidate, conversion or report.
    menu.separator().item(PopupMenuItem::new(t!(command.id.spec().label).into_owned())
        .action(Box::new(crate::actions::RunCommand::targeted(command)))
        .on_click(move |_, window, cx| { let _ = owner.update(cx, |view, cx| {
            if view.authoring_menu_feedback(context, None, window, cx) { return }
            if matches!(command.target, CommandTarget::Viewport(binding) if !view.editor.viewport_binding_valid(binding)) { return }
            view.run_object_deletion(context, &objects, window, cx);
        }); }))
}
impl EditorView {
    pub(crate) fn handle_object_deletion_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = command.document() else {
            return;
        };
        if !self.prepare_property_inputs(document, window, cx) {
            return;
        }
        let Some(context) = AuthoringContext::capture(&self.editor) else {
            return;
        };
        let objects = self.editor.document().unwrap().context().selections.clone();
        self.run_object_deletion(context, &objects, window, cx);
    }
    fn run_object_deletion(
        &mut self,
        context: AuthoringContext,
        objects: &[ObjectRef],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            anyhow::ensure!(
                !self.editor.manual_active()
                    && !self.editor.gesture_active()
                    && !self.editor.is_busy(),
                t!("authoring.candidate_active")
            );
            anyhow::ensure!(
                !self.modal
                    && !window.has_active_dialog(cx)
                    && AuthoringContext::capture(&self.editor) == Some(context),
                t!("authoring.move_stale")
            );
            anyhow::ensure!(!objects.is_empty(), t!("authoring.delete_select_objects"));
            anyhow::ensure!(
                self.prepare_property_inputs(context.editing.document, window, cx),
                t!("authoring.candidate_active")
            );
            let context = AuthoringContext::capture(&self.editor).unwrap();
            let intent = self.editor.document().unwrap().deletion_intent(objects)?;
            let owner = cx.weak_entity();
            window.defer(cx, move |window, cx| {
                DeletionDialog::open(owner, context, intent, window, cx)
            });
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(error) = result {
            self.set_status_message(authoring_error(&error), cx);
        }
    }
    fn apply_tree_deletion(
        &mut self,
        context: AuthoringContext,
        plan: &TreeDeletionPlan,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.modal
                && !self.editor.manual_active()
                && AuthoringContext::capture(&self.editor) == Some(context),
            t!("authoring.move_stale")
        );
        self.editor
            .execute_tree_deletion(context.editing.document, plan)?;
        self.set_status_message(t!("deletion.completed").into_owned(), cx);
        self.redraw(cx);
        Ok(())
    }
}
struct DeletionDialog {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    intent: DeletionIntent,
    plan: Option<TreeDeletionPlan>,
    cancellation: Option<TreeDeletionCancellation>,
    serial: u64,
    preview_revision: Option<u64>,
    working: bool,
    section: S,
    scope_offset: usize,
    offset: usize,
    page: Option<zipzi_sdk::DeletionPage>,
    destination: Entity<SelectState<SearchableVec<Choice>>>,
    mapping: Entity<InputState>,
    dialog: FormDialogState,
    _observe: Subscription,
}
impl DeletionDialog {
    fn source_current(&self, cx: &App) -> bool {
        !self.dialog.closed()
            && self.owner.upgrade().is_some_and(|owner| {
                let view = owner.read(cx);
                !view.modal
                    && !view.editor.manual_active()
                    && AuthoringContext::capture(&view.editor) == Some(self.context)
            })
    }
    fn current(&self, cx: &App) -> bool {
        self.source_current(cx)
            && self.owner.upgrade().is_some_and(|owner| {
                owner.read(cx).editor.document().is_some_and(|d| {
                    self.preview_revision
                        .is_none_or(|r| r == d.preview_revision())
                        && self.plan.as_ref().is_none_or(|p| p.valid_for(d))
                })
            })
    }
    fn executable(&self) -> bool {
        self.plan
            .as_ref()
            .is_some_and(|p| matches!(p.summary().state, State::Ready | State::NeedsConfirmation))
    }
    fn invalidate(&mut self) {
        self.serial += 1;
        if let Some(cancel) = self.cancellation.take() {
            cancel.cancel();
        }
        self.working = false;
        self.plan = None;
        self.page = None;
        self.dialog.set_validation(false, true);
    }
    fn review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.invalidate();
        self.preview_revision = None;
        self.dialog.edited();
        let task = (|| {
            anyhow::ensure!(self.current(cx), t!("authoring.move_stale"));
            let entity = self.owner.upgrade().unwrap();
            let document = entity.read(cx).editor.document().unwrap();
            self.preview_revision = self
                .intent
                .request
                .reparent
                .values()
                .any(|s| *s == zipzi_sdk::ReparentStrategy::PreserveEditState)
                .then(|| document.preview_revision());
            document.start_tree_deletion(self.intent.clone())
        })();
        let task = match task {
            Ok(task) => task,
            Err(error) => {
                self.dialog.error = Some(authoring_error(&error));
                cx.notify();
                return;
            }
        };
        self.cancellation = Some(task.cancellation());
        self.working = true;
        let serial = self.serial;
        let work = cx.background_executor().spawn(async move { task.run() });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.serial != serial || !this.current(cx) {
                    return;
                }
                this.working = false;
                this.cancellation = None;
                match result {
                    Ok(plan) => {
                        let direct = plan.summary().state == State::Ready;
                        this.plan = Some(plan);
                        this.offset = 0;
                        this.section = if this.executable() {
                            S::Removed
                        } else if this.count(S::Decisions) > 0 {
                            S::Decisions
                        } else if this.count(S::Locks) > 0 {
                            S::Locks
                        } else if this.count(S::Structure) > 0 {
                            S::Structure
                        } else {
                            S::Exports
                        };
                        this.load_page();
                        this.dialog.set_validation(this.executable(), true);
                        if direct {
                            match this.submit(window, cx) {
                                Ok(()) => {
                                    this.dialog.close();
                                    window.close_dialog(cx);
                                }
                                Err(error) => this.dialog.error = Some(authoring_error(&error)),
                            }
                        }
                    }
                    Err(error) => this.dialog.error = Some(authoring_error(&error)),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn count(&self, section: S) -> usize {
        self.plan
            .as_ref()
            .and_then(|p| p.summary().sections.iter().find(|s| s.section == section))
            .map_or(0, |s| s.count)
    }
    fn load_page(&mut self) {
        self.page = None;
        if let Some(plan) = &self.plan {
            match plan.page(self.section, self.offset, 16) {
                Ok(page) => self.page = Some(page),
                Err(error) => self.dialog.error = Some(authoring_error(&error)),
            }
        }
    }
    fn name(&self, object: &DeletionObject, cx: &App) -> String {
        self.owner
            .upgrade()
            .and_then(|owner| {
                let view = owner.read(cx);
                let d = view.editor.document()?;
                let index = d.object_index();
                let kind = match object.kind {
                    K::Mesh => ObjectKind::Mesh,
                    K::Part => ObjectKind::Part,
                    K::Deformer => ObjectKind::Deformer,
                    K::Glue => ObjectKind::Glue,
                    K::Offscreen => ObjectKind::Offscreen,
                    K::MeshBinding => ObjectKind::MeshBinding,
                    K::SceneBinding => ObjectKind::SceneBinding,
                    K::BlendBinding => ObjectKind::BlendBinding,
                    K::Axis => ObjectKind::BindingAxis,
                    K::Sample => ObjectKind::BindingSample,
                    K::GluePair => ObjectKind::GlueVertexPair,
                    K::HitArea => ObjectKind::ModelHitArea,
                    K::DrawingGroup => ObjectKind::DrawOrderGroup,
                    K::Motion => ObjectKind::Motion,
                    K::MotionTrack => ObjectKind::MotionTrack,
                    K::MotionKey => ObjectKind::MotionKey,
                    K::Expression => ObjectKind::Expression,
                    K::PoseEntry => ObjectKind::PoseEntry,
                    K::PoseLink => ObjectKind::PoseLink,
                    _ => return None,
                };
                let reference = ObjectRef::new(kind, &object.id);
                let info = index.get(&reference)?;
                let part = index
                    .outgoing(&reference)
                    .find(|r| r.kind == zipzi_editor_core::RelationKind::PartOwner)
                    .map(|r| format!(" · Part: {}", index.name(&r.target)))
                    .unwrap_or_default();
                Some(format!("{} · {}{}", info.name, object.id, part))
            })
            .unwrap_or_else(|| object.id.clone())
    }
    fn resolve(
        &mut self,
        object: DeletionObject,
        choice: C,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            anyhow::ensure!(self.current(cx), t!("authoring.move_stale"));
            match choice {
                C::PreserveEditState | C::KeepLocalData => {
                    self.intent.request.reparent.insert(
                        object.id,
                        if choice == C::PreserveEditState {
                            zipzi_sdk::ReparentStrategy::PreserveEditState
                        } else {
                            zipzi_sdk::ReparentStrategy::KeepLocalData
                        },
                    );
                }
                C::DeleteSubtree => self.intent.request.roots.push(match object.kind {
                    K::Deformer => zipzi_sdk::DeletionTarget::Deformer(object.id),
                    K::Mesh => zipzi_sdk::DeletionTarget::Mesh(object.id),
                    _ => anyhow::bail!("Unsupported subtree"),
                }),
                C::ChoosePart | C::SupplyOffscreenMapping => {
                    let part_id = self
                        .destination
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_choose")))?;
                    let value = self.mapping.read(cx).value();
                    let mapping = if value.trim().is_empty() {
                        None
                    } else {
                        Some(
                            value
                                .split(',')
                                .map(|v| v.trim().parse::<i32>())
                                .collect::<Result<Vec<_>, _>>()?,
                        )
                    };
                    self.intent.request.offscreen_placements.insert(
                        object.id,
                        zipzi_sdk::DeletionOffscreenPlacement { part_id, mapping },
                    );
                }
                C::Cancel => {
                    self.invalidate();
                    self.dialog.close();
                    window.close_dialog(cx);
                    return Ok(());
                }
            }
            self.review(window, cx);
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(error) = result {
            self.dialog.error = Some(authoring_error(&error));
            cx.notify();
        }
    }
    fn open(
        owner: WeakEntity<EditorView>,
        context: AuthoringContext,
        intent: DeletionIntent,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(entity) = owner.upgrade() else {
            return;
        };
        if window.has_active_dialog(cx)
            || AuthoringContext::capture(&entity.read(cx).editor) != Some(context)
        {
            return;
        }
        let choices: Vec<Choice> = entity
            .read(cx)
            .editor
            .document()
            .unwrap()
            .object_index()
            .objects
            .iter()
            .filter(|i| i.reference.kind == ObjectKind::Part)
            .map(|i| Choice {
                value: i.reference.id.clone(),
                label: i.name.clone().into(),
            })
            .collect();
        let view = cx.new(|cx| Self {
            owner,
            context,
            intent,
            plan: None,
            cancellation: None,
            serial: 0,
            preview_revision: None,
            working: false,
            section: S::Removed,
            scope_offset: 0,
            offset: 0,
            page: None,
            destination: cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices), None, window, cx).searchable(true)
            }),
            mapping: cx.new(|cx| InputState::new(window, cx)),
            dialog: FormDialogState::new(cx),
            _observe: cx.observe(&entity, |this: &mut Self, _, cx| {
                if !this.current(cx) {
                    this.invalidate();
                }
                this.dialog
                    .set_validation(this.current(cx) && this.executable(), true);
                cx.notify();
            }),
        });
        let initial_focus = view.read(cx).dialog.cancel_focus.clone();
        form_dialog::open(
            view.clone(),
            FormDialogOptions {
                title: t!("authoring.delete_object").into_owned(),
                submit_label: t!("deletion.submit").into(),
                footer_id: "tree-deletion-footer",
                cancel_id: "tree-deletion-cancel",
                submit_id: "tree-deletion-submit",
                initial_focus,
            },
            window,
            cx,
        );
        view.update(cx, |v, cx| v.review(window, cx));
    }
}
impl Drop for DeletionDialog {
    fn drop(&mut self) {
        self.invalidate();
    }
}
impl FormDialogView for DeletionDialog {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        authoring_error(error)
    }
    fn closing(&mut self) {
        self.invalidate();
    }
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.current(cx) && !self.working,
            t!("authoring.move_stale")
        );
        anyhow::ensure!(self.executable(), t!("deletion.resolve_required"));
        self.owner.update(cx, |v, cx| {
            v.apply_tree_deletion(self.context, self.plan.as_ref().unwrap(), cx)
        })??;
        Ok(())
    }
}
fn section_label(section: S) -> String {
    t!(match section {
        S::Removed => "deletion.removed",
        S::Owned => "deletion.owned",
        S::Rewritten => "deletion.rewritten",
        S::Effects => "deletion.effects",
        S::Decisions => "deletion.decisions",
        S::Locks => "deletion.locks",
        S::Structure => "deletion.structure",
        S::Deformation => "deletion.deformation",
        S::Organization => "deletion.organization",
        S::GuaranteeLimits => "deletion.limits",
        S::ConversionDomain => "deletion.domain",
        S::Exports => "deletion.exports",
        S::Relations => "deletion.relations",
    })
    .into_owned()
}
fn choice_label(choice: &C) -> String {
    t!(match choice {
        C::PreserveEditState => "deletion.edit_state",
        C::KeepLocalData => "deletion.local",
        C::ChoosePart => "deletion.choose_part",
        C::SupplyOffscreenMapping => "deletion.mapping",
        C::DeleteSubtree => "deletion.subtree",
        C::Cancel => "common.cancel",
    })
    .into_owned()
}
impl Render for DeletionDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current(cx);
        let mut body = div()
            .id("tree-deletion-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2();
        if self.working {
            body = body.child(t!("binding.planning"));
        }
        if !current {
            body = body.child(t!("authoring.move_stale"));
        }
        // Scope is an explicit request option, independent of the visible tree.
        let scope_count = self
            .intent
            .request
            .roots
            .iter()
            .filter(|r| {
                matches!(
                    r,
                    zipzi_sdk::DeletionTarget::Part(_)
                        | zipzi_sdk::DeletionTarget::DissolvePart(_)
                        | zipzi_sdk::DeletionTarget::Deformer(_)
                        | zipzi_sdk::DeletionTarget::RemoveDeformer(_)
                )
            })
            .count();
        if scope_count > 4 {
            let previous = self.scope_offset.saturating_sub(4);
            let next = self.scope_offset + 4;
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(t!("deletion.scope_options"))
                    .child(
                        compact::button(Button::new("deletion-scope-previous"))
                            .label("←")
                            .disabled(self.scope_offset == 0)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.scope_offset = previous;
                                cx.notify();
                            })),
                    )
                    .child(
                        compact::button(Button::new("deletion-scope-next"))
                            .label("→")
                            .disabled(next >= scope_count)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.scope_offset = next;
                                cx.notify();
                            })),
                    ),
            );
        }
        for (i, root) in self
            .intent
            .request
            .roots
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                matches!(
                    r,
                    zipzi_sdk::DeletionTarget::Part(_)
                        | zipzi_sdk::DeletionTarget::DissolvePart(_)
                        | zipzi_sdk::DeletionTarget::Deformer(_)
                        | zipzi_sdk::DeletionTarget::RemoveDeformer(_)
                )
            })
            .skip(self.scope_offset)
            .take(4)
            .map(|(i, r)| (i, r.clone()))
        {
            use zipzi_sdk::DeletionTarget as T;
            let alternative = match root {
                T::Part(id) => Some((T::DissolvePart(id), "deletion.dissolve_part")),
                T::DissolvePart(id) => Some((T::Part(id), "deletion.delete_contents")),
                T::Deformer(id) => Some((T::RemoveDeformer(id), "deletion.remove_parent")),
                T::RemoveDeformer(id) => Some((T::Deformer(id), "deletion.delete_contents")),
                _ => None,
            };
            if let Some((target, label)) = alternative {
                let object = target.object();
                body = body.child(
                    compact::button(Button::new(("deletion-scope", i)))
                        .label(format!("{} · {}", self.name(&object, cx), t!(label)))
                        .disabled(!current || self.working)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.intent.request.roots[i] = target.clone();
                            v.review(window, cx);
                        })),
                );
            }
        }
        if let Some(plan) = &self.plan {
            let sections = plan.summary().sections.clone();
            for (group, members) in [
                ("deletion.removed", &[S::Removed, S::Owned][..]),
                (
                    "deletion.rewritten",
                    &[
                        S::Rewritten,
                        S::Deformation,
                        S::Organization,
                        S::GuaranteeLimits,
                        S::ConversionDomain,
                        S::Relations,
                    ][..],
                ),
                ("deletion.effects", &[S::Effects][..]),
                (
                    "deletion.decisions",
                    &[S::Decisions, S::Locks, S::Structure, S::Exports][..],
                ),
            ] {
                let mut tabs = div().flex().flex_wrap().gap_1();
                for (i, section) in sections
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.count > 0 && members.contains(&s.section))
                {
                    let value = section.section;
                    tabs = tabs.child(
                        compact::button(Button::new(("deletion-section", i)))
                            .label(format!("{} ({})", section_label(value), section.count))
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.section = value;
                                v.offset = 0;
                                v.load_page();
                                cx.notify();
                            })),
                    );
                }
                if sections
                    .iter()
                    .any(|s| s.count > 0 && members.contains(&s.section))
                {
                    body = body.child(div().flex().flex_col().gap_1().child(t!(group)).child(tabs));
                }
            }
        }
        if let Some(page) = &self.page {
            let mut rows = div()
                .id("deletion-rows")
                .max_h(px(340.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
            for (i, row) in page.rows.clone().into_iter().enumerate() {
                let text = match &row {
                    R::Removal { removal } => self.name(&removal.object, cx),
                    R::Relation { relation } => format!(
                        "{} · {} → {}",
                        self.name(&relation.owner, cx),
                        field_label(&relation.field),
                        self.name(&relation.target, cx)
                    ),
                    R::Effect { effect } => effect_label(effect, |o| self.name(o, cx)),
                    R::Decision { decision } => format!(
                        "{} · {}",
                        self.name(&decision.object, cx),
                        decision_label(&decision.reason)
                    ),
                    R::Lock { blocker } => format!(
                        "{} · {}: {}",
                        self.name(&blocker.object, cx),
                        t!("deletion.lock_source"),
                        blocker.source_id
                    ),
                    R::Structural { blocker } => format!(
                        "{} · {}: {}",
                        blocker.object_id, blocker.code, blocker.reason
                    ),
                    R::Deformation {
                        object,
                        parent_id,
                        strategy,
                        guarantee,
                    } => format!(
                        "{} → {} · {} · {}",
                        self.name(object, cx),
                        parent_id,
                        strategy_label(strategy),
                        guarantee
                            .as_ref()
                            .map(|g| format!(
                                "{}: {} · {}: {}, {} · {}",
                                t!("deletion.error"),
                                g.error_bound,
                                t!("deletion.tolerances"),
                                g.tolerance,
                                g.relative_tolerance,
                                t!("deletion.position_only")
                            ))
                            .unwrap_or_else(|| t!("deletion.unpreserved").into_owned())
                    ),
                    R::Organization {
                        object,
                        parent_id,
                        mapping_slots,
                    } => format!(
                        "{} → {} · {}: {}",
                        self.name(object, cx),
                        parent_id,
                        t!("deletion.mapping_slots"),
                        mapping_slots.unwrap_or(0)
                    ),
                    R::GuaranteeLimit { object, ancestor } => format!(
                        "{} · {}: {}",
                        self.name(object, cx),
                        t!("deletion.limited_by"),
                        self.name(ancestor, cx)
                    ),
                    R::Domain {
                        object,
                        parameter_id,
                        minimum,
                        maximum,
                        reference_value,
                    } => format!(
                        "{} · {} [{minimum}, {maximum}] · {reference_value}",
                        self.name(object, cx),
                        parameter_id
                    ),
                    R::Export { failure, existing } => format!(
                        "{} · {}",
                        if *existing {
                            t!("deletion.existing_export")
                        } else {
                            t!("deletion.new_export")
                        },
                        export_label(failure)
                    ),
                };
                let mut item = div().flex().flex_col().gap_1().text_xs().child(text);
                if let R::Decision { decision } = row {
                    let mut buttons = div().flex().flex_wrap().gap_1();
                    for (j, choice) in decision.choices.into_iter().enumerate() {
                        let object = decision.object.clone();
                        buttons = buttons.child(
                            compact::button(Button::new(SharedString::from(format!(
                                "deletion-choice-{i}-{j}"
                            ))))
                            .label(choice_label(&choice))
                            .disabled(!current || self.working)
                            .on_click(cx.listener(
                                move |v, _, window, cx| {
                                    v.resolve(object.clone(), choice.clone(), window, cx)
                                },
                            )),
                        );
                    }
                    item = item.child(buttons);
                } else if let R::Export {
                    failure,
                    existing: false,
                } = row
                {
                    if !self.intent.accepted_export_failures.contains(&failure) {
                        item = item.child(
                            compact::button(Button::new(("deletion-accept-export", i)))
                                .label(t!("deletion.accept_export"))
                                .disabled(!current || self.working)
                                .on_click(cx.listener(move |v, _, window, cx| {
                                    v.intent.accepted_export_failures.push(failure.clone());
                                    v.review(window, cx);
                                })),
                        );
                    }
                } else if let R::Lock { blocker } = row {
                    let unlock = blocker.source_id.clone();
                    item = item.child(
                        compact::button(Button::new(("deletion-unlock", i)))
                            .label(t!("deletion.unlock"))
                            .disabled(!current || self.working)
                            .on_click(cx.listener(move |v, _, window, cx| {
                                v.invalidate();
                                let result = v.owner.update(
                                    cx,
                                    |view, cx| -> anyhow::Result<AuthoringContext> {
                                        let object = view
                                            .editor
                                            .document()
                                            .unwrap()
                                            .object_index()
                                            .objects
                                            .iter()
                                            .find(|o| o.reference.id == unlock)
                                            .map(|o| o.reference.clone())
                                            .ok_or_else(|| {
                                                anyhow::anyhow!(t!("authoring.missing_object"))
                                            })?;
                                        view.editor.execute(
                                            zipzi_editor_core::EditorCommand::SetObjectsLocked {
                                                objects: vec![object],
                                                locked: false,
                                            },
                                        )?;
                                        view.redraw(cx);
                                        AuthoringContext::capture(&view.editor).ok_or_else(|| {
                                            anyhow::anyhow!(t!("authoring.move_stale"))
                                        })
                                    },
                                );
                                match result {
                                    Ok(Ok(context)) => {
                                        v.context = context;
                                        v.preview_revision = None;
                                        v.review(window, cx);
                                    }
                                    Ok(Err(error)) => {
                                        v.dialog.error = Some(authoring_error(&error))
                                    }
                                    Err(error) => v.dialog.error = Some(error.to_string()),
                                }
                            })),
                    );
                    let source = blocker.source_id.clone();
                    item = item.child(
                        compact::button(Button::new(("deletion-locate-lock", i)))
                            .label(t!("deletion.locate"))
                            .on_click(cx.listener(move |v, _, _, cx| {
                                let _ = v.owner.update(cx, |view, cx| {
                                    if let Some(object) = view
                                        .editor
                                        .document()
                                        .unwrap()
                                        .object_index()
                                        .objects
                                        .iter()
                                        .find(|o| o.reference.id == source)
                                        .map(|o| o.reference.clone())
                                    {
                                        let _ = view.editor.execute(
                                            zipzi_editor_core::EditorCommand::SelectObject(object),
                                        );
                                        view.redraw(cx);
                                    }
                                });
                            })),
                    );
                }
                rows = rows.child(item);
            }
            let offset = page.offset;
            let previous = offset.saturating_sub(16);
            let next = page.next_offset;
            body = body.child(rows).child(
                div()
                    .flex()
                    .gap_2()
                    .child(format!(
                        "{}–{} / {}",
                        offset,
                        offset + page.rows.len(),
                        page.total
                    ))
                    .child(
                        compact::button(Button::new("deletion-previous"))
                            .label("←")
                            .disabled(offset == 0)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.offset = previous;
                                v.load_page();
                                cx.notify();
                            })),
                    )
                    .child(
                        compact::button(Button::new("deletion-next"))
                            .label("→")
                            .disabled(next.is_none())
                            .on_click(cx.listener(move |v, _, _, cx| {
                                if let Some(next) = next {
                                    v.offset = next;
                                    v.load_page();
                                    cx.notify();
                                }
                            })),
                    ),
            );
            if self.section == S::Decisions {
                body = body
                    .child(compact::select(Select::new(&self.destination)))
                    .child(t!("deletion.mapping_hint"))
                    .child(compact::input(Input::new(&self.mapping)));
            }
        }
        body = body.child(
            compact::button(Button::new("deletion-review"))
                .label(t!("binding.review"))
                .disabled(!self.source_current(cx) || self.working)
                .on_click(cx.listener(|v, _, window, cx| v.review(window, cx))),
        );
        if let Some(error) = &self.dialog.error {
            body = body.child(error.clone());
        }
        body
    }
}
fn strategy_label(s: &zipzi_sdk::ReparentStrategy) -> String {
    t!(match s {
        zipzi_sdk::ReparentStrategy::KeepLocalData => "deletion.local",
        zipzi_sdk::ReparentStrategy::PreserveEditState => "deletion.edit_state",
        zipzi_sdk::ReparentStrategy::PreserveParameterDomain => "deletion.parameter_domain",
    })
    .into_owned()
}
fn field_label(f: &zipzi_sdk::DeletionField) -> String {
    use zipzi_sdk::DeletionField::*;
    t!(match f {
        Owned => "deletion.owned",
        MeshMask { .. } | OffscreenMask { .. } | MeshInvertedMask => "deletion.mask",
        GlueMeshA | GlueMeshB => "deletion.glue",
        HitAreaMesh => "deletion.hit_area",
        DrawingItem { .. } => "deletion.drawing",
        OrganizationParent => "deletion.organization",
        DeformationParent | ConvertedData => "deletion.deformation",
        PosePart | PoseLinkPart | MotionPart | ExpressionSourcePart => "deletion.animation",
        CdiPart { .. } => "deletion.display",
        RequestedScope => "deletion.scope",
    })
    .into_owned()
}
fn decision_label(r: &zipzi_sdk::DeletionDecisionReason) -> String {
    use zipzi_sdk::DeletionDecisionReason::*;
    match r {
        ConversionUnproven { code } => {
            super::availability::reason_message(code).unwrap_or_else(|| code.clone())
        }
        OffscreenOwnerUnavailable => t!("deletion.owner_required").into_owned(),
        OffscreenMappingRequired => t!("deletion.mapping_required").into_owned(),
        PartStateUnpreserved => t!("deletion.part_state").into_owned(),
    }
}
fn effect_label(e: &zipzi_sdk::DeletionEffect, name: impl Fn(&DeletionObject) -> String) -> String {
    use zipzi_sdk::DeletionEffect::*;
    let (key, object) = match e {
        MaskChanged(o) => ("deletion.mask_effect", o),
        GlueRemoved(o) => ("deletion.glue_effect", o),
        HitAreaRemoved(o) => ("deletion.hit_area", o),
        CompositionChanged(o) => ("deletion.composition_effect", o),
        PartAnimationRemoved(o) => ("deletion.animation_effect", o),
        OrganizationChanged(o) => ("deletion.organization", o),
        DeformationChanged(o) => ("deletion.deformation", o),
        DrawingGroupFlattened {
            group,
            destination,
            source_range,
            destination_range,
            dynamic_order,
            offscreen,
        } => {
            return format!(
                "{} · {} → {} · {:?} → {:?} · {}: {dynamic_order} · {}",
                t!("deletion.drawing_effect"),
                name(group),
                name(destination),
                source_range,
                destination_range,
                t!("deletion.dynamic_order"),
                offscreen.as_ref().map(&name).unwrap_or_default()
            );
        }
        UnpreservedChannels { object, channels } => {
            return format!(
                "{} · {}: {}",
                name(object),
                t!("deletion.unpreserved"),
                channels
                    .iter()
                    .map(|c| t!(match c {
                        zipzi_sdk::DeletionChannel::Appearance => "deletion.appearance",
                        zipzi_sdk::DeletionChannel::Enabled => "deletion.enabled",
                        zipzi_sdk::DeletionChannel::GluePostprocessing => "deletion.glue",
                        zipzi_sdk::DeletionChannel::DrawingOrder => "deletion.drawing",
                    })
                    .into_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        PartStateRemoved {
            part,
            enabled,
            animated_order,
        } => {
            return format!(
                "{} · {} · {}: {enabled} · {}: {animated_order}",
                name(part),
                t!("deletion.part_state"),
                t!("deletion.enabled"),
                t!("deletion.dynamic_order")
            );
        }
    };
    format!("{} · {}", name(object), t!(key))
}
fn export_label(e: &zipzi_sdk::DeletionExportFailure) -> String {
    use zipzi_sdk::DeletionExportFailure::*;
    match e {
        Model3NamespaceChanged => "model3 · OPAQUE_NAMESPACE_CHANGED".into(),
        Cdi { problem } => format!("CDI · {problem:?}"),
        Motion { id, problem } => format!("Motion {id} · {problem:?}"),
        Expression { id, problem } => format!("Expression {id} · {problem:?}"),
        Pose { id, problem } => format!("Pose {id} · {problem:?}"),
        Physics { id, problem } => format!("Physics {id} · {problem:?}"),
    }
}
