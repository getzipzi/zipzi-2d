//! Incremental numeric transforms share the canvas's frozen Warp destination.
use super::*;
use crate::authoring::AuthoringContext;
use crate::ui::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use zipzi_editor_core::{DeformerAdjustment, EditorCommand, ModelVector, WarpTransformTarget};

impl PropertiesPanel {
    pub(super) fn warp_transform_action(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = self.target.clone().unwrap();
        let owner = self.owner.clone();
        let panel = cx.weak_entity();
        let epoch = self.epoch;
        let context = self.context;
        let disabled = owner.upgrade().is_none_or(|owner| {
            let view = owner.read(cx);
            view.modal
                || view.editor.is_busy()
                || view.editor.document().is_none_or(|doc| {
                    doc.ensure_editable(std::slice::from_ref(&target.object))
                        .is_err()
                })
        });
        compact::button(Button::new("property-warp-transform"))
            .label(t!("properties.warp_transform").into_owned())
            .disabled(disabled)
            .on_click(move |_, window, cx| {
                if window.has_active_dialog(cx)
                    || !panel.upgrade().is_some_and(|panel| {
                        let panel = panel.read(cx);
                        panel.visible
                            && panel.epoch == epoch
                            && panel.target.as_ref() == Some(&target)
                    })
                {
                    return;
                }
                let result =
                    WarpTransformDialog::open(owner.clone(), &target.object, context, window, cx);
                if let Err(error) = result {
                    let _ = owner.update(cx, |view, cx| view.set_edit_error(error.to_string(), cx));
                }
            })
            .into_any_element()
    }
}

struct WarpTransformDialog {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    target: WarpTransformTarget,
    inputs: [Entity<InputState>; 4],
    dialog: FormDialogState,
    _subscriptions: Vec<Subscription>,
}
impl WarpTransformDialog {
    fn open(
        owner: WeakEntity<EditorView>,
        object: &ObjectRef,
        expected: Option<zipzi_editor_core::EditingContextToken>,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        let (context, target) = entity.update(cx, |view, cx| {
            anyhow::ensure!(
                view.editor.editing_context_token() == expected,
                "Editing context changed"
            );
            let id = view
                .editor
                .active_id()
                .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
            anyhow::ensure!(
                view.prepare_property_inputs(id, window, cx),
                "Finish the active property edit"
            );
            view.editor
                .end_editing(id, zipzi_editor_core::EditingLifecycle::BlockingUi)?;
            let context = AuthoringContext::capture(&view.editor).unwrap();
            let target = view
                .editor
                .warp_transform_target(object)
                .ok_or_else(|| anyhow::anyhow!("Select a Warp deformer"))?;
            view.redraw(cx);
            Ok::<_, anyhow::Error>((context, target))
        })?;
        let form = cx.new(|cx| {
            let inputs = ["0", "0", "0", "100"]
                .map(|text| cx.new(|cx| InputState::new(window, cx).default_value(text)));
            let subscriptions = inputs
                .iter()
                .map(|input| {
                    cx.subscribe_in(input, window, |this: &mut Self, _, event, window, cx| {
                        if this.dialog.closed() {
                            return;
                        }
                        if matches!(event, InputEvent::Change) {
                            this.validate(cx);
                        } else {
                            form_dialog::input_enter::<Self>(event, window, cx);
                        }
                    })
                })
                .collect();
            Self {
                owner,
                context,
                target,
                inputs,
                dialog: FormDialogState::new(cx),
                _subscriptions: subscriptions,
            }
        });
        let focus = form.read(cx).inputs[0].read(cx).focus_handle(cx);
        form_dialog::open(
            form,
            FormDialogOptions {
                title: t!("properties.warp_transform").into_owned(),
                submit_label: t!("properties.warp_apply").into(),
                footer_id: "warp-transform-footer",
                cancel_id: "warp-transform-cancel",
                submit_id: "warp-transform-apply",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
    fn draft(&self, cx: &App) -> anyhow::Result<DeformerAdjustment> {
        let values = self
            .inputs
            .iter()
            .map(|input| input.read(cx).value().trim().parse::<f32>())
            .collect::<Result<Vec<_>, _>>()?;
        let scale = values[3] / 100.;
        anyhow::ensure!(
            values.iter().all(|v| v.is_finite()) && scale > 0.,
            t!("properties.warp_transform_invalid")
        );
        Ok(DeformerAdjustment {
            translation: ModelVector {
                x: values[0],
                y: values[1],
            },
            angle: values[2],
            scale,
        })
    }
    fn validate(&mut self, cx: &mut Context<Self>) {
        self.dialog.edited();
        let result = self.draft(cx);
        self.dialog.set_validation(
            result.is_ok(),
            result.as_ref().is_ok_and(|v| {
                v.translation != ModelVector::default() || v.angle != 0. || v.scale != 1.
            }),
        );
        if let Err(error) = result {
            self.dialog.error = Some(error.to_string());
        }
        cx.notify();
    }
    fn current(&self, editor: &zipzi_editor_core::Editor) -> bool {
        AuthoringContext::capture(editor) == Some(self.context)
            && editor.warp_transform_target(&self.target.object).as_ref() == Some(&self.target)
    }
}
impl FormDialogView for WarpTransformDialog {
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
        let adjustment = self.draft(cx)?;
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        owner.update(cx, |view, cx| {
            anyhow::ensure!(
                self.current(&view.editor),
                t!("properties.warp_preview_expired")
            );
            view.editor.execute(EditorCommand::AdjustWarp {
                target: self.target.clone(),
                adjustment,
            })?;
            view.redraw(cx);
            Ok(())
        })
    }
}
impl Render for WarpTransformDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.dialog.closed()
            && !self
                .owner
                .upgrade()
                .is_some_and(|owner| self.current(&owner.read(cx).editor))
        {
            self.dialog.error = Some(t!("properties.warp_preview_expired").into_owned());
            self.dialog.set_validation(false, true);
        }
        let label = if self.target.components.is_empty() {
            t!("properties.warp_target_whole").into_owned()
        } else {
            t!(
                "properties.warp_target_selection",
                count = self.target.components.len()
            )
            .into_owned()
        };
        let mut body = div()
            .id("warp-transform-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .text_sm()
            .child(label)
            .child(t!("properties.warp_transform_hint").into_owned());
        for (i, (id, label)) in [
            ("warp-transform-x", "properties.warp_transform_x"),
            ("warp-transform-y", "properties.warp_transform_y"),
            ("warp-transform-angle", "properties.rotation_angle"),
            ("warp-transform-scale", "properties.warp_transform_scale"),
        ]
        .into_iter()
        .enumerate()
        {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().child(t!(label).into_owned()))
                    .child(
                        div().w(px(140.)).child(
                            compact::input(Input::new(&self.inputs[i]))
                                .id(id)
                                .aria_label(t!(label).into_owned()),
                        ),
                    ),
            );
        }
        if let Some(error) = &self.dialog.error {
            body = body.child(div().text_color(cx.theme().danger).child(error.clone()));
        }
        body
    }
}
