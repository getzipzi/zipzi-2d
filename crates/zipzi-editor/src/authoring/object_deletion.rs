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
        let objects = if self.editor.editing_context() == Some(EditingContext::Modeling)
            && !matches!(command.target, CommandTarget::Document(_))
        {
            self.editor.document().unwrap().context().selections.clone()
        } else {
            self.editor
                .editing_context()
                .and_then(|context| self.editor.inspection(context))
                .and_then(|i| i.object())
                .into_iter()
                .collect()
        };
        self.run_object_deletion(context, &objects, window, cx);
    }
    pub(crate) fn run_object_deletion(
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
    texture: Entity<SelectState<SearchableVec<Choice>>>,
    node: Entity<SelectState<SearchableVec<Choice>>>,
    base: Entity<SelectState<SearchableVec<Choice>>>,
    slice: Entity<InputState>,
    dialog: FormDialogState,
    playback: PlaybackDialogGuard,
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
                    K::GlueBinding => ObjectKind::GlueBinding,
                    K::Parameter => ObjectKind::Parameter,
                    K::ParameterFolder => ObjectKind::ParameterGroup,
                    K::Texture => ObjectKind::Texture,
                    K::Attachment => ObjectKind::Attachment,
                    K::BlendKeyTable => ObjectKind::BlendKeyTable,
                    K::BlendConstraint => ObjectKind::BlendConstraint,
                    K::MotionGroup => ObjectKind::MotionGroup,
                    K::MotionRegistration => ObjectKind::MotionRegistration,
                    K::MotionEvent => ObjectKind::MotionEvent,
                    K::ExpressionEntry => ObjectKind::ExpressionEntry,
                    K::Pose => ObjectKind::Pose,
                    K::PoseGroup => ObjectKind::PoseGroup,
                    K::Physics => ObjectKind::Physics,
                    K::PhysicsGroup => ObjectKind::PhysicsGroup,
                    K::PhysicsInput => ObjectKind::PhysicsInput,
                    K::PhysicsOutput => ObjectKind::PhysicsOutput,
                    K::PhysicsNode => ObjectKind::PhysicsNode,
                    K::ModelParameterGroup => ObjectKind::ModelParameterGroup,
                    K::ModelParameterMember => ObjectKind::ModelParameterMember,
                    K::CdiCombinedSet => ObjectKind::CdiCombinedSet,
                    K::CdiCombinedMember => ObjectKind::CdiCombinedMember,
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
                    K::Part => zipzi_sdk::DeletionTarget::Part(object.id),
                    K::Offscreen => zipzi_sdk::DeletionTarget::Offscreen(object.id),
                    _ => zipzi_sdk::DeletionTarget::Record(object),
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
                C::ClearTexture => {
                    self.intent
                        .request
                        .texture_replacements
                        .insert(object.id, None);
                }
                C::ChooseTexture => {
                    let id = self
                        .texture
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_choose")))?;
                    self.intent
                        .request
                        .texture_replacements
                        .insert(object.id, Some(id));
                }
                C::ChoosePhysicsNode => {
                    let id = self
                        .node
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_choose")))?;
                    self.intent
                        .request
                        .physics_node_replacements
                        .insert(object.id, id);
                }
                C::ChooseBlendBase => {
                    let id = self
                        .base
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_choose")))?;
                    self.intent
                        .request
                        .blend_base_replacements
                        .insert(object.id, id);
                }
                C::Cancel => {
                    self.playback.close();
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
        let choices = |kind| -> Vec<Choice> {
            entity
                .read(cx)
                .editor
                .document()
                .unwrap()
                .object_index()
                .objects
                .iter()
                .filter(|i| i.reference.kind == kind)
                .filter(|i| {
                    !intent.request.roots.iter().any(|r| {
                        let root = r.object();
                        root.id == i.reference.id && root.kind != K::DrawingGroup
                    })
                })
                .map(|i| Choice {
                    value: i.reference.id.clone(),
                    label: i.name.clone().into(),
                })
                .collect()
        };
        let parts = choices(ObjectKind::Part);
        let textures = choices(ObjectKind::Texture);
        let nodes = choices(ObjectKind::PhysicsNode);
        let samples = choices(ObjectKind::BindingSample);
        let playback = PlaybackDialogGuard::new(context);
        entity.update(cx, |v, _| v.playback_dialog = Some(playback.clone()));
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
                SelectState::new(SearchableVec::new(parts), None, window, cx).searchable(true)
            }),
            mapping: cx.new(|cx| InputState::new(window, cx)),
            texture: cx.new(|cx| {
                SelectState::new(SearchableVec::new(textures), None, window, cx).searchable(true)
            }),
            node: cx.new(|cx| {
                SelectState::new(SearchableVec::new(nodes), None, window, cx).searchable(true)
            }),
            base: cx.new(|cx| {
                SelectState::new(SearchableVec::new(samples), None, window, cx).searchable(true)
            }),
            slice: cx.new(|cx| InputState::new(window, cx)),
            dialog: FormDialogState::new(cx),
            playback,
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
        self.playback.close();
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
        self.playback.close();
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
        C::ChooseTexture => "deletion.choose_texture",
        C::ClearTexture => "deletion.clear_texture",
        C::ChoosePhysicsNode => "deletion.choose_node",
        C::ChooseBlendBase => "deletion.choose_base",
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
            body = body.child(
                div()
                    .id("deletion-planning")
                    .test_support()
                    .aria_label(t!("binding.planning").into_owned())
                    .child(t!("binding.planning")),
            );
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
                        | zipzi_sdk::DeletionTarget::ParameterFolder(_)
                        | zipzi_sdk::DeletionTarget::ParameterFolderContents(_)
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
                        | zipzi_sdk::DeletionTarget::ParameterFolder(_)
                        | zipzi_sdk::DeletionTarget::ParameterFolderContents(_)
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
                T::ParameterFolder(id) => {
                    Some((T::ParameterFolderContents(id), "deletion.delete_contents"))
                }
                T::ParameterFolderContents(id) => {
                    Some((T::ParameterFolder(id), "deletion.dissolve_folder"))
                }
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
        let parameters: Vec<_> = self
            .intent
            .request
            .roots
            .iter()
            .filter_map(|root| match root {
                zipzi_sdk::DeletionTarget::Parameter(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        if !parameters.is_empty() {
            body = body
                .child(t!("deletion.slice"))
                .child(compact::input(Input::new(&self.slice)).id("deletion-slice"));
            for (i, id) in parameters.into_iter().enumerate() {
                body = body.child(
                    compact::button(Button::new(("deletion-slice-apply", i)))
                        .label(format!("{} · {}", id, t!("deletion.apply_slice")))
                        .disabled(!current || self.working)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            let value = v.slice.read(cx).value();
                            if value.trim().is_empty() {
                                v.intent.request.parameter_slices.remove(&id);
                            } else {
                                match value.trim().parse::<f32>() {
                                    Ok(value) if value.is_finite() => {
                                        v.intent.request.parameter_slices.insert(id.clone(), value);
                                    }
                                    _ => {
                                        v.dialog.error =
                                            Some(t!("binding.invalid_number").into_owned());
                                        cx.notify();
                                        return;
                                    }
                                }
                            }
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
                .test_support()
                .h(px(240.))
                .flex_shrink_0()
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
                let mut item = div()
                    .id(("deletion-row", i))
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .aria_label(text.clone())
                    .child(text);
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
                                let Some(plan) = v.plan.clone() else { return };
                                v.invalidate();
                                let result = v.owner.update(
                                    cx,
                                    |view, cx| -> anyhow::Result<AuthoringContext> {
                                        view.editor.unlock_deletion_source(
                                            v.context.editing.document,
                                            &plan,
                                            &unlock,
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
                                        let _ = view.editor.inspect_authoring_object(
                                            v.context.editing.document,
                                            object,
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
                let choices: Vec<_> = page
                    .rows
                    .iter()
                    .filter_map(|r| match r {
                        R::Decision { decision } => Some(&decision.choices),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                if choices
                    .iter()
                    .any(|c| matches!(c, C::ChoosePart | C::SupplyOffscreenMapping))
                {
                    body = body
                        .child(compact::select(Select::new(&self.destination)))
                        .child(t!("deletion.mapping_hint"))
                        .child(compact::input(Input::new(&self.mapping)));
                }
                if choices.contains(&&C::ChooseTexture) {
                    body = body
                        .child(t!("deletion.choose_texture"))
                        .child(compact::select(Select::new(&self.texture)).id("deletion-texture"));
                }
                if choices.contains(&&C::ChoosePhysicsNode) {
                    body = body
                        .child(t!("deletion.choose_node"))
                        .child(compact::select(Select::new(&self.node)).id("deletion-node"));
                }
                if choices.contains(&&C::ChooseBlendBase) {
                    body = body
                        .child(t!("deletion.choose_base"))
                        .child(compact::select(Select::new(&self.base)).id("deletion-base"));
                }
            }
        }
        body = body.child(
            compact::button(Button::new("deletion-review"))
                .label(t!("binding.review"))
                .disabled(!self.source_current(cx) || self.working)
                .on_click(cx.listener(|v, _, window, cx| v.review(window, cx))),
        );
        if let Some(error) = &self.dialog.error {
            body = body.child(
                div()
                    .id("deletion-error")
                    .test_support()
                    .aria_label(error.clone())
                    .child(error.clone()),
            );
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
        ParameterAxis | BlendTableParameter | BlendConstraintParameter =>
            "deletion.parameter_dimension",
        BlendTable | BlendConstraint { .. } => "deletion.binding_loss",
        MotionParameter | ExpressionParameter | ExpressionSourceParameter | MotionClip =>
            "deletion.animation",
        MotionRegistrationContainer => "deletion.move_to_root",
        PhysicsParameter | PhysicsNode => "deletion.physics",
        Texture | AttachmentSound | AttachmentUserData => "deletion.resource",
        ParameterFolder | ModelParameter | CdiParameter { .. } | CdiCombinedParameter =>
            "deletion.display",
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
        TextureReplacementRequired => t!("deletion.texture_required").into_owned(),
        BlendBaseReplacementRequired => t!("deletion.base_required").into_owned(),
        PhysicsNodeReplacementRequired => t!("deletion.node_required").into_owned(),
        InvalidContainer { code } => format!("{}: {code}", t!("deletion.invalid_container")),
    }
}
fn effect_label(e: &zipzi_sdk::DeletionEffect, name: impl Fn(&DeletionObject) -> String) -> String {
    use zipzi_sdk::DeletionEffect::*;
    let (key, object) = match e {
        MotionRegistrationMoved {
            registration,
            group,
        } => {
            return format!(
                "{} · {}: {}",
                name(registration),
                t!("deletion.move_to_root"),
                name(group)
            );
        }
        ContentChanged(o) => ("deletion.content_changed", o),
        ParameterAnimationRemoved(o) => ("deletion.animation_effect", o),
        ParameterDimensionRemoved {
            object,
            parameter,
            value,
            effective_value,
            disabled,
        } => {
            return format!(
                "{} · {}: {} = {value} → {effective_value} · {}: {disabled}",
                name(object),
                t!("deletion.parameter_dimension"),
                name(parameter),
                t!("deletion.disabled")
            );
        }
        BindingContributionRemoved { object, binding } => {
            return format!(
                "{} · {}: {}",
                name(object),
                t!("deletion.binding_loss"),
                name(binding)
            );
        }
        PhysicsChannelRemoved { object, group } => {
            return format!(
                "{} · {}: {}",
                name(group),
                t!("deletion.physics"),
                name(object)
            );
        }
        ResourceReplaced {
            object,
            source,
            replacement,
        } => {
            return format!(
                "{} · {}: {} → {}",
                name(object),
                t!("deletion.resource"),
                name(source),
                replacement
                    .as_ref()
                    .map(&name)
                    .unwrap_or_else(|| t!("deletion.unconfigured").into_owned())
            );
        }
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
