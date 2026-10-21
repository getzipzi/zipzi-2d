use super::*;
use crate::{
    properties::Choice,
    ui::slide_number_input::{
        SlideNumberChange, SlideNumberGesture, SlideNumberInput, SlideNumberOptions,
        format_one_decimal,
    },
};
use gpui_kit::component::{
    IndexPath,
    select::{Select, SelectEvent},
};

pub(super) struct NumberControl {
    input: Entity<InputState>,
    number: Entity<SlideNumberInput>,
    _events: Vec<Subscription>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SelectField {
    Reuse,
}
pub(super) struct SelectControl {
    state: Entity<SelectState<SearchableVec<Choice>>>,
    choices: Vec<Choice>,
    _event: Subscription,
}

impl PropertiesPanel {
    pub(super) fn physics_number(
        &mut self,
        field: NumberField,
        label: String,
        value: f32,
        range: (f32, f32, f32),
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control =
            self.physics_number_control(field, label.clone(), value, range, disabled, window, cx);
        compact::inline_property_row(label, control, cx).into_any_element()
    }

    pub(super) fn physics_number_control(
        &mut self,
        field: NumberField,
        label: String,
        value: f32,
        range: (f32, f32, f32),
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(spec) = field.spec() {
            let id = spec.key;
            return div()
                .id(SharedString::from(format!("physics-number-{field:?}")))
                .test_support()
                .min_w_0()
                .w_full()
                .child(self.render_field_control(id, window, cx))
                .into_any_element();
        }
        let decimal = |value: f32| value.to_string().parse::<f64>().unwrap_or(value as f64);
        let options = SlideNumberOptions {
            value: decimal(value),
            min: decimal(range.0.min(value)),
            max: decimal(range.1.max(value)),
            step: 0.1,
            drag_distance: Some(400.),
            disabled,
        };
        if !self.physics.numbers.contains_key(&field) {
            let input = cx.new(|cx| {
                InputState::new(window, cx).default_value(format_one_decimal(value as f64))
            });
            let number = cx.new(|cx| {
                SlideNumberInput::new(input.clone(), label.clone(), options, window, cx)
                    .format_sliding_value(format_one_decimal)
            });
            let epoch = self.physics.epoch;
            let key = field.clone();
            let text = cx.subscribe_in(
                &input,
                window,
                move |this, source, event: &InputEvent, window, cx| {
                    if !this.physics_allowed(epoch, window, cx) {
                        return;
                    }
                    let change = matches!(event, InputEvent::Change)
                        && source.read(cx).focus_handle(cx).is_focused(window);
                    let finish = matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur);
                    if !(change || finish)
                        || this
                            .physics
                            .numbers
                            .get(&key)
                            .is_some_and(|c| c.number.read(cx).is_sliding())
                    {
                        return;
                    }
                    let raw = source.read(cx).value().to_string();
                    match raw.trim().parse::<f32>() {
                        Ok(value) if value.is_finite() => {
                            if matches!(key, NumberField::Manual(_)) {
                                this.physics_numeric_change(key.clone(), value, false, cx);
                            }
                        }
                        _ => {
                            this.physics_cancel(cx);
                            this.physics.error = Some(t!("physics.invalid_number").into_owned());
                            cx.notify();
                        }
                    }
                },
            );
            let gesture = cx.subscribe_in(
                &number,
                window,
                move |this, _, event: &SlideNumberGesture, window, cx| {
                    if !this.physics_allowed(epoch, window, cx) {
                        this.physics_cancel(cx);
                        return;
                    }
                    match event {
                        SlideNumberGesture::Cancel => {
                            this.physics_cancel(cx);
                            window.focus(&this.focus, cx);
                        }
                        SlideNumberGesture::End => window.focus(&this.focus, cx),
                        SlideNumberGesture::Begin => {}
                    }
                },
            );
            let key = field.clone();
            let change = cx.subscribe_in(
                &number,
                window,
                move |this, _, event: &SlideNumberChange, window, cx| {
                    if this.physics_allowed(epoch, window, cx) {
                        this.physics_numeric_change(key.clone(), event.0 as f32, true, cx);
                    }
                },
            );
            self.physics.numbers.insert(
                field.clone(),
                NumberControl {
                    input,
                    number,
                    _events: vec![text, gesture, change],
                },
            );
        }
        let control = &self.physics.numbers[&field];
        control.number.update(cx, |number, cx| {
            number.configure(label.clone(), options, cx)
        });
        if !control.input.read(cx).focus_handle(cx).is_focused(window)
            && !control.number.read(cx).is_sliding()
        {
            let text = format_one_decimal(value as f64);
            if control.input.read(cx).value().as_ref() != text {
                control
                    .input
                    .update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
        let id = format!("physics-number-{field:?}");
        div()
            .id(SharedString::from(id))
            .test_support()
            .min_w_0()
            .w_full()
            .child(control.number.clone())
            .into_any_element()
    }
    fn physics_numeric_change(
        &mut self,
        field: NumberField,
        value: f32,
        _sliding: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(context) = self.physics.context else {
            return;
        };
        if let NumberField::Manual(id) = &field {
            let id = id.clone();
            let result = self.owner.update(cx, |owner, cx| {
                let result = owner.editor.set_physics_input(context.document, &id, value);
                owner.redraw(cx);
                result
            });
            self.physics.error = result.ok().and_then(Result::err).map(|e| e.to_string());
            cx.notify();
            return;
        }
    }
    pub(super) fn physics_select(
        &mut self,
        key: SelectField,
        choices: Vec<(String, String)>,
        value: String,
        label: String,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut choices = choices
            .into_iter()
            .map(|(value, label)| Choice {
                value,
                label: label.into(),
            })
            .collect::<Vec<_>>();
        if !choices.iter().any(|c| c.value == value) {
            choices.push(Choice {
                value: value.clone(),
                label: "—".into(),
            });
        }
        if !self.physics.selects.contains_key(&key) {
            let selected = choices
                .iter()
                .position(|c| c.value == value)
                .map(IndexPath::new);
            let state = cx.new(|cx| {
                SelectState::new(SearchableVec::new(choices.clone()), selected, window, cx)
                    .searchable(true)
            });
            let epoch = self.physics.epoch;
            let event = cx.subscribe_in(
                &state,
                window,
                move |this, source, event: &SelectEvent<SearchableVec<Choice>>, window, cx| {
                    if !this.physics_allowed(epoch, window, cx)
                        || this
                            .physics
                            .selects
                            .get(&key)
                            .is_none_or(|c| &c.state != source)
                    {
                        return;
                    }
                    if let SelectEvent::Confirm(Some(value)) = event {
                        this.physics_select_change(key, value.clone(), cx);
                    }
                },
            );
            self.physics.selects.insert(
                key,
                SelectControl {
                    state,
                    choices: choices.clone(),
                    _event: event,
                },
            );
        }
        let control = self.physics.selects.get_mut(&key).unwrap();
        if control.choices != choices {
            control.state.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(choices.clone()), window, cx)
            });
            control.choices = choices;
        }
        if control.state.read(cx).selected_value() != Some(&value) {
            control
                .state
                .update(cx, |state, cx| state.set_selected_value(&value, window, cx));
        }
        let control = div()
            .id(SharedString::from(format!("physics-select-{key:?}")))
            .test_support()
            .min_w_0()
            .w_full()
            .child(
                compact::select(Select::new(&control.state))
                    .w_full()
                    .h(compact::rem(compact::NUMBER_CONTROL))
                    .menu_width(compact::rem(260.))
                    .accessibility_label(label.clone())
                    .disabled(disabled),
            )
            .into_any_element();
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(compact::rem(2.))
            .child(
                div()
                    .id("physics-reuse-label")
                    .test_support()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(control)
            .into_any_element()
    }
    fn physics_select_change(&mut self, key: SelectField, value: String, cx: &mut Context<Self>) {
        if key != SelectField::Reuse {
            return;
        }
        self.physics_cancel(cx);
        let source = self.physics_asset(cx).and_then(|asset| {
            value.rsplit_once(':').and_then(|(id, i)| {
                asset
                    .data
                    .settings
                    .iter()
                    .find(|rig| rig.id == id)?
                    .vertices
                    .get(i.parse::<usize>().ok()?)
                    .cloned()
            })
        });
        let (Some(source), Some(target)) = (source, self.target.clone()) else {
            return;
        };
        let node = self.physics.node;
        let result = self
            .owner
            .update(cx, |owner, cx| {
                let result = (|| {
                    use editor_core::property::physics as schema;
                    use editor_core::{PropertyPatch, PropertyScope, PropertyValue};
                    let current = target.values(&owner.editor)?;
                    let changes: editor_core::PropertyValues = [
                        (schema::RADIUS, source.radius),
                        (schema::MOBILITY, source.mobility),
                        (schema::DELAY, source.delay),
                        (schema::ACCELERATION, source.acceleration),
                    ]
                    .into_iter()
                    .map(|(spec, value)| (spec.key.at(node), PropertyValue::Float(value)))
                    .collect();
                    let expected = changes
                        .keys()
                        .map(|key| {
                            current
                                .get(key)
                                .cloned()
                                .map(|value| (*key, value))
                                .ok_or_else(|| anyhow::anyhow!("Physics segment changed"))
                        })
                        .collect::<anyhow::Result<_>>()?;
                    target.commit(
                        &mut owner.editor,
                        PropertyPatch {
                            scope: PropertyScope::Object,
                            expected,
                            changes,
                        },
                    )
                })();
                owner.animation_anchor = None;
                owner.redraw(cx);
                result
            })
            .unwrap_or_else(|error| Err(error.into()));
        self.physics.error = result.err().map(|error| error.to_string());
        cx.notify();
    }
    pub(super) fn physics_channel_actions(
        &self,
        output: bool,
        manual: bool,
        disabled: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (len, index) = self
            .physics_asset(cx)
            .and_then(|asset| {
                asset
                    .data
                    .settings
                    .iter()
                    .find(|r| r.id == self.physics.group)
                    .map(|r| {
                        if output {
                            (r.outputs.len(), self.physics.output)
                        } else {
                            (r.inputs.len(), self.physics.input)
                        }
                    })
            })
            .unwrap_or_default();
        let epoch = self.physics.epoch;
        let mut row = div().flex().items_center().gap(compact::rem(2.));
        for (action, icon, label, off) in [
            (0, IconName::Plus, "physics.add_channel", false),
            (1, IconName::Trash, "physics.delete_channel", len == 0),
            (2, IconName::ArrowUp, "physics.up", index == 0 || len == 0),
            (3, IconName::ArrowDown, "physics.down", index + 1 >= len),
        ] {
            let id = SharedString::from(format!("physics-channel-{output}-{action}"));
            row = row.child(
                compact::header_button(
                    Button::new(id)
                        .ghost()
                        .icon(icon)
                        .tooltip(t!(label).into_owned())
                        .accessibility_label(t!(label).into_owned()),
                )
                .disabled(disabled || off)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !this.physics_allowed(epoch, window, cx) {
                        return;
                    }
                    let parameter = this.owner.upgrade().and_then(|owner| {
                        owner
                            .read(cx)
                            .editor
                            .document()?
                            .animation_catalog()
                            .parameters
                            .first()
                            .map(|p| (p.id.clone(), p.runtime_id.clone()))
                    });
                    this.physics_mutate(cx, move |asset, group| {
                        let Some(rig_index) =
                            asset.data.settings.iter().position(|r| r.id == group)
                        else {
                            return;
                        };
                        if action == 0 {
                            let (id, mut runtime) =
                                parameter.unwrap_or_else(|| (String::new(), "Parameter".into()));
                            if !id.is_empty() {
                                runtime = asset.parameter_binding_key(&id, &runtime);
                                asset.parameter_bindings.insert(runtime.clone(), id);
                            }
                            let reference = PhysicsParameterRef {
                                target: "Parameter".into(),
                                id: runtime,
                                extensions: Default::default(),
                            };
                            let rig = std::sync::Arc::make_mut(&mut asset.data.settings[rig_index]);
                            if output {
                                rig.outputs.push(PhysicsOutput {
                                    destination: reference,
                                    vertex_index: usize::from(rig.vertices.len() > 1),
                                    scale: 1.,
                                    weight: 100.,
                                    kind: PhysicsAxis::Angle,
                                    reflect: false,
                                    extensions: Default::default(),
                                })
                            } else {
                                rig.inputs.push(PhysicsInput {
                                    source: reference,
                                    weight: 100.,
                                    kind: PhysicsAxis::X,
                                    reflect: false,
                                    extensions: Default::default(),
                                })
                            }
                        } else {
                            let rig = std::sync::Arc::make_mut(&mut asset.data.settings[rig_index]);
                            if output {
                                match action {
                                    1 => {
                                        rig.outputs.remove(index);
                                    }
                                    2 => rig.outputs.swap(index, index - 1),
                                    _ => rig.outputs.swap(index, index + 1),
                                }
                            } else {
                                match action {
                                    1 => {
                                        rig.inputs.remove(index);
                                    }
                                    2 => rig.inputs.swap(index, index - 1),
                                    _ => rig.inputs.swap(index, index + 1),
                                }
                            }
                        }
                    });
                    let next = match action {
                        0 => len,
                        2 => index.saturating_sub(1),
                        3 => index + 1,
                        _ => index.saturating_sub(1),
                    };
                    if output {
                        this.physics.output = next
                    } else {
                        this.physics.input = next
                    }
                    cx.notify();
                })),
            );
        }
        if !output && manual {
            row = row.child(
                compact::header_button(
                    Button::new("physics-reset-inputs").label(t!("physics.reset_inputs")),
                )
                .disabled(disabled)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.physics_session(window, cx, |editor, id| editor.reset_physics_inputs(id))
                })),
            );
        }
        row.into_any_element()
    }
    pub(super) fn physics_particle_actions(
        &self,
        rig: &PhysicsSetting,
        disabled: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let epoch = self.physics.epoch;
        let node = self.physics.node;
        let len = rig.vertices.len();
        div()
            .flex()
            .gap(compact::rem(2.))
            .items_center()
            .child(
                self.physics_note(format!("{} {}", t!("physics.segment"), node), cx)
                    .flex_1(),
            )
            .child(
                self.physics_button(
                    "physics-add-node",
                    IconName::Plus,
                    t!("physics.add_segment").into_owned(),
                    disabled,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.physics_allowed(epoch, window, cx) {
                        this.physics_mutate(cx, |asset, group| {
                            if let Some(rig) =
                                asset.data.settings.iter_mut().find(|r| r.id == group)
                            {
                                let rig = std::sync::Arc::make_mut(rig);
                                let mut vertex = rig.vertices.last().unwrap().clone();
                                vertex.radius = vertex.radius.max(1.);
                                vertex.position.y += vertex.radius;
                                rig.vertices.push(vertex);
                            }
                        });
                        this.physics.node = len;
                        cx.notify();
                    }
                })),
            )
            .child(
                self.physics_button(
                    "physics-delete-node",
                    IconName::Trash,
                    t!("physics.delete_segment").into_owned(),
                    disabled || node == 0 || len <= 2,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.physics_allowed(epoch, window, cx) {
                        this.physics_mutate(cx, move |asset, group| {
                            if let Some(rig) =
                                asset.data.settings.iter_mut().find(|r| r.id == group)
                            {
                                let rig = std::sync::Arc::make_mut(rig);
                                rig.vertices.remove(node);
                                rig.outputs.retain(|o| o.vertex_index != node);
                                for output in &mut rig.outputs {
                                    if output.vertex_index > node {
                                        output.vertex_index -= 1;
                                    }
                                }
                            }
                        });
                        this.physics.node = node.saturating_sub(1).max(1);
                        cx.notify();
                    }
                })),
            )
            .into_any_element()
    }
}
