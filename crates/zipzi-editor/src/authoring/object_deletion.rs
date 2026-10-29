//! Frozen object selection, native dependency validation and deletion review.
use super::*;
use crate::ui::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use zipzi_editor_core::{Command, CommandTarget, EditingContext, TreeDeletionPlan};

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
    // Validate the entire frozen selection; individual capabilities would reject
    // references between two selected objects that can be deleted together.
    let plan = state
        .editor
        .document()
        .unwrap()
        .plan_tree_deletion(command.deletion_projection(), objects);
    let reason = if state.editor.manual_active() || state.editor.gesture_active() {
        Some(t!("authoring.candidate_active").into_owned())
    } else if objects.is_empty() {
        Some(t!("authoring.delete_select_objects").into_owned())
    } else {
        plan.as_ref().err().map(authoring_error)
    };
    menu.separator().item(
        PopupMenuItem::new(t!(command.id.spec().label).into_owned())
            .action(Box::new(crate::actions::RunCommand::targeted(command)))
            .on_click(move |_, window, cx| {
                let _ = owner.update(cx, |view, cx| {
                    if view.authoring_menu_feedback(context, reason.as_deref(), window, cx) { return; }
                    let Ok(plan) = &plan else { return };
                    if matches!(command.target, CommandTarget::Viewport(binding) if !view.editor.viewport_binding_valid(binding)) {
                        return;
                    }
                    view.run_object_deletion(context, plan, window, cx);
                });
            }),
    )
}
impl EditorView {
    pub(crate) fn handle_object_deletion_command(
        &mut self,
        command: zipzi_editor_core::Command,
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
        let result = self.editor.document().unwrap().plan_tree_deletion(
            command.deletion_projection(),
            &self.editor.document().unwrap().context().selections,
        );
        match result {
            Ok(plan) => self.run_object_deletion(context, &plan, window, cx),
            Err(error) => self.set_edit_error(authoring_error(&error), cx),
        }
    }
    fn run_object_deletion(
        &mut self,
        context: AuthoringContext,
        plan: &TreeDeletionPlan,
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
            anyhow::ensure!(
                self.prepare_property_inputs(context.editing.document, window, cx),
                t!("authoring.candidate_active")
            );
            anyhow::ensure!(
                AuthoringContext::capture(&self.editor) == Some(context),
                t!("authoring.move_stale")
            );
            if plan.removed_ids().len() > 1 {
                let owner = cx.weak_entity();
                let plan = plan.clone();
                window.defer(cx, move |window, cx| {
                    DeletionDialog::open(owner, context, plan, window, cx)
                });
            } else {
                self.apply_tree_deletion(context, plan, cx)?;
            }
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(error) = result {
            self.set_edit_error(authoring_error(&error), cx);
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
        self.clear_edit_error(cx);
        self.redraw(cx);
        Ok(())
    }
}
struct DeletionDialog {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    plan: TreeDeletionPlan,
    dialog: FormDialogState,
    rows: std::sync::Arc<[String]>,
    _observe: Subscription,
}
impl DeletionDialog {
    fn valid(&self, cx: &App) -> bool {
        self.owner.upgrade().is_some_and(|owner| {
            let view = owner.read(cx);
            !view.modal
                && !view.editor.manual_active()
                && AuthoringContext::capture(&view.editor) == Some(self.context)
        })
    }
    fn open(
        owner: WeakEntity<EditorView>,
        context: AuthoringContext,
        plan: TreeDeletionPlan,
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
        let index = entity.read(cx).editor.document().unwrap().object_index();
        let rows = plan
            .removed_ids()
            .iter()
            .map(|id| {
                index
                    .objects
                    .iter()
                    .find(|info| info.reference.id == *id)
                    .map_or_else(|| id.clone(), |info| format!("{} · {id}", info.name))
            })
            .collect::<Vec<_>>()
            .into();
        let view = cx.new(|cx| {
            let mut dialog = FormDialogState::new(cx);
            dialog.set_validation(true, true);
            DeletionDialog {
                owner,
                context,
                plan,
                dialog,
                rows,
                _observe: cx.observe(&entity, |this: &mut Self, _, cx| {
                    this.dialog.set_validation(this.valid(cx), true);
                    cx.notify();
                }),
            }
        });
        let initial_focus = view.read(cx).dialog.cancel_focus.clone();
        form_dialog::open(
            view,
            FormDialogOptions {
                title: t!("authoring.delete_object").into_owned(),
                submit_label: t!("authoring.apply_deletion").into(),
                footer_id: "tree-deletion-footer",
                cancel_id: "tree-deletion-cancel",
                submit_id: "tree-deletion-submit",
                initial_focus,
            },
            window,
            cx,
        );
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
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(self.valid(cx), t!("authoring.move_stale"));
        self.owner.update(cx, |view, cx| {
            view.apply_tree_deletion(self.context, &self.plan, cx)
        })??;
        Ok(())
    }
}
impl Render for DeletionDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        div()
            .id("tree-deletion-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .child(format!(
                "{} ({})",
                t!("authoring.removal_deleted"),
                rows.len()
            ))
            .child(
                uniform_list("tree-deletion-effects", rows.len(), move |range, _, _| {
                    range
                        .map(|i| {
                            div()
                                .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                                .text_xs()
                                .text_ellipsis()
                                .overflow_hidden()
                                .child(rows[i].clone())
                        })
                        .collect()
                })
                .h(crate::ui::compact::rem(
                    self.rows.len().min(10) as f32 * crate::ui::compact::ROW,
                ))
                .w_full(),
            )
            .when_some(
                if !self.valid(cx) {
                    Some(t!("authoring.move_stale").into_owned())
                } else {
                    self.dialog.error.clone()
                },
                |e, reason| e.child(reason),
            )
    }
}
