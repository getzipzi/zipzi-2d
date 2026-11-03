use super::*;
use crate::ui::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use zipzi_core::{DeformerOriginalAction as Action, DeformerOriginalSource as Source};
use zipzi_editor_core::{DeformerOriginalInfo, DeformerOriginalTarget, DocumentId, EditorCommand};

impl PropertiesPanel {
    pub(super) fn deformer_original_actions(&self, cx: &Context<Self>) -> AnyElement {
        let target = self.target.clone().unwrap();
        let panel = cx.weak_entity();
        let epoch = self.epoch;
        let info = self.owner.upgrade().and_then(|o| {
            o.read(cx)
                .editor
                .document()?
                .deformer_original_info(&target.object)
        });
        let key = match info.as_ref().and_then(|i| i.source) {
            Some(Source::Created) => "properties.original_created",
            Some(Source::FirstKey) => "properties.original_first_key",
            Some(Source::Manual) => "properties.original_manual",
            None => "properties.original_missing",
        };
        let mut body = div()
            .id("property-deformer-original")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!(key).into_owned()),
            );
        if let Some(error) = info.as_ref().and_then(|i| i.capture_error.as_ref()) {
            body = body.child(div().child(
                t!("properties.original_capture_error", code = error.as_str()).into_owned(),
            ));
        }
        let mut actions = div().flex().gap_2();
        for (id, label, hint, restore) in [
            (
                "property-original-update",
                "properties.original_update",
                "properties.original_update_hint",
                false,
            ),
            (
                "property-original-restore",
                "properties.original_restore",
                "properties.original_restore_hint",
                true,
            ),
        ] {
            let owner = self.owner.clone();
            let target = target.clone();
            let panel = panel.clone();
            actions = actions.child(
                compact::button(Button::new(id))
                    .flex_1()
                    .min_w_0()
                    .label(t!(label).into_owned())
                    .tooltip(t!(hint))
                    .disabled(
                        self.disabled
                            || !target.scope.shape_editable()
                            || (restore && info.as_ref().is_none_or(|i| i.source.is_none())),
                    )
                    .on_click(move |_, window, cx| {
                        if window.has_active_dialog(cx)
                            || !panel.upgrade().is_some_and(|p| {
                                let p = p.read(cx);
                                p.visible && p.epoch == epoch && p.target.as_ref() == Some(&target)
                            })
                        {
                            return;
                        }
                        let _ = owner.update(cx, |view, cx| {
                            if view.editor.active_id() != Some(target.document)
                                || !view.prepare_property_inputs(target.document, window, cx)
                            {
                                return;
                            }
                            let result = (|| {
                                view.editor.end_editing(
                                    target.document,
                                    zipzi_editor_core::EditingLifecycle::BlockingUi,
                                )?;
                                let frozen = view
                                    .editor
                                    .deformer_original_target(&target.object)
                                    .ok_or_else(|| anyhow::anyhow!("Deformer changed"))?;
                                let info = view
                                    .editor
                                    .document()
                                    .unwrap()
                                    .deformer_original_info(&target.object)
                                    .unwrap();
                                if restore && info.requires_conversion() {
                                    OriginalRestoreDialog::open(
                                        owner.clone(),
                                        target.document,
                                        frozen,
                                        info,
                                        window,
                                        cx,
                                    );
                                    view.redraw(cx);
                                    Ok(())
                                } else {
                                    view.editor.execute(EditorCommand::DeformerOriginal {
                                        target: frozen,
                                        action: if restore {
                                            Action::Restore {
                                                allow_resize: false,
                                            }
                                        } else {
                                            Action::Update
                                        },
                                    })
                                }
                            })();
                            view.complete_edit(result, cx);
                        });
                    }),
            );
        }
        body.child(actions).into_any_element()
    }
}
struct OriginalRestoreDialog {
    owner: WeakEntity<EditorView>,
    document: DocumentId,
    target: DeformerOriginalTarget,
    info: DeformerOriginalInfo,
    dialog: FormDialogState,
}
impl OriginalRestoreDialog {
    fn open(
        owner: WeakEntity<EditorView>,
        document: DocumentId,
        target: DeformerOriginalTarget,
        info: DeformerOriginalInfo,
        window: &mut Window,
        cx: &mut App,
    ) {
        let form = cx.new(|cx| {
            let mut dialog = FormDialogState::new(cx);
            dialog.set_validation(true, true);
            Self {
                owner,
                document,
                target,
                info,
                dialog,
            }
        });
        let focus = form.read(cx).dialog.cancel_focus.clone();
        form_dialog::open(
            form,
            FormDialogOptions {
                title: t!("properties.original_restore").into_owned(),
                submit_label: t!("properties.original_restore").into(),
                footer_id: "original-restore-footer",
                cancel_id: "original-restore-cancel",
                submit_id: "original-restore-apply",
                initial_focus: focus,
            },
            window,
            cx,
        );
    }
    fn current(&self, cx: &App) -> bool {
        self.owner.upgrade().is_some_and(|o| {
            let editor = &o.read(cx).editor;
            editor.active_id() == Some(self.document)
                && editor
                    .deformer_original_target(&self.target.object)
                    .as_ref()
                    == Some(&self.target)
        })
    }
}
impl FormDialogView for OriginalRestoreDialog {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        error.to_string()
    }
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(self.current(cx), t!("properties.warp_preview_expired"));
        self.owner.update(cx, |view, cx| {
            view.editor.execute(EditorCommand::DeformerOriginal {
                target: self.target.clone(),
                action: Action::Restore { allow_resize: true },
            })?;
            view.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })?
    }
}
impl Render for OriginalRestoreDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.dialog.closed() {
            return div().into_any_element();
        }
        if !self.current(cx) {
            self.dialog.error = Some(t!("properties.warp_preview_expired").into_owned());
            self.dialog.set_validation(false, true);
        }
        let mut body = div()
            .id("original-restore-review")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .child(t!("properties.original_restore_conversion").into_owned());
        for (grid, key) in [
            (
                self.info.conversion_grid,
                "properties.original_conversion_grid",
            ),
            (
                self.info.controller_grid,
                "properties.original_controller_grid",
            ),
        ] {
            if let Some((a, b)) = grid {
                body = body.child(
                    t!(
                        key,
                        from_columns = a[0],
                        from_rows = a[1],
                        to_columns = b[0],
                        to_rows = b[1]
                    )
                    .into_owned(),
                );
            }
        }
        if self.info.interpolation_change {
            body = body.child(t!("properties.original_interpolation_change").into_owned());
        }
        if self.info.conversion_grid.is_some() || self.info.interpolation_change {
            body = body.child(t!("properties.original_conversion_effect").into_owned());
        }
        if self.info.controller_grid.is_some() {
            body = body.child(t!("properties.original_controller_effect").into_owned());
        }
        if let Some(error) = &self.dialog.error {
            body = body.child(
                div()
                    .id("original-restore-error")
                    .test_support()
                    .aria_label(error.clone())
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        body.into_any_element()
    }
}
