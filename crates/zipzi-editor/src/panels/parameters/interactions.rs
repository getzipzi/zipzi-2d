//! Slider gestures, numeric input and parameter focus commands.
use super::*;

impl ParametersPanel {
    pub(super) fn reset_defaults(
        &mut self,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) || !self.slider_interaction_allowed(window, cx) {
            return;
        }
        self.finish_slider_drag(cx);
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        owner.update(cx, |owner, cx| {
            if !owner.prepare_property_inputs(document_id, window, cx) {
                return;
            }
            let Some(document) = owner.editor.document() else {
                return;
            };
            // Reset the whole document, including collapsed and filtered rows.
            let values = document
                .parameters()
                .into_iter()
                .map(|parameter| (parameter.id, parameter.default_value))
                .collect();
            let result = owner
                .editor
                .execute_for(document_id, EditorCommand::SetPreviewParameters { values });
            owner.complete_preview_edit(result, cx);
        });
        self.sync_with_editor(window, cx);
        cx.notify();
    }

    pub(super) fn select_nearest_direct_influence(
        &mut self,
        document_id: DocumentId,
        parameter_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) || !self.slider_interaction_allowed(window, cx) {
            return;
        }
        self.finish_slider_drag(cx);
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        owner.update(cx, |owner, cx| {
            if !owner.prepare_property_inputs(document_id, window, cx) {
                return;
            }
            let Some(document) = owner.editor.document() else {
                return;
            };
            let index = document.object_index();
            let context = document.context();
            // Paths start at the parameter and its binding, followed by the
            // directly bound ancestor and its descendants. Prefer the primary
            // selection's shortest path when several selected objects qualify.
            let target = index
                .parameter(parameter_id)
                .indirect
                .iter()
                .filter(|path| context.selections.contains(&path.target))
                .min_by_key(|path| {
                    (
                        context.selection.as_ref() != Some(&path.target),
                        path.path.len(),
                    )
                })
                .and_then(|path| {
                    path.path
                        .iter()
                        .find_map(|reference| index.binding(reference))
                })
                .map(|binding| binding.target.clone());
            if let Some(target) = target {
                let result = owner
                    .editor
                    .execute_for(document_id, EditorCommand::SelectObject(target));
                owner.complete_edit(result, cx);
            }
        });
    }

    pub(super) fn edit_keyforms(
        &mut self,
        document_id: DocumentId,
        parameter_id: &str,
        action: zipzi_editor_core::ParameterKeyformAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) || !self.slider_interaction_allowed(window, cx) {
            return;
        }
        self.finish_slider_drag(cx);
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        owner.update(cx, |owner, cx| {
            if !owner.prepare_property_inputs(document_id, window, cx) {
                return;
            }
            let result = owner.editor.execute_for(
                document_id,
                EditorCommand::EditSelectedParameterKeyforms {
                    parameter_id: parameter_id.to_owned(),
                    action,
                },
            );
            owner.complete_edit(result, cx);
        });
    }

    pub(super) fn begin_slider_drag(
        &mut self,
        document_id: DocumentId,
        slider: &Entity<SliderState>,
        position: Point<Pixels>,
        move_thumb: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id)
            || !self.rows.iter().any(|row| &row.slider == slider)
            || !self.slider_interaction_allowed(window, cx)
            || slider.read(cx).bounds().size.width <= px(0.)
        {
            return;
        }
        self.finish_slider_drag(cx);
        if let Some(parameter) = self
            .rows
            .iter()
            .find(|r| &r.slider == slider)
            .map(|r| r.parameter.id.clone())
        {
            if let Some(owner) = self.owner.upgrade() {
                owner.update(cx, |owner, cx| {
                    let result = owner
                        .editor
                        .execute_for(document_id, EditorCommand::FocusParameter(parameter));
                    owner.complete_preview_edit(result, cx);
                });
            }
        }
        self.slider_drag = Some(slider.clone());
        if move_thumb {
            slider.update(cx, |slider, cx| {
                slider.update_value_by_position(Axis::Horizontal, position, false, window, cx);
            });
        }
    }

    pub(super) fn slider_interaction_allowed(&self, window: &mut Window, cx: &mut App) -> bool {
        if window.has_active_dialog(cx) {
            return false;
        }
        self.owner.upgrade().is_some_and(|owner| {
            let owner = owner.read(cx);
            !owner.modal && !owner.editor.is_busy() && owner.editor.active_id() == self.document_id
        })
    }

    pub(super) fn move_slider_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if crate::benchmark::ignore_native_pointer_event() {
            return true;
        }
        let Some(slider) = self.slider_drag.clone() else {
            return false;
        };
        if event.pressed_button != Some(MouseButton::Left)
            || !self.slider_interaction_allowed(window, cx)
        {
            self.finish_slider_drag(cx);
            return false;
        }
        slider.update(cx, |slider, cx| {
            slider.update_value_by_position(Axis::Horizontal, event.position, false, window, cx);
        });
        true
    }

    pub(super) fn finish_slider_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(slider) = self.slider_drag.take() else {
            return false;
        };
        slider.update(cx, |slider, cx| slider.handle_release(cx));
        true
    }

    pub(super) fn cancel_input(
        &mut self,
        document_id: DocumentId,
        parameter_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) {
            return;
        }
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|r| r.parameter.id == parameter_id)
        {
            row.input_dirty = false;
            row.input.update(cx, |input, cx| {
                input.set_value(format_value(row.parameter.value), window, cx)
            });
        }
    }

    pub(super) fn commit_input(
        &mut self,
        document_id: DocumentId,
        parameter_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) {
            return;
        }
        let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.parameter.id == parameter_id)
        else {
            return;
        };
        // Focusing and blurring a truncated display must not quantize the preview.
        if !row.input_dirty {
            return;
        }
        let value = row.input.read(cx).value().trim().parse::<f32>().ok();
        let valid = value.filter(|value| {
            value.is_finite() && (row.parameter.minimum..=row.parameter.maximum).contains(value)
        });
        let original = row.parameter.value;
        let input = row.input.clone();
        if let Some(value) = valid.filter(|value| *value != original) {
            self.apply_values(document_id, &[(parameter_id.to_owned(), value)], window, cx);
        }
        let displayed = self
            .rows
            .iter()
            .find(|row| row.parameter.id == parameter_id)
            .map_or(original, |row| row.parameter.value);
        input.update(cx, |input, cx| {
            input.set_value(format_value(displayed), window, cx);
        });
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|r| r.parameter.id == parameter_id)
        {
            row.input_dirty = false;
        }
    }

    pub(super) fn apply_values(
        &mut self,
        document_id: DocumentId,
        values: &[(String, f32)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document_id) {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let changed = values.iter().any(|(id, value)| {
            self.rows
                .iter()
                .any(|r| &r.parameter.id == id && r.parameter.value != *value)
        });
        let applied = owner.update(cx, |owner, cx| {
            if owner.modal
                || owner.editor.is_busy()
                || owner.editor.active_id() != Some(document_id)
            {
                return false;
            }
            if values.len() == 1
                && owner.editor.document().is_some_and(|d| {
                    d.context()
                        .parameter_focus
                        .as_ref()
                        .is_none_or(|p| p.id != values[0].0)
                })
            {
                let result = owner.editor.execute_for(
                    document_id,
                    EditorCommand::FocusParameter(values[0].0.clone()),
                );
                if result.is_err() {
                    owner.complete_preview_edit(result, cx);
                    return false;
                }
            }
            if !changed {
                return true;
            }
            let result = owner.editor.execute_for(
                document_id,
                EditorCommand::SetPreviewParameters {
                    values: values.to_vec(),
                },
            );
            let applied = result.is_ok();
            owner.complete_preview_edit(result, cx);
            applied
        });
        if applied {
            for (id, value) in values {
                if let Some(row) = self.rows.iter_mut().find(|row| row.parameter.id == *id) {
                    row.parameter.value = *value;
                    let view = row.view.clone();
                    let applied_value = *value;
                    cx.defer(move |cx| {
                        view.update(cx, |view, cx| {
                            view.parameter.value = applied_value;
                            cx.notify();
                        })
                    });
                    if !row.input_dirty {
                        row.input.update(cx, |input, cx| {
                            input.set_value(format_value(*value), window, cx)
                        });
                    }
                    row.slider.update(cx, |slider, cx| {
                        slider.set_value(*value, window, cx);
                    });
                }
            }
        }
    }

    pub(super) fn focus_parameter(
        &mut self,
        document_id: DocumentId,
        parameter: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_row(
            document_id,
            ParameterCursor::Object(zipzi_editor_core::ObjectRef::new(
                zipzi_editor_core::ObjectKind::Parameter,
                parameter,
            )),
            window,
            cx,
        );
    }
}
