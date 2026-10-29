//! One coordinator for business collections, captured row actions and frozen review.
use super::*;
use crate::properties::Choice;
use crate::ui::{
    compact,
    form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView},
};
use gpui_kit::component::{
    WindowExt,
    menu::PopupMenuItem,
    select::{SearchableVec, Select, SelectEvent, SelectState},
};
use zipzi_editor_core::{BusinessAction as A, BusinessPlan, ObjectKind as K};

pub(crate) fn availability(editor: &Editor, action: &A) -> Option<ActionAvailability> {
    match editor.document()?.business_capability(action) {
        Ok(c) => ActionAvailability::from_core(&c),
        Err(e) => Some(ActionAvailability {
            enabled: false,
            reason: Some(authoring_error(&e).into()),
        }),
    }
}
pub(crate) fn create_label(kind: K) -> SharedString {
    t!(match kind {
        K::PhysicsGroup => "physics.add_group",
        K::PoseGroup => "authoring.new_pose_group",
        K::PoseEntry => "authoring.new_pose_entry",
        K::PoseLink => "authoring.new_pose_link",
        _ => unreachable!(),
    })
    .into()
}
impl EditorView {
    pub(crate) fn run_business_action(
        &mut self,
        context: AuthoringContext,
        action: A,
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
                self.prepare_property_inputs(context.editing.document, window, cx),
                t!("authoring.candidate_active")
            );
            let context = AuthoringContext::capture(&self.editor).unwrap();
            let plan = self.editor.document().unwrap().plan_business(
                &action,
                match &action {
                    A::Create(kind, _) => create_label(*kind).to_string(),
                    _ => String::new(),
                },
            )?;
            if plan.removed.len() > 1 || !plan.moved.is_empty() {
                let owner = cx.weak_entity();
                window.defer(cx, move |window, cx| {
                    if let Err(error) =
                        BusinessDialog::open(owner.clone(), context, Some(plan), None, window, cx)
                    {
                        let _ =
                            owner.update(cx, |v, cx| v.set_edit_error(authoring_error(&error), cx));
                    }
                });
            } else {
                self.apply_business(context, &plan, window, cx)?;
            }
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(error) = result {
            self.set_edit_error(authoring_error(&error), cx);
        }
    }
    fn apply_business(
        &mut self,
        context: AuthoringContext,
        plan: &BusinessPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.modal && AuthoringContext::capture(&self.editor) == Some(context),
            t!("authoring.animation_stale")
        );
        let document = context.editing.document;
        let result = self.editor.execute_business(document, plan)?;
        if let Some(kind) = plan.created_kind {
            self.present_created_object(document, &result, kind, window, cx)?;
        } else {
            self.present_authoring_result(
                document,
                &result,
                AuthoringPresentation::AfterDelete {
                    candidates: plan
                        .moved
                        .iter()
                        .cloned()
                        .chain(plan.fallback.iter().cloned())
                        .collect(),
                },
                window,
                cx,
            )?;
        }
        self.animation_anchor = None;
        self.clear_edit_error(cx);
        self.redraw(cx);
        Ok(())
    }
    pub(crate) fn open_business_destination(
        &mut self,
        context: AuthoringContext,
        source: ObjectRef,
        migrate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if AuthoringContext::capture(&self.editor) != Some(context)
            || self.modal
            || window.has_active_dialog(cx)
            || !self.prepare_property_inputs(context.editing.document, window, cx)
        {
            return;
        }
        let context = AuthoringContext::capture(&self.editor).unwrap();
        let owner = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            if let Err(error) = BusinessDialog::open(
                owner.clone(),
                context,
                None,
                Some((source, migrate)),
                window,
                cx,
            ) {
                let _ = owner.update(cx, |v, cx| v.set_edit_error(authoring_error(&error), cx));
            }
        });
    }
}
pub(crate) fn menu(
    owner: WeakEntity<EditorView>,
    source: ObjectRef,
) -> impl Fn(
    gpui_kit::component::menu::PopupMenu,
    &mut Window,
    &mut Context<gpui_kit::component::menu::PopupMenu>,
) -> gpui_kit::component::menu::PopupMenu {
    move |mut menu, window, cx| {
        menu = compact::menu(menu);
        let Some(entity) = owner.upgrade() else {
            return menu;
        };
        let Some(context) = entity.update(cx, |v, cx| {
            let context = AuthoringContext::capture(&v.editor)?;
            (!v.modal && v.prepare_property_inputs(context.editing.document, window, cx))
                .then(|| AuthoringContext::capture(&v.editor))
                .flatten()
        }) else {
            return menu;
        };
        let mut actions = vec![];
        for kind in match source.kind {
            K::Pose => vec![K::PoseGroup],
            K::PoseGroup => vec![K::PoseGroup, K::PoseEntry],
            K::PoseEntry | K::PoseLink => vec![K::PoseEntry, K::PoseLink],
            K::Physics | K::PhysicsGroup => vec![K::PhysicsGroup],
            _ => vec![],
        } {
            actions.push((create_label(kind), A::Create(kind, Some(source.clone()))));
        }
        actions.extend([
            (
                t!("authoring.copy_business").into(),
                A::Copy(source.clone()),
            ),
            (
                t!("authoring.delete_business").into(),
                A::Delete(source.clone()),
            ),
        ]);
        let mut items = Vec::new();
        for (label, action) in actions {
            let Some(availability) = availability(&entity.read(cx).editor, &action) else {
                continue;
            };
            let reason = availability.reason.map(|reason| reason.to_string());
            let section = match &action {
                A::Create(..) => 1,
                A::Copy(_) => 2,
                A::Delete(_) => 3,
                _ => unreachable!("only create, copy and delete are direct menu actions"),
            };
            let owner = owner.clone();
            items.push((
                section,
                PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |v, cx| {
                        if v.authoring_menu_feedback(context, reason.as_deref(), window, cx) {
                            return;
                        }
                        v.run_business_action(context, action.clone(), window, cx)
                    });
                }),
            ));
        }
        if matches!(
            source.kind,
            K::PoseGroup | K::PoseEntry | K::PoseLink | K::PhysicsInput | K::PhysicsOutput
        ) {
            let migrate = source.kind == K::PoseGroup;
            let action = if migrate {
                zipzi_editor_core::AuthoringAction::Dissolve
            } else {
                zipzi_editor_core::AuthoringAction::Move
            };
            if let Some(available) = ActionAvailability::for_object(
                entity.read(cx).editor.document().unwrap(),
                &source,
                action,
            ) {
                let owner = owner.clone();
                let source = source.clone();
                items.push((
                    2,
                    PopupMenuItem::new(
                        t!(if migrate {
                            "authoring.migrate_pose_group"
                        } else {
                            "authoring.move_to"
                        })
                        .into_owned(),
                    )
                    .disabled(!available.enabled)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |v, cx| {
                            v.open_business_destination(
                                context,
                                source.clone(),
                                migrate,
                                window,
                                cx,
                            )
                        });
                    }),
                ));
            }
        }
        items.sort_by_key(|(section, _)| *section);
        let mut previous = None;
        for (section, item) in items {
            if previous.is_some_and(|previous| previous != section) {
                menu = menu.separator();
            }
            menu = menu.item(item);
            previous = Some(section);
        }
        menu
    }
}

