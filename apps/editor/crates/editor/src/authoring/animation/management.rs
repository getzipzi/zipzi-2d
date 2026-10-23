//! Frozen resource choices and native deletion plans; neither dialog activates preview.
use super::*;
use crate::ui::{
    compact,
    form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView},
};
use editor_core::{AnimationDeletionPlan, AnimationRemoval, MotionCopyKind, MotionCopyPlan};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
};

impl EditorView {
    pub(crate) fn copy_motion(
        &mut self,
        context: AuthoringContext,
        source: ObjectRef,
        kind: MotionCopyKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let document = context.editing.document;
        let result = self
            .execute_authoring(context, window, cx, |editor, document| {
                let doc = editor.document().unwrap();
                let name = if source.kind == ObjectKind::MotionRegistration {
                    doc.motion_registration(&source.id)
                        .and_then(|(_, e)| doc.motion_definition(&e.clip_id))
                        .ok_or_else(|| anyhow::anyhow!("Missing motion"))?
                        .name
                        .clone()
                } else {
                    doc.object_index().name(&source).to_owned()
                };
                let plan = doc.plan_motion_copy(
                    &source,
                    kind,
                    t!("authoring.copy_name", name = name).into_owned(),
                )?;
                editor.execute_motion_copy(document, &plan)
            })
            .and_then(|result| {
                self.present_created_object(
                    document,
                    &result,
                    if kind == MotionCopyKind::Group {
                        ObjectKind::MotionGroup
                    } else {
                        ObjectKind::Motion
                    },
                    window,
                    cx,
                )
                .map(|_| ())
            });
        match result {
            Ok(()) => self.redraw(cx),
            Err(error) => self.set_edit_error(authoring_error(&error), cx),
        }
    }
    pub(crate) fn review_animation_deletion(
        &mut self,
        context: AuthoringContext,
        source: ObjectRef,
        operation: AnimationRemoval,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            anyhow::ensure!(
                AuthoringContext::capture(&self.editor) == Some(context)
                    && !self.modal
                    && !window.has_active_dialog(cx),
                t!("authoring.animation_stale")
            );
            anyhow::ensure!(
                self.prepare_property_inputs(context.editing.document, window, cx),
                t!("authoring.candidate_active")
            );
            let context = AuthoringContext::capture(&self.editor).unwrap();
            let plan = self
                .editor
                .document()
                .unwrap()
                .plan_animation_deletion(&source, operation)?;
            let candidates: Vec<_> = std::iter::once(source.clone())
                .chain(self.editor.document().unwrap().authoring_neighbors(&source))
                .collect();
            if !plan.requires_review() {
                let result = self
                    .editor
                    .execute_animation_deletion(context.editing.document, &plan)?;
                self.present_authoring_result(
                    context.editing.document,
                    &result,
                    AuthoringPresentation::AfterDelete { candidates },
                    window,
                    cx,
                )?;
                self.redraw(cx);
            } else {
                let owner = cx.weak_entity();
                window.defer(cx, move |window, cx| {
                    if let Err(error) =
                        DeletionReview::open(owner.clone(), context, plan, candidates, window, cx)
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
}

#[derive(Clone)]
struct ClipChoice {
    id: String,
    label: SharedString,
}
impl SelectItem for ClipChoice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &String {
        &self.id
    }
}
struct ReferencePicker {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    at: Option<ObjectRef>,
    choice: Entity<SelectState<SearchableVec<ClipChoice>>>,
    plan: Result<MotionCopyPlan, String>,
    dialog: FormDialogState,
    playback: PlaybackDialogGuard,
    _event: Subscription,
    _observe: Subscription,
}
pub(super) fn open_reference(
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    at: Option<ObjectRef>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Err(error) = ReferencePicker::open(owner.clone(), context, at, window, cx) {
        let _ = owner.update(cx, |v, cx| v.set_edit_error(authoring_error(&error), cx));
    }
}
impl ReferencePicker {
    fn open(
        owner: WeakEntity<EditorView>,
        context: AuthoringContext,
        at: Option<ObjectRef>,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.animation_stale")))?;
        anyhow::ensure!(
            !window.has_active_dialog(cx)
                && !entity.read(cx).modal
                && AuthoringContext::capture(&entity.read(cx).editor) == Some(context),
            t!("authoring.animation_stale")
        );
        let doc = entity.read(cx).editor.document().unwrap();
        let index = doc.object_index();
        let clips: Vec<_> = index
            .objects
            .iter()
            .filter(|o| o.reference.kind == ObjectKind::Motion)
            .collect();
        let mut names = std::collections::HashMap::new();
        for clip in &clips {
            *names.entry(clip.name.as_str()).or_insert(0usize) += 1;
        }
        let choices: Vec<_> = clips
            .iter()
            .map(|o| ClipChoice {
                id: o.reference.id.clone(),
                label: if names[o.name.as_str()] > 1 {
                    format!("{} · {}", o.name, o.reference.id).into()
                } else {
                    o.name.clone().into()
                },
            })
            .collect();
        let playback = PlaybackDialogGuard::new(context);
        entity.update(cx, |v, _| v.playback_dialog = Some(playback.clone()));
        let view = cx.new(|cx| {
            let choice = cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices), None, window, cx).searchable(true)
            });
            let event = cx.subscribe_in(&choice, window, |this: &mut Self, _, event, _, cx| {
                if matches!(event, SelectEvent::Confirm(_)) && !this.dialog.closed() {
                    this.plan = (|| {
                        let entity = this
                            .owner
                            .upgrade()
                            .ok_or_else(|| anyhow::anyhow!(t!("authoring.animation_stale")))?;
                        anyhow::ensure!(
                            AuthoringContext::capture(&entity.read(cx).editor)
                                == Some(this.context),
                            t!("authoring.animation_stale")
                        );
                        let id = this
                            .choice
                            .read(cx)
                            .selected_value()
                            .ok_or_else(|| anyhow::anyhow!(t!("authoring.choose_motion")))?;
                        entity
                            .read(cx)
                            .editor
                            .document()
                            .unwrap()
                            .plan_motion_reference(id, this.at.as_ref())
                    })()
                    .map_err(|e| authoring_error(&e));
                    this.dialog.edited();
                    this.dialog.set_validation(this.plan.is_ok(), true);
                    cx.notify();
                }
            });
            let observe = cx.observe(&entity, |this: &mut Self, owner, cx| {
                let valid = !owner.read(cx).modal
                    && AuthoringContext::capture(&owner.read(cx).editor) == Some(this.context);
                this.dialog.set_validation(valid && this.plan.is_ok(), true);
                cx.notify();
            });
            let mut dialog = FormDialogState::new(cx);
            dialog.set_validation(false, true);
            Self {
                owner,
                context,
                at,
                choice,
                plan: Err(t!("authoring.choose_motion").into_owned()),
                dialog,
                playback,
                _event: event,
                _observe: observe,
            }
        });
        let focus = view.read(cx).choice.read(cx).focus_handle(cx);
        form_dialog::open(
            view,
            FormDialogOptions {
                title: t!("authoring.add_motion_reference").into_owned(),
                submit_label: t!("authoring.add_reference_submit").into(),
                footer_id: "motion-reference-footer",
                cancel_id: "motion-reference-cancel",
                submit_id: "motion-reference-submit",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
}
impl Drop for ReferencePicker {
    fn drop(&mut self) {
        self.playback.close();
    }
}
impl FormDialogView for ReferencePicker {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn closing(&mut self) {
        self.playback.close();
    }
    fn error_message(error: &anyhow::Error) -> String {
        authoring_error(error)
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let plan = self.plan.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.owner.update(cx, |v, cx| {
            anyhow::ensure!(
                !v.modal && AuthoringContext::capture(&v.editor) == Some(self.context),
                t!("authoring.animation_stale")
            );
            v.editor
                .execute_motion_copy(self.context.editing.document, plan)?;
            v.inspect_authoring_object(
                self.context.editing.document,
                plan.selection.clone(),
                false,
                window,
                cx,
            )?;
            v.defer_authoring_location(
                self.context.editing.document,
                plan.selection.clone(),
                None,
                true,
                true,
                window,
                cx,
            );
            v.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })??;
        Ok(())
    }
}
impl Render for ReferencePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let valid = self.owner.upgrade().is_some_and(|v| {
            !v.read(cx).modal && AuthoringContext::capture(&v.read(cx).editor) == Some(self.context)
        });
        self.dialog.set_validation(valid && self.plan.is_ok(), true);
        div()
            .id("motion-reference-picker")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .text_sm()
            .child(t!("authoring.reference_explanation"))
            .child(
                compact::select(Select::new(&self.choice))
                    .id("motion-reference-select")
                    .placeholder(t!("authoring.choose_motion"))
                    .disabled(!valid)
                    .w_full(),
            )
            .when_some(
                self.dialog.error.clone().or_else(|| {
                    if !valid {
                        Some(t!("authoring.animation_stale").into_owned())
                    } else {
                        self.plan.as_ref().err().cloned()
                    }
                }),
                |e, reason| {
                    e.child(
                        div()
                            .id("motion-reference-reason")
                            .test_support()
                            .role(Role::Label)
                            .aria_label(reason.clone())
                            .child(reason),
                    )
                },
            )
    }
}

