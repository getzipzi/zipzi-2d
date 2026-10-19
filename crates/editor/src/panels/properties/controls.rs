use super::*;
use crate::properties::{
    self, Choice, FieldEditor, PropertyTarget,
    form::{self as property_form, PropertyForm},
    target,
};
use crate::ui::slide_number_input::{
    SlideNumberChange, SlideNumberGesture, SlideNumberInput, SlideNumberOptions,
};
use editor_core::{PropertyKey, PropertyValue};
use gpui_kit::component::{
    IndexPath,
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
};

impl SelectItem for Choice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &String {
        &self.value
    }
}
#[derive(PartialEq, Eq)]
struct ChoiceKey {
    revision: u64,
    locale: String,
    value: String,
}

pub(super) struct SelectControl {
    pub(super) reference_scroll: UniformListScrollHandle,
    key: ChoiceKey,
    state: Entity<SelectState<SearchableVec<Choice>>>,
    choices: Vec<Choice>,
    _event: Subscription,
}

pub(super) struct FieldControl {
    pub input: Entity<InputState>,
    pub(super) displayed: String,
    pub(super) settled: u64,
    number: Option<Entity<SlideNumberInput>>,
    _events: Vec<Subscription>,
}

impl PropertiesPanel {
    #[cfg(test)]
    pub(crate) fn choice_labels(&self, id: PropertyKey) -> Vec<String> {
        self.selects[&id]
            .choices
            .iter()
            .map(|choice| choice.label.to_string())
            .collect()
    }