struct BusinessDialog {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    dialog: FormDialogState,
    playback: PlaybackDialogGuard,
    plan: Result<BusinessPlan, String>,
    source: Option<(ObjectRef, bool)>,
    destinations: Vec<zipzi_editor_core::BusinessDestination>,
    destination: Entity<SelectState<SearchableVec<Choice>>>,
    node: Entity<SelectState<SearchableVec<Choice>>>,
    rows: std::sync::Arc<[String]>,
    _subscriptions: Vec<Subscription>,
}
impl BusinessDialog {
    fn valid(&self, cx: &App) -> bool {
        self.owner.upgrade().is_some_and(|o| {
            !o.read(cx).modal && AuthoringContext::capture(&o.read(cx).editor) == Some(self.context)
        })
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.plan = (|| {
            anyhow::ensure!(self.valid(cx), t!("authoring.animation_stale"));
            let (source, migrate) = self.source.as_ref().unwrap();
            let destination = self
                .destination
                .read(cx)
                .selected_value()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!(t!("authoring.move_choose")))?;
            let action = if *migrate {
                A::MigratePoseGroup(source.clone(), destination)
            } else {
                A::Move(
                    source.clone(),
                    destination,
                    self.node.read(cx).selected_value().cloned(),
                )
            };
            self.owner
                .upgrade()
                .unwrap()
                .read(cx)
                .editor
                .document()
                .unwrap()
                .plan_business(&action, String::new())
        })()
        .map_err(|e| authoring_error(&e));
        self.sync_rows(cx);
        self.dialog.edited();
        self.dialog
            .set_validation(self.plan.as_ref().is_ok_and(|p| !p.noop), true);
        cx.notify();
    }
    fn sync_rows(&mut self, cx: &App) {
        let mut rows = vec![];
        if let Ok(plan) = &self.plan {
            let owner = self.owner.upgrade().unwrap();
            let index = owner.read(cx).editor.document().unwrap().object_index();
            for (label, objects) in [
                ("authoring.removal_deleted", &plan.removed),
                ("authoring.business_moved", &plan.moved),
            ] {
                if objects.is_empty() {
                    continue;
                }
                rows.push(format!("{} ({})", t!(label), objects.len()));
                rows.extend(
                    objects
                        .iter()
                        .map(|o| format!("{} · {}", index.name(o), o.id)),
                );
            }
        }
        self.rows = rows.into();
    }
    fn open(
        owner: WeakEntity<EditorView>,
        context: AuthoringContext,
        plan: Option<BusinessPlan>,
        source: Option<(ObjectRef, bool)>,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("authoring.animation_stale")))?;
        anyhow::ensure!(
            !window.has_active_dialog(cx)
                && AuthoringContext::capture(&entity.read(cx).editor) == Some(context),
            t!("authoring.animation_stale")
        );
        let destinations = source
            .as_ref()
            .map(|(o, migrate)| {
                entity
                    .read(cx)
                    .editor
                    .document()
                    .unwrap()
                    .business_destinations(o, *migrate)
            })
            .transpose()?
            .unwrap_or_default();
        let choices: Vec<Choice> = destinations
            .iter()
            .map(|d| Choice {
                value: d.id.clone(),
                label: d.label.clone().into(),
            })
            .collect();
        let playback = PlaybackDialogGuard::new(context);
        entity.update(cx, |v, _| v.playback_dialog = Some(playback.clone()));
        let picking = source.is_some();
        let view = cx.new(|cx| {
            let destination = cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices), None, window, cx).searchable(true)
            });
            let node = cx.new(|cx| {
                SelectState::new(SearchableVec::new(vec![]), None, window, cx).searchable(true)
            });
            let event = cx.subscribe_in(
                &destination,
                window,
                |this: &mut Self, _, event, window, cx| {
                    if matches!(event, SelectEvent::Confirm(_)) {
                        let id = this.destination.read(cx).selected_value().cloned();
                        let nodes: Vec<Choice> = this
                            .destinations
                            .iter()
                            .find(|d| Some(&d.id) == id.as_ref())
                            .map(|d| {
                                d.nodes
                                    .iter()
                                    .map(|(value, label)| Choice {
                                        value: value.clone(),
                                        label: label.clone().into(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        this.node.update(cx, |state, cx| {
                            state.set_items(SearchableVec::new(nodes), window, cx);
                            state.set_selected_index(None, window, cx);
                        });
                        this.refresh(cx);
                    }
                },
            );
            let node_event = cx.subscribe_in(&node, window, |this: &mut Self, _, event, _, cx| {
                if matches!(event, SelectEvent::Confirm(_)) {
                    this.refresh(cx);
                }
            });
            let observe = cx.observe(&entity, |this: &mut Self, _, cx| {
                this.dialog.set_validation(
                    this.valid(cx) && this.plan.as_ref().is_ok_and(|p| !p.noop),
                    true,
                );
                cx.notify();
            });
            let mut dialog = FormDialogState::new(cx);
            dialog.set_validation(plan.as_ref().is_some_and(|p| !p.noop), true);
            let mut this = Self {
                owner,
                context,
                dialog,
                playback,
                plan: plan.ok_or_else(|| t!("authoring.move_choose").into_owned()),
                source,
                destinations,
                destination,
                node,
                rows: std::sync::Arc::from([]),
                _subscriptions: vec![event, node_event, observe],
            };
            this.sync_rows(cx);
            this
        });
        let focus = if picking {
            view.read(cx).destination.read(cx).focus_handle(cx)
        } else {
            view.read(cx).dialog.cancel_focus.clone()
        };
        form_dialog::open(
            view,
            FormDialogOptions {
                title: t!(if picking {
                    "authoring.move_to"
                } else {
                    "authoring.review_business"
                })
                .into_owned(),
                submit_label: t!(if picking {
                    "authoring.move_submit"
                } else {
                    "authoring.apply_deletion"
                })
                .into(),
                footer_id: "business-dialog-footer",
                cancel_id: "business-dialog-cancel",
                submit_id: "business-dialog-submit",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
}
impl Drop for BusinessDialog {
    fn drop(&mut self) {
        self.playback.close();
    }
}
impl FormDialogView for BusinessDialog {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn closing(&mut self) {
        self.playback.close();
    }
    fn error_message(e: &anyhow::Error) -> String {
        authoring_error(e)
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(self.valid(cx), t!("authoring.animation_stale"));
        let plan = self.plan.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
        anyhow::ensure!(!plan.noop, t!("authoring.no_change"));
        self.owner
            .update(cx, |v, cx| v.apply_business(self.context, plan, window, cx))??;
        Ok(())
    }
}
impl Render for BusinessDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let valid = self.valid(cx);
        let rows = self.rows.clone();
        let output = self.source.as_ref().is_some_and(|(o, _)| {
            o.kind == K::PhysicsOutput
                && self
                    .destination
                    .read(cx)
                    .selected_value()
                    .is_some_and(|id| {
                        self.owner
                            .upgrade()
                            .and_then(|owner| owner.read(cx).editor.document()?.business_owner(o))
                            .is_some_and(|g| g.id != *id)
                    })
        });
        div()
            .id("business-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .text_sm()
            .child(t!("authoring.business_effects_hint"))
            .when(self.source.is_some(), |e| {
                e.child(
                    compact::select(Select::new(&self.destination))
                        .id("business-destination")
                        .placeholder(t!("authoring.move_choose"))
                        .disabled(!valid)
                        .w_full(),
                )
            })
            .when(output, |e| {
                e.child(t!("authoring.choose_output_node")).child(
                    compact::select(Select::new(&self.node))
                        .id("business-output-node")
                        .disabled(!valid)
                        .w_full(),
                )
            })
            .child(
                uniform_list("business-effects", rows.len(), move |range, _, _| {
                    range
                        .map(|i| {
                            div()
                                .id(("business-effect", i))
                                .test_support()
                                .role(Role::Label)
                                .aria_label(rows[i].clone())
                                .h(compact::rem(compact::ROW))
                                .text_xs()
                                .text_ellipsis()
                                .overflow_hidden()
                                .child(rows[i].clone())
                        })
                        .collect()
                })
                .h(compact::rem(self.rows.len().min(10) as f32 * compact::ROW))
                .w_full(),
            )
            .when_some(
                if !valid {
                    Some(t!("authoring.animation_stale").into_owned())
                } else {
                    self.dialog
                        .error
                        .clone()
                        .or_else(|| self.plan.as_ref().err().cloned())
                        .or_else(|| {
                            self.plan
                                .as_ref()
                                .ok()
                                .filter(|p| p.noop)
                                .map(|_| t!("authoring.no_change").into_owned())
                        })
                },
                |e, reason| {
                    e.child(
                        div()
                            .id("business-dialog-reason")
                            .test_support()
                            .role(Role::Label)
                            .aria_label(reason.clone())
                            .child(reason),
                    )
                },
            )
    }
}
