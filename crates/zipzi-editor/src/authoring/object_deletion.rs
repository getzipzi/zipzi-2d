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

#[path = "object_deletion/labels.rs"]
mod labels;
#[path = "object_deletion/view.rs"]
mod view;

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
    details: bool,
    options: bool,
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
    _choice_observers: Vec<Subscription>,
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
            // Runtime publication shares the application thread with rendering.
            // Finish even if the dialog or its source view has gone away.
            let result = work.await.finish();
            let _ = this.update_in(cx, |this, _, cx| {
                if this.serial != serial || !this.current(cx) {
                    return;
                }
                this.working = false;
                this.cancellation = None;
                match result {
                    Ok(plan) => {
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
    fn select_section(&mut self, section: S) {
        self.section = section;
        self.offset = 0;
        self.load_page();
    }
    fn name_id(&self, id: &str, cx: &App) -> String {
        self.owner
            .upgrade()
            .and_then(|owner| {
                owner
                    .read(cx)
                    .editor
                    .document()?
                    .object_index()
                    .objects
                    .iter()
                    .find(|info| info.reference.id == id)
                    .map(|info| info.name.clone())
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                if self.details {
                    id.to_owned()
                } else {
                    t!("deletion.related_data").into_owned()
                }
            })
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
                let reference = object_reference(object)?;
                let kind = reference.kind;
                let info = index.get(&reference)?;
                let part = index
                    .outgoing(&reference)
                    .find(|r| r.kind == zipzi_editor_core::RelationKind::PartOwner)
                    .map(|r| format!(" · Part: {}", index.name(&r.target)))
                    .unwrap_or_default();
                let name =
                    if info.name.is_empty() || (!self.details && info.name.contains(&object.id)) {
                        (crate::properties::provider(kind).type_name)(d, &reference).to_string()
                    } else {
                        info.name.clone()
                    };
                Some(if self.details {
                    format!("{name}{part}")
                } else {
                    name
                })
            })
            .unwrap_or_else(|| {
                if self.details {
                    object.id.clone()
                } else {
                    t!("deletion.related_data").into_owned()
                }
            })
    }
    fn object_type(&self, object: &DeletionObject, cx: &App) -> String {
        self.owner
            .upgrade()
            .and_then(|owner| {
                let document = owner.read(cx).editor.document()?;
                let reference = object_reference(object)?;
                Some(
                    (crate::properties::provider(reference.kind).type_name)(document, &reference)
                        .to_string(),
                )
            })
            .unwrap_or_else(|| t!("deletion.related_data").into_owned())
    }
    fn choice_ready(&self, choice: &C, cx: &App) -> bool {
        match choice {
            C::ChoosePart | C::SupplyOffscreenMapping => {
                self.destination.read(cx).selected_value().is_some()
            }
            C::ChooseTexture => self.texture.read(cx).selected_value().is_some(),
            C::ChoosePhysicsNode => self.node.read(cx).selected_value().is_some(),
            C::ChooseBlendBase => self.base.read(cx).selected_value().is_some(),
            _ => true,
        }
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
        let view = cx.new(|cx| {
            let destination = cx.new(|cx| {
                SelectState::new(SearchableVec::new(parts), None, window, cx).searchable(true)
            });
            let texture = cx.new(|cx| {
                SelectState::new(SearchableVec::new(textures), None, window, cx).searchable(true)
            });
            let node = cx.new(|cx| {
                SelectState::new(SearchableVec::new(nodes), None, window, cx).searchable(true)
            });
            let base = cx.new(|cx| {
                SelectState::new(SearchableVec::new(samples), None, window, cx).searchable(true)
            });
            let observers = [&destination, &texture, &node, &base]
                .into_iter()
                .map(|input| cx.observe(input, |_: &mut Self, _, cx| cx.notify()))
                .collect();
            Self {
                owner,
                context,
                intent,
                plan: None,
                cancellation: None,
                serial: 0,
                preview_revision: None,
                working: false,
                details: false,
                options: false,
                section: S::Removed,
                scope_offset: 0,
                offset: 0,
                page: None,
                destination,
                mapping: cx.new(|cx| InputState::new(window, cx)),
                texture,
                node,
                base,
                slice: cx.new(|cx| InputState::new(window, cx)),
                dialog: FormDialogState::new(cx),
                playback,
                _choice_observers: observers,
                _observe: cx.observe(&entity, |this: &mut Self, _, cx| {
                    if !this.current(cx) {
                        this.invalidate();
                    }
                    this.dialog
                        .set_validation(this.current(cx) && this.executable(), true);
                    cx.notify();
                }),
            }
        });
        let initial_focus = view.read(cx).dialog.cancel_focus.clone();
        form_dialog::open(
            view.clone(),
            FormDialogOptions {
                title: t!("deletion.title").into_owned(),
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
    fn width(_: &Window, cx: &App) -> Pixels {
        compact::scaled(480., cx)
    }
    fn submit_label(&self) -> Option<SharedString> {
        Some(if self.plan.is_some() {
            t!(
                if self.count(S::Removed) == 1 {
                    "deletion.confirm_single"
                } else {
                    "deletion.confirm_count"
                },
                count = self.count(S::Removed)
            )
            .into_owned()
            .into()
        } else {
            t!("deletion.submit").into_owned().into()
        })
    }
    fn destructive() -> bool {
        true
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

fn object_reference(object: &DeletionObject) -> Option<ObjectRef> {
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
    Some(ObjectRef::new(kind, &object.id))
}