struct DeletionReview {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    plan: AnimationDeletionPlan,
    candidates: Vec<ObjectRef>,
    rows: std::sync::Arc<[String]>,
    dialog: FormDialogState,
    playback: PlaybackDialogGuard,
    _observe: Subscription,
}
impl DeletionReview {
    fn open(
        owner: WeakEntity<EditorView>,
        context: AuthoringContext,
        plan: AnimationDeletionPlan,
        candidates: Vec<ObjectRef>,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.animation_stale")))?;
        anyhow::ensure!(
            !window.has_active_dialog(cx)
                && !entity.read(cx).modal
                && AuthoringContext::capture(&entity.read(cx).editor) == Some(context),
            t!("authoring.animation_stale")
        );
        let doc = entity.read(cx).editor.document().unwrap();
        let index = doc.object_index();
        let resources = doc.motion_resources();
        let registrations: std::collections::HashMap<_, _> = resources
            .iter()
            .filter_map(|r| r.selection.entry.as_ref().map(|id| (id.as_str(), r)))
            .collect();
        let describe = |o: &ObjectRef| {
            if o.kind == ObjectKind::MotionRegistration {
                if let Some(r) = registrations.get(o.id.as_str()) {
                    return format!(
                        "{} [{}] · {} · {}",
                        r.group
                            .as_deref()
                            .map_or_else(|| t!("workspace.ungrouped").into_owned(), str::to_owned),
                        r.entry_index + 1,
                        r.name,
                        o.id
                    );
                }
            }
            format!("{} · {}", index.name(o), o.id)
        };
        let mut rows = vec![];
        for (label, objects) in [
            ("authoring.removal_deleted", &plan.removed),
            ("authoring.removal_migrated", &plan.migrated_to_root),
            ("authoring.removal_preserved", &plan.preserved),
            (
                "authoring.removal_retained_resources",
                &plan.retained_resources,
            ),
            ("authoring.removal_blockers", &plan.blocked_by),
        ] {
            if objects.is_empty() {
                continue;
            }
            rows.push(format!("{} ({})", t!(label), objects.len()));
            let mut owned = std::collections::BTreeMap::<ObjectKind, usize>::new();
            for object in objects {
                if matches!(
                    object.kind,
                    ObjectKind::MotionTrack
                        | ObjectKind::MotionKey
                        | ObjectKind::MotionEvent
                        | ObjectKind::ExpressionEntry
                ) {
                    *owned.entry(object.kind).or_default() += 1;
                } else {
                    rows.push(describe(object));
                }
            }
            for (kind, count) in owned {
                let key = format!("properties.type_{kind:?}");
                let label = t!(&key).into_owned();
                rows.push(format!("{label}: {count}"));
            }
        }
        let playback = PlaybackDialogGuard::new(context);
        entity.update(cx, |v, _| v.playback_dialog = Some(playback.clone()));
        let view = cx.new(|cx| {
            let observe = cx.observe(&entity, |this: &mut Self, owner, cx| {
                let valid = !owner.read(cx).modal
                    && AuthoringContext::capture(&owner.read(cx).editor) == Some(this.context);
                this.dialog.set_validation(
                    valid && !this.plan.is_noop() && this.plan.blocked_by.is_empty(),
                    true,
                );
                cx.notify();
            });
            let mut dialog = FormDialogState::new(cx);
            dialog.set_validation(!plan.is_noop() && plan.blocked_by.is_empty(), true);
            Self {
                owner,
                context,
                plan,
                candidates,
                rows: rows.into(),
                dialog,
                playback,
                _observe: observe,
            }
        });
        let focus = view.read(cx).dialog.cancel_focus.clone();
        form_dialog::open(
            view,
            FormDialogOptions {
                title: t!("authoring.review_animation_deletion").into_owned(),
                submit_label: t!("authoring.apply_deletion").into(),
                footer_id: "animation-delete-footer",
                cancel_id: "animation-delete-cancel",
                submit_id: "animation-delete-submit",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
}
impl Drop for DeletionReview {
    fn drop(&mut self) {
        self.playback.close();
    }
}
impl FormDialogView for DeletionReview {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn closing(&mut self) {
        self.playback.close();
    }
    fn error_message(error: &anyhow::Error) -> String {
        authoring_error(error)
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.plan.is_noop() && self.plan.blocked_by.is_empty(),
            t!("authoring.removal_noop")
        );
        self.owner.update(cx, |v, cx| {
            anyhow::ensure!(
                !v.modal && AuthoringContext::capture(&v.editor) == Some(self.context),
                t!("authoring.animation_stale")
            );
            let result = v
                .editor
                .execute_animation_deletion(self.context.editing.document, &self.plan)?;
            v.present_authoring_result(
                self.context.editing.document,
                &result,
                AuthoringPresentation::AfterDelete {
                    candidates: self.candidates.clone(),
                },
                window,
                cx,
            )?;
            v.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })??;
        Ok(())
    }
}
impl Render for DeletionReview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let valid = self.owner.upgrade().is_some_and(|v| {
            !v.read(cx).modal && AuthoringContext::capture(&v.read(cx).editor) == Some(self.context)
        });
        self.dialog.set_validation(
            valid && !self.plan.is_noop() && self.plan.blocked_by.is_empty(),
            true,
        );
        let rows = self.rows.clone();
        div()
            .id("animation-deletion-review")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .text_sm()
            .child(t!(removal_explanation(self.plan.operation)))
            .child(
                uniform_list(
                    "animation-delete-effects",
                    rows.len(),
                    move |range, _, cx| {
                        range
                            .map(|i| {
                                div()
                                    .id(("animation-delete-effect", i))
                                    .test_support()
                                    .h(compact::rem(compact::ROW))
                                    .text_xs()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .text_color(cx.theme().foreground)
                                    .role(Role::Label)
                                    .aria_label(rows[i].clone())
                                    .child(rows[i].clone())
                            })
                            .collect()
                    },
                )
                .h(compact::rem(
                    (self.rows.len().clamp(1, 12) as f32) * compact::ROW,
                ))
                .w_full(),
            )
            .when_some(
                self.dialog.error.clone().or_else(|| {
                    if !valid {
                        Some(t!("authoring.animation_stale").into_owned())
                    } else if self.plan.is_noop() {
                        Some(t!("authoring.removal_noop").into_owned())
                    } else if !self.plan.blocked_by.is_empty() {
                        Some(t!("authoring.referenced").into_owned())
                    } else {
                        None
                    }
                }),
                |e, reason| {
                    e.child(
                        div()
                            .id("animation-delete-reason")
                            .test_support()
                            .role(Role::Label)
                            .aria_label(reason.clone())
                            .child(reason),
                    )
                },
            )
    }
}
fn removal_explanation(operation: AnimationRemoval) -> &'static str {
    match operation {
        AnimationRemoval::Motion => "authoring.delete_motion_explanation",
        AnimationRemoval::Expression => "authoring.delete_expression_explanation",
        AnimationRemoval::DissolveGroup => "authoring.dissolve_motion_explanation",
        _ => "authoring.delete_registration_explanation",
    }
}
