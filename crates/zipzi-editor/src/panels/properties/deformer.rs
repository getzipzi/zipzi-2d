//! Warp density is a definition edit with an explicit, cancellable preview.
use super::*;
use crate::properties::{self, Choice, PreviewToken};
use crate::ui::form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView};
use gpui_kit::component::{
    IndexPath,
    input::{Input, InputEvent, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectState},
};
use zipzi_editor_core::property::deformer as fields;
use zipzi_editor_core::{
    PropertyPatch, PropertyScope, PropertyValue, PropertyValues, WarpGridPreviewMetrics,
};

impl PropertiesPanel {
    pub(super) fn warp_keep_children_action(&self, cx: &Context<Self>) -> AnyElement {
        let owner = self.owner.clone();
        let target = self.target.clone().unwrap();
        let panel = cx.weak_entity();
        let epoch = self.epoch;
        let active = owner
            .upgrade()
            .is_some_and(|o| o.read(cx).editor.warp_keep_children());
        compact::toggle(Button::new("property-warp-keep-children"), active)
            .label(t!("properties.warp_keep_children").into_owned())
            .tooltip(t!("properties.warp_keep_children_hint"))
            .disabled(self.disabled)
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
                    let result = view
                        .editor
                        .execute(zipzi_editor_core::EditorCommand::ToggleWarpKeepChildren);
                    view.complete_edit(result, cx);
                });
            })
            .into_any_element()
    }
    pub(super) fn warp_shape_actions(&self, cx: &Context<Self>) -> AnyElement {
        let target = self.target.clone().unwrap();
        let epoch = self.epoch;
        let panel = cx.weak_entity();
        let context = self
            .owner
            .upgrade()
            .and_then(|o| crate::authoring::AuthoringContext::capture(&o.read(cx).editor));
        let mut body = div()
            .id("property-warp-shape")
            .test_support()
            .flex()
            .gap_2();
        for (id, label, hint, action) in [
            (
                "property-warp-reset",
                "properties.warp_reset_controller",
                "properties.warp_reset_controller_hint",
                zipzi_core::WarpShapeAction::ResetController,
            ),
            (
                "property-warp-reform",
                "properties.warp_reform",
                "properties.warp_reform_hint",
                zipzi_core::WarpShapeAction::Reform,
            ),
        ] {
            let owner = self.owner.clone();
            let target = target.clone();
            let panel = panel.clone();
            body = body.child(
                compact::button(Button::new(id))
                    .label(t!(label).into_owned())
                    .tooltip(t!(hint))
                    .disabled(self.disabled || !target.scope.shape_editable())
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
                            if view.modal
                                || crate::authoring::AuthoringContext::capture(&view.editor)
                                    != context
                            {
                                return;
                            }
                            if !view.prepare_property_inputs(target.document, window, cx) {
                                return;
                            }
                            let result = (|| {
                                let binding = view
                                    .warp_property_viewport()
                                    .ok_or_else(|| anyhow::anyhow!("Warp viewport closed"))?;
                                let target = view
                                    .editor
                                    .warp_transform_target_for(binding, &target.object)
                                    .ok_or_else(|| anyhow::anyhow!("Warp changed"))?;
                                view.editor
                                    .execute(zipzi_editor_core::EditorCommand::WarpShape {
                                        target,
                                        action,
                                    })
                            })();
                            view.complete_edit(result, cx);
                        });
                    }),
            );
        }
        body.into_any_element()
    }
    pub(super) fn warp_grid_action(&self, cx: &mut Context<Self>) -> AnyElement {
        self.warp_density_action(false, cx)
    }
    pub(super) fn warp_bezier_grid_action(&self, cx: &mut Context<Self>) -> AnyElement {
        self.warp_density_action(true, cx)
    }
    fn warp_density_action(&self, bezier: bool, cx: &mut Context<Self>) -> AnyElement {
        let target = self.target.clone().unwrap();
        let owner = self.owner.clone();
        let panel = cx.weak_entity();
        let epoch = self.epoch;
        let context = self.context;
        compact::button(Button::new(if bezier {
            "property-warp-bezier-grid"
        } else {
            "property-warp-grid"
        }))
        .label(
            t!(if bezier {
                "properties.warp_bezier_grid"
            } else {
                "properties.warp_grid"
            })
            .into_owned(),
        )
        .disabled(self.disabled)
        .on_click(move |_, window, cx| {
            if !panel.upgrade().is_some_and(|panel| {
                let panel = panel.read(cx);
                panel.visible && panel.epoch == epoch && panel.target.as_ref() == Some(&target)
            }) || window.has_active_dialog(cx)
            {
                return;
            }
            if !owner
                .upgrade()
                .is_some_and(|owner| owner.read(cx).editor.editing_context_token() == context)
            {
                return;
            }
            let result = WarpGridDialog::open(owner.clone(), target.clone(), bezier, window, cx);
            if let Err(error) = result {
                let _ = owner.update(cx, |view, cx| view.set_edit_error(error.to_string(), cx));
            }
        })
        .into_any_element()
    }
}

