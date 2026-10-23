//! Shared UI boundary for object operations, selection, Save and history.
use crate::*;
use editor_core::DocumentId;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputBoundary {
    CommitText,
    Discard,
}

impl EditorView {
    pub(crate) fn settle_property_inputs(
        &mut self,
        id: DocumentId,
        boundary: InputBoundary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let before = self.editor.document().map(|document| {
            (
                document.model_generation(),
                document.preview_revision(),
                document.properties_revision(),
            )
        });
        if self.editor.active_id() == Some(id)
            && self.dock.as_ref().is_some_and(|dock| {
                dock.properties
                    .update(cx, |panel, cx| panel.composing_input(window, cx))
            })
        {
            self.set_edit_error(t!("properties.finish_composition").into_owned(), cx);
            self.editor.cancel_exit();
            return false;
        }
        if boundary == InputBoundary::CommitText
            && let Some((target, field, raw)) = self
                .dock
                .as_ref()
                .and_then(|dock| dock.properties.read(cx).focused_input_snapshot(window, cx))
        {
            if target.document == id && self.editor.editing_context() == Some(target.context) {
                if let Some(form) = self.property_forms.entries.get_mut(&target) {
                    let spec = target
                        .definition(&self.editor)
                        .fields
                        .iter()
                        .map(|field| field.binding)
                        .find(|spec| spec.key == field)
                        .unwrap();
                    if form
                        .fields
                        .get(&field)
                        .is_some_and(|value| value.raw != raw)
                    {
                        form.edit(&spec, raw);
                    }
                }
            }
        }
        if let Some((target, field, token, text)) = self
            .dock
            .as_ref()
            .filter(|_| self.editor.active_id() == Some(id))
            .and_then(|dock| {
                dock.properties
                    .update(cx, |panel, cx| panel.take_preview(window, cx))
            })
        {
            let result = (|| {
                if boundary == InputBoundary::Discard || !text {
                    token.cancel(&mut self.editor);
                    return Ok(());
                }
                let form = self
                    .property_forms
                    .entries
                    .get(&target)
                    .ok_or_else(|| anyhow::anyhow!("Form ended"))?;
                let mut patch = form.patch(target.object.kind, field)?;
                if patch.changes.is_empty() {
                    token.cancel(&mut self.editor);
                    return Ok(());
                }
                if target
                    .definition(&self.editor)
                    .field(field)
                    .is_some_and(|spec| spec.binding.keyframed)
                {
                    patch.scope = target.scope.clone();
                }
                properties::update_edit(&mut self.editor, &token, &patch)?;
                token.finish(&mut self.editor)
            })();
            if let Err(error) = result {
                token.cancel(&mut self.editor);
                self.set_edit_error(error.to_string(), cx);
            }
            if let Some(form) = self.property_forms.entries.get_mut(&target) {
                if let Some(values) = target.values(&self.editor).ok() {
                    form.sync_definition(target.definition(&self.editor), values);
                    form.reload_field(field);
                }
            }
        }
        // Release pointer owners before the next operation. This does not pause
        // ordinary playback or advance the editing context for presentation edits.
        if self.editor.active_id() == Some(id) {
            self.cancel_input_captures(window, cx);
            let pending = self
                .editor
                .document()
                .is_some_and(|doc| doc.has_pending_edit())
                || self.editor.timeline_edit().is_some()
                || self.editor.gesture_active();
            if pending && let Err(error) = self.editor.execute_for(id, EditorCommand::CancelEdit) {
                self.set_edit_error(error.to_string(), cx);
                return false;
            }
        }
        if boundary == InputBoundary::CommitText {
            if let Some(error) = self.property_forms.settle_document(&mut self.editor, id) {
                self.set_edit_error(error, cx);
            }
        } else {
            self.property_forms.discard_document(&self.editor, id);
        }
        if let Some(panel) = self.dock.as_ref().map(|dock| dock.properties.clone()) {
            let target = panel
                .read(cx)
                .current_target()
                .filter(|target| target.document == id)
                .cloned();
            if let Some(form) = target
                .and_then(|target| self.property_forms.entries.get(&target))
                .cloned()
            {
                panel.update(cx, |panel, cx| panel.settled_inputs(form, window, cx));
            }
        }
        let after = self.editor.document().map(|document| {
            (
                document.model_generation(),
                document.preview_revision(),
                document.properties_revision(),
            )
        });
        if before != after {
            self.redraw(cx);
        } else {
            cx.notify();
        }
        true
    }

    pub(super) fn prepare_property_inputs(
        &mut self,
        id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.settle_property_inputs(id, InputBoundary::CommitText, window, cx)
    }
}