    pub(super) fn sync_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let next = target(&owner.read(cx).editor);
        let context = owner.read(cx).editor.editing_context_token();
        if self.target != next || self.context != context {
            if self.preview.is_none() {
                if let Some(previous) = self.target.clone() {
                    let inputs: Vec<_> = self
                        .controls
                        .iter()
                        .map(|(&id, control)| {
                            (
                                id,
                                control.input.read(cx).value().to_string(),
                                control.settled,
                                self.composing(id, window, cx),
                            )
                        })
                        .collect();
                    owner.update(cx, |state, cx| {
                        if let Some(form) = state.property_forms.entries.get_mut(&previous) {
                            for (id, raw, settled, composing) in inputs {
                                if composing {
                                    form.reload_field(id);
                                } else if form.fields[&id].revision == settled
                                    && form.fields[&id].raw != raw
                                {
                                    let spec = editor_core::property_specs(previous.object.kind)
                                        .iter()
                                        .find(|s| s.key == id)
                                        .unwrap();
                                    form.edit(spec, raw);
                                }
                            }
                        }
                        let pending: Vec<_> = state
                            .property_forms
                            .entries
                            .get(&previous)
                            .map(|form| {
                                form.fields
                                    .iter()
                                    .filter(|(_, f)| f.touched)
                                    .map(|(&id, _)| id)
                                    .collect()
                            })
                            .unwrap_or_default();
                        let changed = !pending.is_empty();
                        for id in pending {
                            if let Err(error) =
                                state
                                    .property_forms
                                    .commit(&mut state.editor, &previous, id)
                            {
                                state.set_edit_error(error.to_string(), cx);
                            }
                        }
                        if changed {
                            state.redraw(cx);
                        }
                    });
                }
            }
            self.cancel_preview(cx);
            self.controls.clear();
            self.selects.clear();
            self.colors.clear();
            self.epoch += 1;
            self.definition = next.as_ref().map_or_else(Default::default, |target| {
                properties::definition(target.object.kind)
            });
            self.target = next;
            self.context = context;
            self.snapshot = None;
        }
        if self
            .preview
            .as_ref()
            .is_some_and(|(_, token)| !owner.read(cx).editor.property_edit_owned(token))
        {
            self.cancel_preview(cx);
        }
        let Some(target) = &self.target else {
            self.form = PropertyForm::default();
            return;
        };
        let state = owner.read(cx);
        self.disabled = state.modal
            || state.editor.is_busy()
            || state.editor.exiting()
            || !(properties::provider(target.object.kind).editable_in)(target.context)
            || state.editor.document().is_none_or(|doc| {
                doc.ensure_editable(std::slice::from_ref(&target.object))
                    .is_err()
            });
        let snapshot = state
            .editor
            .document()
            .map(|doc| (target.clone(), doc.model_generation()));
        if snapshot == self.snapshot {
            if let Some(form) = state.property_forms.entries.get(target) {
                if form != &self.form {
                    self.form.clone_from(form);
                }
            } else {
                self.form = PropertyForm::default();
            }
            return;
        }
        let values = state
            .editor
            .document()
            .and_then(|doc| doc.property_values(&target.object).ok());
        if let Some(values) = values {
            self.form = owner.update(cx, |state, _| {
                state.property_forms.sync(target, values).clone()
            });
            self.snapshot = snapshot;
        } else {
            self.form = PropertyForm::default();
        }
    }

    pub(super) fn allowed(&self, epoch: u64, window: &mut Window, cx: &mut App) -> bool {
        self.visible
            && !self.disabled
            && self.epoch == epoch
            && !window.has_active_dialog(cx)
            && self.owner.upgrade().is_some_and(|owner| {
                let state = owner.read(cx);
                !state.modal
                    && !state.editor.is_busy()
                    && !state.editor.exiting()
                    && state.editor.editing_context_token() == self.context
                    && target(&state.editor) == self.target
                    && self.target.as_ref().is_some_and(|target| {
                        state.editor.document().is_some_and(|doc| {
                            doc.ensure_editable(std::slice::from_ref(&target.object))
                                .is_ok()
                        })
                    })
            })
    }

    pub(super) fn field_spec(&self, id: PropertyKey) -> properties::FieldSpec {
        self.definition.field(id).unwrap()
    }

    fn edit_text(&mut self, id: PropertyKey, raw: String, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let spec = self.field_spec(id).binding;
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |state, cx| {
                if let Some(form) = state.property_forms.entries.get_mut(&target) {
                    form.edit(&spec, raw);
                }
                cx.notify();
            });
        }
        cx.notify();
    }

    fn commit_field(&mut self, id: PropertyKey, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |state, cx| {
                match state.property_forms.commit(&mut state.editor, &target, id) {
                    Ok(()) => {
                        state.clear_edit_error(cx);
                        state.redraw(cx);
                    }
                    Err(error) => {
                        state.set_edit_error(error.to_string(), cx);
                    }
                }
            });
        }
        cx.notify();
    }

    pub(super) fn set_value(
        &mut self,
        id: PropertyKey,
        value: PropertyValue,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let spec = self.field_spec(id);
        if spec.binding.readonly {
            return;
        }
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |state, cx| {
                if let Some(form) = state.property_forms.entries.get_mut(&target) {
                    form.set_value(&spec.binding, value);
                }
                cx.notify();
            });
        }
        self.commit_field(id, cx);
        cx.notify();
    }

    pub(super) fn reload_field(&mut self, id: PropertyKey, cx: &mut Context<Self>) {
        self.cancel_preview(cx);
        let Some(target) = self.target.clone() else {
            return;
        };
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |state, cx| {
                if let (Some(values), Some(form)) = (
                    state
                        .editor
                        .document()
                        .and_then(|doc| doc.property_values(&target.object).ok()),
                    state.property_forms.entries.get_mut(&target),
                ) {
                    form.sync(target.object.kind, values);
                    form.reload_field(id);
                }
                state.clear_edit_error(cx);
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(super) fn begin_preview(&mut self, id: PropertyKey, cx: &mut Context<Self>) {
        self.cancel_preview(cx);
        let Some(target) = self.target.clone() else {
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let keyframed = self.field_spec(id).binding.keyframed;
        let result = owner.update(cx, |state, _| {
            let form = state
                .property_forms
                .entries
                .get(&target)
                .ok_or_else(|| anyhow::anyhow!("Form ended"))?;
            let mut patch = form.patch(target.object.kind, id)?;
            if keyframed {
                patch.scope = target.scope.clone();
            }
            patch.changes.insert(id, form.fields[&id].baseline.clone());
            let context = state
                .editor
                .editing_context_token()
                .ok_or_else(|| anyhow::anyhow!("Editing context changed"))?;
            state
                .editor
                .begin_property_edit(context, target.object.clone(), &patch)
        });
        match result {
            Ok(token) => self.preview = Some((id, token)),
            Err(error) => owner.update(cx, |state, cx| {
                if let Some(form) = state.property_forms.entries.get_mut(&target) {
                    form.reload_field(id);
                }
                state.set_edit_error(error.to_string(), cx);
            }),
        }
    }

    fn update_preview(&mut self, id: PropertyKey, value: f64, cx: &mut Context<Self>) {
        let field = self.field_spec(id);
        let value = value / field.scale;
        let raw = if field.binding.value_type == editor_core::PropertyType::Integer {
            (value as i64).to_string()
        } else {
            (value as f32).to_string()
        };
        if let Ok(value) = field.binding.parse(&raw) {
            self.update_preview_value(id, value, cx);
        }
    }

    pub(super) fn update_preview_value(
        &mut self,
        id: PropertyKey,
        value: PropertyValue,
        cx: &mut Context<Self>,
    ) {
        let Some((field, token)) = self.preview.clone() else {
            return;
        };
        if field != id {
            return;
        }
        let Some(target) = self.target.clone() else {
            return;
        };
        let spec = self.field_spec(id).binding;
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |state, cx| {
                let Some(form) = state.property_forms.entries.get_mut(&target) else {
                    return;
                };
                let previous = form.fields[&id].clone();
                form.set_value(&spec, value);
                let result = form.patch(target.object.kind, id).and_then(|mut patch| {
                    if spec.keyframed {
                        patch.scope = target.scope.clone();
                    }
                    state.editor.update_property_edit(&token, &patch)
                });
                if let Err(error) = result {
                    form.fields.insert(id, previous);
                    state.set_edit_error(error.to_string(), cx);
                }
                state.redraw(cx);
            });
        }
        cx.notify();
    }

    pub(super) fn finish_preview(&mut self, cx: &mut Context<Self>) {
        let Some((id, token)) = self.preview.take() else {
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(target) = self.target.clone() else {
            return;
        };
        owner.update(cx, |state, cx| {
            let result = state.editor.finish_property_edit(&token);
            if let (Some(values), Some(form)) = (
                state
                    .editor
                    .document()
                    .and_then(|doc| doc.property_values(&target.object).ok()),
                state.property_forms.entries.get_mut(&target),
            ) {
                form.sync(target.object.kind, values);
                form.reload_field(id);
            }
            if let Err(error) = result {
                state.set_edit_error(error.to_string(), cx);
            }
            state.redraw(cx);
        });
        cx.notify();
    }

    pub(super) fn cancel_preview(&mut self, cx: &mut Context<Self>) {
        let Some((id, token)) = self.preview.take() else {
            return;
        };
        if let Some(color) = self.colors.get_mut(&id) {
            color
                .state
                .update(cx, |state, cx| state.cancel_silently(cx));
        }
        if let Some(control) = self.controls.get(&id).and_then(|c| c.number.clone()) {
            control.update(cx, |number, cx| number.reset_drag(cx));
        }
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |state, cx| {
                state.editor.cancel_property_edit(&token);
                if let Some(target) = &self.target {
                    if let Some(form) = state.property_forms.entries.get_mut(target) {
                        form.reload_field(id);
                    }
                }
                state.redraw(cx);
            });
        }
    }

    pub(crate) fn take_preview(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(PropertyTarget, PropertyKey, editor_core::PropertyEditToken)> {
        let (id, token) = self.preview.take()?;
        if let Some(color) = self.colors.get(&id) {
            color
                .state
                .update(cx, |state, cx| state.cancel_silently(cx));
        }
        if let Some(number) = self.controls.get(&id).and_then(|c| c.number.clone()) {
            number.update(cx, |number, cx| number.reset_drag(cx));
        }
        self.target.clone().map(|target| (target, id, token))
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// Return domain cleanup to the parent; never update EditorView while the
    /// parent's render holds its entity borrow.
    pub(crate) fn set_visible(
        &mut self,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Option<(PropertyTarget, PropertyKey, editor_core::PropertyEditToken)> {
        if self.visible == visible {
            return None;
        }
        self.visible = visible;
        if visible {
            return None;
        }
        let preview = self.preview.take();
        if let Some((id, _)) = &preview {
            if let Some(number) = self.controls.get(id).and_then(|c| c.number.clone()) {
                number.update(cx, |number, cx| number.reset_drag(cx));
            }
        }
        self.controls.clear();
        self.selects.clear();
        self.colors.clear();
        self.epoch += 1;
        self.snapshot = None;
        preview.and_then(|(id, token)| self.target.clone().map(|target| (target, id, token)))
    }

    pub(super) fn ensure_control(
        &mut self,
        id: PropertyKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.controls.contains_key(&id) {
            return;
        }
        let spec = self.field_spec(id);
        let epoch = self.epoch;
        let input = cx
            .new(|cx| InputState::new(window, cx).default_value(self.form.fields[&id].raw.clone()));
        let text = cx.subscribe_in(
            &input,
            window,
            move |this, source, event: &InputEvent, window, cx| {
                if !this.allowed(epoch, window, cx)
                    || this
                        .controls
                        .get(&id)
                        .is_none_or(|control| &control.input != source)
                {
                    return;
                }
                match event {
                    InputEvent::Change if source.read(cx).focus_handle(cx).is_focused(window) => {
                        let raw = source.read(cx).value().to_string();
                        let matches_draft = this.owner.upgrade().is_some_and(|owner| {
                            owner
                                .read(cx)
                                .property_forms
                                .entries
                                .get(this.target.as_ref().unwrap())
                                .is_some_and(|form| form.fields[&id].raw == raw)
                        });
                        if !matches_draft {
                            this.edit_text(id, raw, cx);
                        }
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        if !this.composing(id, window, cx) {
                            let raw = source.read(cx).value().to_string();
                            if this.form.fields[&id].touched || this.form.fields[&id].raw != raw {
                                this.edit_text(id, raw, cx);
                                this.commit_field(id, cx);
                            }
                        }
                    }
                    _ => {}
                }
            },
        );
        let mut events = vec![text];
        let number = (spec.binding.preview
            && matches!(
                spec.editor,
                FieldEditor::Number | FieldEditor::OptionalNumber
            ))
        .then(|| {
            let number = cx.new(|cx| {
                let mut control = SlideNumberInput::new(
                    input.clone(),
                    t!(spec.label).into_owned(),
                    self.number_options(id),
                    window,
                    cx,
                );
                if spec.scale == 100. {
                    control = control.suffix("%");
                }
                if spec.binding.value_type == editor_core::PropertyType::Integer {
                    control.truncate_to_integer()
                } else {
                    control
                }
            });
            events.push(cx.subscribe_in(
                &number,
                window,
                move |this, source, event: &SlideNumberGesture, window, cx| {
                    if this.epoch != epoch
                        || this.controls.get(&id).and_then(|c| c.number.as_ref()) != Some(source)
                    {
                        return;
                    }
                    if !this.allowed(epoch, window, cx) {
                        this.cancel_preview(cx);
                        return;
                    }
                    match event {
                        SlideNumberGesture::Begin => this.begin_preview(id, cx),
                        SlideNumberGesture::End => this.finish_preview(cx),
                        SlideNumberGesture::Cancel => this.cancel_preview(cx),
                    }
                },
            ));
            events.push(cx.subscribe_in(
                &number,
                window,
                move |this, _, event: &SlideNumberChange, window, cx| {
                    if this.allowed(epoch, window, cx) {
                        this.update_preview(id, event.0, cx);
                    }
                },
            ));
            number
        });
        self.controls.insert(
            id,
            FieldControl {
                input,
                displayed: self.form.fields[&id].raw.clone(),
                settled: self.form.fields[&id].revision,
                number,
                _events: events,
            },
        );
    }

    pub(super) fn render_select(
        &mut self,
        id: PropertyKey,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let spec = self.field_spec(id);
        let references = spec.editor == FieldEditor::References;
        let draft = &self.form.fields[&id];
        let selected = match &draft.parsed {
            Ok(PropertyValue::References(ids)) => ids.clone(),
            _ => vec![],
        };
        let value = if references {
            String::new()
        } else {
            draft.raw.clone()
        };
        let desired = (!references).then_some(&value);
        let key = ChoiceKey {
            revision: self
                .owner
                .upgrade()
                .and_then(|owner| {
                    owner
                        .read(cx)
                        .editor
                        .document()
                        .map(|doc| doc.model_generation())
                })
                .unwrap_or_default(),
            locale: rust_i18n::locale().to_string(),
            value: draft.raw.clone(),
        };
        if self
            .selects
            .get(&id)
            .is_none_or(|control| control.key != key)
        {
            #[cfg(test)]
            {
                self.choice_builds += 1;
            }
            let mut choices = self
                .owner
                .upgrade()
                .and_then(|owner| {
                    owner.read(cx).editor.document().map(|document| {
                        spec.choices.expect("Select requires a choices provider")(document)
                    })
                })
                .unwrap_or_default();

            // Retain imported unknown values until the user explicitly replaces them.
            if references {
                choices.retain(|choice| !selected.contains(&choice.value));
            }
            if !references && !choices.iter().any(|choice| choice.value == value) {
                choices.push(Choice {
                    label: value.clone().into(),
                    value: value.clone(),
                });
            }
            if !self.selects.contains_key(&id) {
                let selected = choices
                    .iter()
                    .position(|choice| choice.value == value)
                    .map(IndexPath::new);
                let state = cx.new(|cx| {
                    SelectState::new(SearchableVec::new(choices.clone()), selected, window, cx)
                        .searchable(spec.searchable)
                });
                let epoch = self.epoch;
                let event = cx.subscribe_in(
                    &state,
                    window,
                    move |this, source, event: &SelectEvent<SearchableVec<Choice>>, window, cx| {
                        if !this.allowed(epoch, window, cx)
                            || this
                                .selects
                                .get(&id)
                                .is_none_or(|control| &control.state != source)
                        {
                            return;
                        }
                        if let SelectEvent::Confirm(Some(value)) = event {
                            if references {
                                let mut ids = match &this.form.fields[&id].parsed {
                                    Ok(PropertyValue::References(ids)) => ids.clone(),
                                    _ => vec![],
                                };
                                if !ids.contains(value) {
                                    ids.push(value.clone());
                                }
                                this.set_value(id, PropertyValue::References(ids), cx);
                            } else {
                                this.set_value(id, PropertyValue::Text(value.clone()), cx);
                            }
                        }
                    },
                );
                self.selects.insert(
                    id,
                    SelectControl {
                        reference_scroll: UniformListScrollHandle::new(),
                        key: ChoiceKey {
                            revision: key.revision,
                            locale: key.locale.clone(),
                            value: key.value.clone(),
                        },
                        state,
                        choices: choices.clone(),
                        _event: event,
                    },
                );
            }
            let control = self.selects.get_mut(&id).unwrap();
            if control.choices != choices {
                control.state.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(choices.clone()), window, cx);
                    state.set_selected_value(&value, window, cx);
                });
                control.choices = choices;
            } else if control.state.read(cx).selected_value() != desired {
                control
                    .state
                    .update(cx, |state, cx| state.set_selected_value(&value, window, cx));
            }
            control.key = key;
        }
        let control = &self.selects[&id];
        // A rejected selection can leave the widget ahead of the restored form,
        // even when the cached choices and committed value did not change.
        if control.state.read(cx).selected_value() != desired {
            control
                .state
                .update(cx, |state, cx| state.set_selected_value(&value, window, cx));
        }
        // Select's root fills its parent; constrain both the root and trigger.
        div()
            .w_full()
            .h(compact::rem(compact::NUMBER_CONTROL))
            .child(
                compact::select(Select::new(&control.state))
                    .h(compact::rem(compact::NUMBER_CONTROL))
                    .py(compact::rem(0.))
                    .line_height(compact::rem(14.))
                    .id(SharedString::from(format!("property-select-{}", id.name())))
                    .accessibility_label(t!(self.field_spec(id).label))
                    .disabled(disabled)
                    .when(references, |select| {
                        select.placeholder(t!("properties.add_mask"))
                    })
                    .w_full(),
            )
            .into_any_element()
    }

    fn number_options(&self, id: PropertyKey) -> SlideNumberOptions {
        let field = self.field_spec(id);
        let spec = field.binding;
        let (min, max) = self
            .field_spec(id)
            .number_bounds(self.form.committed_values());
        SlideNumberOptions {
            value: self.form.fields[&id]
                .parsed
                .as_ref()
                .ok()
                .and_then(|value| value.text().parse::<f64>().ok())
                .unwrap_or(0.)
                * field.scale,
            min: min * field.scale,
            max: max * field.scale,
            step: spec.step * field.scale,
            drag_distance: None,
            disabled: self.disabled,
        }
    }

    pub(super) fn render_field(
        &mut self,
        id: PropertyKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(field) = self.form.fields.get(&id).cloned() else {
            return div().into_any_element();
        };
        let spec = self.field_spec(id);
        let epoch = self.epoch;
        let unavailable = spec.binding.keyframed
            && self
                .target
                .as_ref()
                .is_some_and(|target| target.scope == editor_core::PropertyScope::InterpolatedMesh);
        let disabled =
            self.disabled || spec.binding.readonly || unavailable || window.has_active_dialog(cx);
        let body = if unavailable {
            div()
                .text_color(cx.theme().muted_foreground)
                .child(t!("properties.select_keyform"))
                .into_any_element()
        } else if spec.editor == FieldEditor::Color {
            self.render_color(id, disabled, window, cx)
        } else if spec.editor == FieldEditor::References {
            self.render_references(id, disabled, window, cx)
        } else if spec.editor == FieldEditor::Toggle {
            let checked = matches!(field.parsed, Ok(PropertyValue::Bool(true)));
            Checkbox::new(SharedString::from(format!("property-toggle-{}", id.name())))
                .xsmall()
                .checked(checked)
                .disabled(disabled)
                .on_click(cx.listener(move |this, checked, window, cx| {
                    if this.allowed(epoch, window, cx) {
                        this.set_value(id, PropertyValue::Bool(*checked), cx);
                    }
                }))
                .into_any_element()
        } else if spec.editor == FieldEditor::Select {
            self.render_select(id, disabled, window, cx)
        } else {
            self.ensure_control(id, window, cx);
            let options = self.number_options(id);
            let control = self.controls.get_mut(&id).unwrap();
            let input = control.input.clone();
            if control.displayed != field.raw || control.settled != field.revision {
                control.settled = field.revision;
                control.displayed = field.raw.clone();
                if input.read(cx).value().as_ref() != field.raw {
                    input.update(cx, |input, cx| {
                        input.set_value(field.raw.clone(), window, cx)
                    });
                }
            }
            let number = control.number.clone();
            let mut body = div()
                .flex()
                .items_center()
                .w_full()
                .min_w_0()
                .gap(compact::rem(compact::GAP));
            if spec.editor == FieldEditor::OptionalNumber {
                let custom = self.form.numeric_visible(id);
                body = body.child(
                    Checkbox::new(SharedString::from(format!("property-custom-{}", id.name())))
                        .label(t!("properties.custom").into_owned())
                        .xsmall()
                        .checked(custom)
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, custom: &bool, window, cx| {
                            if this.allowed(epoch, window, cx) {
                                this.set_value(
                                    id,
                                    PropertyValue::OptionalFloat(custom.then_some(0.)),
                                    cx,
                                );
                            }
                        })),
                );
            }
            if self.form.numeric_visible(id) {
                let numeric = matches!(
                    spec.editor,
                    FieldEditor::Number | FieldEditor::OptionalNumber
                );
                let editor = if let Some(number) = &number {
                    number.update(cx, |number, cx| {
                        number.configure(t!(spec.label).into_owned(), options, cx)
                    });
                    number.clone().into_any_element()
                } else {
                    compact::input(Input::new(&input))
                        .map(|input| Styled::h(input, compact::rem(compact::NUMBER_CONTROL)))
                        .line_height(compact::rem(14.))
                        .disabled(disabled)
                        .aria_label(t!(spec.label))
                        .when(numeric, |input| {
                            input
                                .text_align(TextAlign::Right)
                                .font_family(".AppleSystemUIFontMonospaced")
                        })
                        .when(spec.scale == 100., |input| input.suffix("%"))
                        .into_any_element()
                };
                body = body.child(
                    div()
                        .id(SharedString::from(format!("property-input-{}", id.name())))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .w_full()
                        .child(editor),
                );
            }
            body.into_any_element()
        };
        let label = t!(spec.label).into_owned();
        compact::property_row(label, body, cx)
            .id(SharedString::from(format!("object-property-{}", id.name())))
            .test_support()
            .into_any_element()
    }

    pub(super) fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.allowed(self.epoch, window, cx) {
            return false;
        }
        if self.composing_input(window, cx) {
            return false;
        }
        if self.preview.is_some() {
            self.cancel_preview(cx);
            return true;
        }
        if let Some(color) = self
            .colors
            .values()
            .find(|color| color.state.read(cx).owns_focus(window, cx))
            .map(|color| color.state.clone())
        {
            color.update(cx, |state, cx| state.cancel(cx));
            return true;
        }
        let Some((&id, _)) = self
            .controls
            .iter()
            .find(|(_, control)| control.input.read(cx).focus_handle(cx).is_focused(window))
        else {
            return false;
        };
        if self.composing(id, window, cx) {
            return false;
        }
        self.reload_field(id, cx);
        window.focus(&self.focus, cx);
        true
    }

    pub(crate) fn active_property_edit(
        &self,
        forms: &property_form::PropertyForms,
        window: &Window,
        cx: &App,
    ) -> bool {
        if !self.visible || self.disabled {
            return false;
        }
        if self.preview.is_some() {
            return true;
        }
        let Some(target) = &self.target else {
            return false;
        };
        let Some((&id, control)) = self
            .controls
            .iter()
            .find(|(_, control)| control.input.read(cx).focus_handle(cx).is_focused(window))
        else {
            return false;
        };
        let live_raw = control.input.read(cx).value();
        forms
            .entries
            .get(target)
            .is_some_and(|form| form.field_active(id, live_raw.as_ref()))
    }

    fn composing(&self, id: PropertyKey, window: &mut Window, cx: &mut App) -> bool {
        use gpui_kit::EntityInputHandler;
        self.controls[&id].input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        })
    }

    pub(crate) fn composing_input(&self, window: &mut Window, cx: &mut App) -> bool {
        self.colors.values().any(|control| {
            control
                .state
                .update(cx, |state, cx| state.composing(window, cx))
        }) || self
            .controls
            .iter()
            .find(|(_, control)| control.input.read(cx).focus_handle(cx).is_focused(window))
            .map(|(&id, _)| id)
            .is_some_and(|id| self.composing(id, window, cx))
    }

    pub(crate) fn focused_input_snapshot(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(PropertyTarget, PropertyKey, String)> {
        if !self.visible {
            return None;
        }
        let (&id, control) = self
            .controls
            .iter()
            .find(|(_, control)| control.input.read(cx).focus_handle(cx).is_focused(window))?;
        Some((
            self.target.clone()?,
            id,
            control.input.read(cx).value().to_string(),
        ))
    }

    pub(super) fn property_action(
        &mut self,
        action: editor_core::CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use editor_core::CommandId::{CancelProperty, CommitProperty};
        if !matches!(action, CommitProperty | CancelProperty)
            || !self.allowed(self.epoch, window, cx)
        {
            return false;
        }
        if action == CancelProperty {
            return self.escape(window, cx);
        }
        if let Some(color) = self
            .colors
            .values()
            .find(|color| color.state.read(cx).owns_focus(window, cx))
            .map(|color| color.state.clone())
        {
            color.update(cx, |state, cx| state.accept(window, cx));
            return true;
        }
        if self.preview.is_some() {
            if let Some(color) = self
                .preview
                .as_ref()
                .and_then(|(id, _)| self.colors.get(id))
                .map(|control| control.state.clone())
            {
                color.update(cx, |state, cx| state.accept(window, cx));
                return true;
            }
            self.finish_preview(cx);
            return true;
        }
        let Some((&id, _)) = self
            .controls
            .iter()
            .find(|(_, control)| control.input.read(cx).focus_handle(cx).is_focused(window))
        else {
            return false;
        };
        if self.composing(id, window, cx) {
            return true;
        }
        let raw = self.controls[&id].input.read(cx).value().to_string();
        self.edit_text(id, raw, cx);
        self.commit_field(id, cx);
        true
    }
}