struct WarpGridDialog {
    bezier: bool,
    owner: WeakEntity<EditorView>,
    target: PropertyTarget,
    context: zipzi_editor_core::EditingContextToken,
    baseline: PropertyValues,
    inputs: [Entity<InputState>; 2],
    interpolation: Entity<SelectState<SearchableVec<Choice>>>,
    preview: Option<PreviewToken>,
    metrics: Option<WarpGridPreviewMetrics>,
    dialog: FormDialogState,
    _subscriptions: Vec<Subscription>,
}
impl WarpGridDialog {
    fn open(
        owner: WeakEntity<EditorView>,
        target: PropertyTarget,
        bezier: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let view = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        view.update(cx, |view, cx| {
            anyhow::ensure!(
                view.editor.active_id() == Some(target.document)
                    && view.editor.editing_context() == Some(target.context),
                "Editing context changed"
            );
            anyhow::ensure!(
                !view.editor.document().is_some_and(|doc| {
                    properties::deformer::requires_single_selection(doc, &target.object)
                }),
                t!("properties.deformer_single_selection")
            );
            anyhow::ensure!(
                view.prepare_property_inputs(target.document, window, cx),
                "Finish the active property edit"
            );
            view.editor.end_editing(
                target.document,
                zipzi_editor_core::EditingLifecycle::BlockingUi,
            )?;
            view.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })?;
        let editor = &view.read(cx).editor;
        let context = editor
            .editing_context_token()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        let values = target.values(editor)?;
        let specs = if bezier {
            vec![fields::BEZIER_COLUMNS, fields::BEZIER_ROWS]
        } else {
            vec![fields::COLUMNS, fields::ROWS, fields::QUAD]
        };
        let baseline: PropertyValues = specs
            .into_iter()
            .map(|s| {
                values
                    .get(&s.key)
                    .cloned()
                    .map(|v| (s.key, v))
                    .ok_or_else(|| anyhow::anyhow!("Select a Warp deformer"))
            })
            .collect::<anyhow::Result<_>>()?;
        let form = cx.new(|cx| {
            let dimensions = if bezier {
                [fields::BEZIER_COLUMNS, fields::BEZIER_ROWS]
            } else {
                [fields::COLUMNS, fields::ROWS]
            };
            let inputs = dimensions.map(|s| {
                cx.new(|cx| InputState::new(window, cx).default_value(baseline[&s.key].text()))
            });
            let quad = baseline.get(&fields::QUAD.key) == Some(&PropertyValue::Bool(true));
            let interpolation = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(vec![
                        Choice {
                            label: t!("properties.warp_interpolation_standard").into(),
                            value: "standard".into(),
                        },
                        Choice {
                            label: t!("properties.warp_interpolation_quad").into(),
                            value: "quad".into(),
                        },
                    ]),
                    Some(IndexPath::new(usize::from(quad))),
                    window,
                    cx,
                )
            });
            let mut subscriptions = vec![];
            for input in &inputs {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this: &mut Self, _, event, _, cx| {
                        if matches!(event, InputEvent::Change) && !this.dialog.closed() {
                            this.refresh(cx);
                        }
                    },
                ));
            }
            subscriptions.push(cx.subscribe_in(
                &interpolation,
                window,
                |this: &mut Self, _, event, _, cx| {
                    if matches!(event, SelectEvent::Confirm(_)) && !this.dialog.closed() {
                        this.refresh(cx);
                    }
                },
            ));
            cx.on_release(|this: &mut Self, cx| {
                let token = this.preview.take();
                let owner = this.owner.clone();
                cx.defer(move |cx| {
                    if let Some(token) = token {
                        let _ = owner.update(cx, |view, cx| {
                            token.cancel(&mut view.editor);
                            view.redraw(cx);
                        });
                    }
                });
            })
            .detach();
            Self {
                bezier,
                owner,
                target,
                context,
                baseline,
                inputs,
                interpolation,
                preview: None,
                metrics: None,
                dialog: FormDialogState::new(cx),
                _subscriptions: subscriptions,
            }
        });
        let focus = form.read(cx).dialog.cancel_focus.clone();
        form_dialog::open(
            form,
            FormDialogOptions {
                title: t!(if bezier {
                    "properties.warp_bezier_grid"
                } else {
                    "properties.warp_grid"
                })
                .into_owned(),
                submit_label: t!("properties.warp_apply").into(),
                footer_id: "warp-grid-footer",
                cancel_id: "warp-grid-cancel",
                submit_id: "warp-grid-apply",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
    fn patch(&self, cx: &App) -> anyhow::Result<PropertyPatch> {
        let mut values: PropertyValues = (if self.bezier {
            [fields::BEZIER_COLUMNS, fields::BEZIER_ROWS]
        } else {
            [fields::COLUMNS, fields::ROWS]
        })
        .into_iter()
        .zip(&self.inputs)
        .map(|(s, input)| {
            Ok((
                s.key,
                s.parse(input.read(cx).value().as_ref()).map_err(|_| {
                    anyhow::anyhow!(t!(if self.bezier {
                        "properties.warp_bezier_dimensions_invalid"
                    } else {
                        "properties.warp_dimensions_invalid"
                    }))
                })?,
            ))
        })
        .collect::<anyhow::Result<_>>()?;
        if !self.bezier {
            values.insert(
                fields::QUAD.key,
                PropertyValue::Bool(
                    self.interpolation
                        .read(cx)
                        .selected_value()
                        .is_some_and(|v| v == "quad"),
                ),
            );
        }
        values.retain(|key, value| self.baseline.get(key) != Some(value));
        Ok(PropertyPatch {
            scope: PropertyScope::Object,
            expected: self.baseline.clone(),
            changes: values,
        })
    }
    fn current(&self, editor: &zipzi_editor_core::Editor) -> bool {
        editor.editing_context_token_valid(self.context)
            && editor.active_id() == Some(self.target.document)
            && self
                .preview
                .as_ref()
                .is_none_or(|token| token.owned(editor))
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.dialog.edited();
        let result = (|| {
            let patch = self.patch(cx)?;
            let changed = !patch.changes.is_empty();
            let owner = self
                .owner
                .upgrade()
                .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
            owner.update(cx, |view, cx| {
                anyhow::ensure!(
                    self.current(&view.editor),
                    t!("properties.warp_preview_expired")
                );
                if changed || self.preview.is_some() {
                    if self.preview.is_none() {
                        self.preview = Some(properties::begin_edit(
                            &mut view.editor,
                            &self.target,
                            &patch,
                        )?);
                        if let Some(PreviewToken::Object(token)) = &self.preview {
                            view.dialog_property_preview = Some(token.clone());
                        }
                    }
                    properties::update_edit(
                        &mut view.editor,
                        self.preview.as_ref().unwrap(),
                        &patch,
                    )?;
                    self.metrics = Some(
                        view.editor
                            .document()
                            .unwrap()
                            .warp_density_preview_metrics(&self.target.object, self.bezier)?,
                    );
                    view.redraw(cx);
                }
                Ok::<_, anyhow::Error>(())
            })?;
            Ok::<_, anyhow::Error>(changed)
        })();
        match result {
            Ok(changed) => self.dialog.set_validation(true, changed),
            Err(error) => {
                self.dialog.error = Some(error.to_string());
                self.dialog.set_validation(false, true);
            }
        }
        cx.notify();
    }
}
impl FormDialogView for WarpGridDialog {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(error: &anyhow::Error) -> String {
        error.to_string()
    }
    fn cancel_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(token) = self.preview.take() {
            let _ = self.owner.update(cx, |view, cx| {
                if let PreviewToken::Object(object_token) = &token {
                    if view.dialog_property_preview.as_ref() == Some(object_token) {
                        view.dialog_property_preview = None;
                    }
                }
                token.cancel(&mut view.editor);
                view.redraw(cx);
            });
        }
    }
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.refresh(cx);
        anyhow::ensure!(
            self.dialog.error.is_none() && !self.patch(cx)?.changes.is_empty(),
            "Enter valid Warp settings"
        );
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        owner.update(cx, |view, cx| {
            anyhow::ensure!(
                self.current(&view.editor),
                t!("properties.warp_preview_expired")
            );
            self.preview
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Preview the Warp settings first"))?
                .finish(&mut view.editor)?;
            if let Some(PreviewToken::Object(token)) = &self.preview {
                if view.dialog_property_preview.as_ref() == Some(token) {
                    view.dialog_property_preview = None;
                }
            }
            self.preview = None;
            view.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })
    }
}
impl Render for WarpGridDialog {
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
        let mut body = div()
            .id("warp-grid-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .text_sm()
            .child(
                t!(if self.bezier {
                    "properties.warp_bezier_grid_hint"
                } else {
                    "properties.warp_grid_hint"
                })
                .into_owned(),
            );
        for (i, label) in (if self.bezier {
            [
                "properties.warp_bezier_columns",
                "properties.warp_bezier_rows",
            ]
        } else {
            ["properties.warp_columns", "properties.warp_rows"]
        })
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
                        div().w(px(120.)).child(
                            compact::input(Input::new(&self.inputs[i]))
                                .id(if i == 0 {
                                    "warp-grid-columns"
                                } else {
                                    "warp-grid-rows"
                                })
                                .aria_label(t!(label).into_owned()),
                        ),
                    ),
            );
        }
        if !self.bezier {
            body = body.child(
                div()
                    .child(t!("properties.warp_interpolation").into_owned())
                    .child(
                        compact::select(Select::new(&self.interpolation))
                            .id("warp-grid-interpolation"),
                    ),
            );
        }
        if let Some(metrics) = self.metrics {
            body = body.child(
                div().id("warp-grid-metrics").test_support().child(
                    t!(
                        "properties.warp_grid_metrics",
                        shapes = metrics.shapes,
                        samples = metrics.samples_per_shape,
                        error = format!("{:.6}", metrics.maximum_sample_error)
                    )
                    .into_owned(),
                ),
            );
            if let Some(error) = metrics.maximum_controller_error {
                body = body.child(
                    div().id("warp-bezier-metrics").test_support().child(
                        t!(
                            "properties.warp_bezier_grid_metrics",
                            shapes = metrics.controller_shapes,
                            error = format!("{error:.6}")
                        )
                        .into_owned(),
                    ),
                );
            }
        }
        if let Some(error) = &self.dialog.error {
            body = body.child(div().text_color(cx.theme().danger).child(error.clone()));
        }
        body
    }
}
