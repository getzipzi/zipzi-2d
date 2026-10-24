//! Compact vertical physics authoring and presented-frame inspection.
mod controls;
mod drawing;
mod table;
use super::PropertiesPanel;
use crate::properties::physics::NumberField;
use crate::t;
use crate::ui::compact;
use controls::{NumberControl, SelectControl, SelectField};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::component::{
    checkbox::Checkbox,
    input::{InputEvent, InputState},
    select::{SearchableVec, SelectState},
};
use gpui_kit::{prelude::*, *};
use std::collections::HashMap;
use zipzi_core::{
    document::{PhysicsAsset, PhysicsEdit, PhysicsSetting},
    physics::PhysicsAxis,
};
use zipzi_editor_core::{AnimationPreviewMode, DocumentId, EditingContextToken};

#[derive(Default)]
pub(super) struct PhysicsState {
    context: Option<EditingContextToken>,
    pub(super) visible: bool,
    group: String,
    input: usize,
    output: usize,
    node: usize,
    input_id: Option<String>,
    output_id: Option<String>,
    node_id: Option<String>,
    numbers: HashMap<NumberField, NumberControl>,
    selects: HashMap<SelectField, SelectControl>,
    epoch: u64,
    error: Option<String>,
    drawing: drawing::DrawingState,
    scroll: ScrollHandle,
}
impl PhysicsState {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

impl PropertiesPanel {
    fn physics_allowed(&self, epoch: u64, window: &mut Window, cx: &mut App) -> bool {
        self.visible
            && self.physics.visible
            && self.physics.epoch == epoch
            && !window.has_active_dialog(cx)
            && self.owner.upgrade().is_some_and(|owner| {
                let owner = owner.read(cx);
                !owner.modal
                    && !owner.editor.is_busy()
                    && !owner.editor.exiting()
                    && self.resolved_target(&owner.editor) == self.target
                    && self
                        .physics
                        .context
                        .is_some_and(|context| owner.editor.editing_context_token_valid(context))
            })
    }
    fn physics_asset(&self, cx: &App) -> Option<PhysicsAsset> {
        self.owner
            .upgrade()?
            .read(cx)
            .editor
            .document()?
            .physics_definition()
            .cloned()
    }
    fn physics_cancel(&mut self, cx: &mut Context<Self>) {
        self.cancel_preview(cx);
    }
    fn physics_clear_controls(&mut self, cx: &mut Context<Self>) {
        self.physics_cancel(cx);
        self.physics.epoch += 1;
        self.physics.numbers.clear();
        self.physics.selects.clear();
        self.physics.error = None;
    }
    fn physics_group(&mut self, group: String, cx: &mut Context<Self>) {
        self.physics_clear_controls(cx);
        self.physics.group = group.clone();
        self.physics.input = 0;
        self.physics.output = 0;
        self.physics.node = 1;
        self.physics.input_id = None;
        self.physics.output_id = None;
        self.physics.node_id = None;
        self.physics.drawing = Default::default();
        cx.notify();
    }
    fn physics_remember_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(asset) = self.physics_asset(cx) {
            if let Some(rig) = asset
                .data
                .settings
                .iter()
                .find(|g| g.id == self.physics.group)
            {
                self.physics.input_id = rig.inputs.get(self.physics.input).map(|e| e.id.clone());
                self.physics.output_id = rig.outputs.get(self.physics.output).map(|e| e.id.clone());
                self.physics.node_id = rig.vertices.get(self.physics.node).map(|e| e.id.clone());
            }
        }
    }
    fn physics_inspect_child(&self, object: zipzi_editor_core::ObjectRef, cx: &mut Context<Self>) {
        let _ = self.owner.update(cx, |v, cx| {
            if let Some(document) = v.editor.active_id() {
                let _ = v.editor.inspect_authoring_object(document, object);
                v.redraw(cx);
            }
        });
    }
    fn physics_apply(&mut self, intent: PhysicsEdit, cx: &mut Context<Self>) -> bool {
        self.physics_cancel(cx);
        let Some(context) = self.physics.context else {
            return false;
        };
        let result = self
            .owner
            .update(cx, |owner, cx| {
                let result = (|| {
                    anyhow::ensure!(
                        owner.editor.editing_context_token_valid(context) && !owner.modal,
                        "Physics context changed"
                    );
                    let plan = owner
                        .editor
                        .plan_physics_authoring(context.document, intent)?;
                    owner
                        .editor
                        .execute_physics_authoring(context.document, &plan)?;
                    Ok(())
                })();
                owner.animation_anchor = None;
                owner.redraw(cx);
                result
            })
            .unwrap_or_else(|e| Err(e.into()));
        let succeeded = result.is_ok();
        self.physics.error = result.err().map(|e: anyhow::Error| e.to_string());
        cx.notify();
        succeeded
    }
    fn physics_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut zipzi_editor_core::Editor, DocumentId) -> anyhow::Result<()>,
    ) {
        if !self.physics_allowed(self.physics.epoch, window, cx) {
            return;
        }
        self.physics_cancel(cx);
        let Some(context) = self.physics.context else {
            return;
        };
        let result = self
            .owner
            .update(cx, |owner, cx| {
                let result = change(&mut owner.editor, context.document);
                owner.animation_anchor = None;
                owner.redraw(cx);
                result
            })
            .unwrap_or_else(|e| Err(e.into()));
        self.physics.error = result.err().map(|e| e.to_string());
        window.focus(&self.focus, cx);
        cx.notify();
    }
    fn physics_button(
        &self,
        id: &'static str,
        icon: IconName,
        label: String,
        disabled: bool,
    ) -> Button {
        compact::header_button(
            Button::new(id)
                .icon(icon)
                .ghost()
                .tooltip(label.clone())
                .accessibility_label(label),
        )
        .disabled(disabled)
    }
    fn physics_section(&self, key: &'static str, label: String, cx: &Context<Self>) -> AnyElement {
        div()
            .id(SharedString::from(format!("physics-fold-{key}")))
            .test_support()
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                self.property_group(key.into(), label, self.collapsed.contains(key), cx)
                    .h(compact::rem(compact::HEADER_CONTROL)),
            )
            .into_any_element()
    }
    fn physics_note(&self, text: impl Into<SharedString>, cx: &App) -> Div {
        div()
            .text_size(compact::rem(compact::CAPTION))
            .text_color(cx.theme().muted_foreground)
            .child(text.into())
    }
    pub(super) fn render_physics(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.physics.visible = true;
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let context = owner.read(cx).editor.editing_context_token();
        if self.physics.context != context {
            self.physics_clear_controls(cx);
            self.physics.context = context;
            self.physics.group.clear();
            self.physics.drawing = Default::default();
        }
        let disabled = !self.physics_allowed(self.physics.epoch, window, cx);
        let mut body = div()
            .id("physics-properties")
            .test_support()
            .track_focus(&self.focus)
            .key_context("ZipziDocument")
            .on_action(cx.listener(
                |this, _: &gpui_kit::component::dialog::Cancel, window, cx| {
                    if this.preview.is_some() {
                        this.physics_cancel(cx);
                        window.focus(&this.focus, cx);
                        cx.stop_propagation();
                    } else {
                        cx.propagate();
                    }
                },
            ))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.physics.scroll)
            .p(compact::rem(compact::PADDING))
            .flex()
            .flex_col()
            .gap(compact::rem(4.))
            .text_size(compact::rem(compact::BODY));
        let Some(document) = owner.read(cx).editor.document() else {
            return body.child(t!("editor.welcome")).into_any_element();
        };
        let parameters = document.parameters();
        let Some(asset) = document.physics_definition().cloned() else {
            return compact::scroll_frame(body, &self.physics.scroll).into_any_element();
        };
        let selected = self.target.as_ref().map(|target| target.object.clone());
        let group = selected.and_then(|object| {
            asset
                .data
                .settings
                .iter()
                .find(|rig| {
                    object
                        == zipzi_editor_core::ObjectRef::new(
                            zipzi_editor_core::ObjectKind::PhysicsGroup,
                            rig.id.clone(),
                        )
                })
                .map(|rig| rig.id.clone())
        });
        let Some(group) = group else {
            return compact::scroll_frame(body, &self.physics.scroll).into_any_element();
        };
        if self.physics.group != group {
            self.physics_group(group, cx);
        }
        let name = self.render_inline_field(crate::properties::physics::NAME.key, window, cx);
        body = body.child(name);
        let animation = owner.read(cx).editor.animation().unwrap();
        let manual = animation.physics_preview.manual;
        let bypass = animation.physics_preview.bypass;
        let editing = animation.preview_mode == AnimationPreviewMode::Editing && !manual;
        let settings = animation.physics_preview.clone();
        let snapshot = owner.read(cx).displayed_animation();
        // Retained canvas images can predate a candidate; never label their traces
        // as current-configuration observations while a replacement is pending.
        let current = animation.presented.is_some();
        let trace = snapshot
            .filter(|_| current && !editing && !bypass)
            .and_then(|s| s.physics.iter().find(|r| r.id == self.physics.group))
            .cloned();
        let final_values = snapshot
            .filter(|_| current)
            .map(|s| s.parameters.clone())
            .unwrap_or_default();
        body = body.child(
            Checkbox::new("physics-manual")
                .xsmall()
                .label(t!("physics.manual").into_owned())
                .checked(manual)
                .disabled(disabled)
                .on_click(cx.listener(|this, value, window, cx| {
                    let manual = *value;
                    this.physics_session(window, cx, |editor, id| {
                        editor.set_physics_manual(id, manual)
                    });
                })),
        );
        if editing {
            body = body
                .child(self.physics_note(t!("physics.editing_hint").into_owned(), cx))
                .child(
                    compact::header_button(
                        Button::new("physics-result").label(t!("physics.result")),
                    )
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.physics_session(window, cx, |editor, id| {
                            editor.set_animation_preview_mode(id, AnimationPreviewMode::Result)
                        })
                    })),
                );
        } else {
            body = body.child(
                Checkbox::new("physics-bypass")
                    .xsmall()
                    .label(t!("physics.bypass").into_owned())
                    .checked(bypass)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, value, window, cx| {
                        let bypass = *value;
                        this.physics_session(window, cx, |editor, id| {
                            editor.set_physics_bypass(id, bypass)
                        });
                    })),
            );
        }
        body = body.child(
            div()
                .flex()
                .gap(compact::rem(4.))
                .min_w_0()
                .child(
                    self.physics_note(t!("physics.model_scope").into_owned(), cx)
                        .flex_1()
                        .min_w_0()
                        .truncate(),
                )
                .child(
                    div()
                        .id("animation-physics-fps")
                        .test_support()
                        .aria_label(asset.data.meta.fps.map_or_else(
                            || t!("workspace.resource_default").into_owned(),
                            |fps| format!("{fps:.1} fps"),
                        ))
                        .flex_shrink_0()
                        .text_size(compact::rem(compact::CAPTION))
                        .child(asset.data.meta.fps.map_or_else(
                            || t!("workspace.resource_default").into_owned(),
                            |fps| format!("{fps:.1} fps"),
                        )),
                ),
        );
        let Some((rig_index, rig)) = asset
            .data
            .settings
            .iter()
            .enumerate()
            .find(|(_, r)| r.id == self.physics.group)
        else {
            return compact::scroll_frame(body, &self.physics.scroll).into_any_element();
        };
        if let Some(object) = owner
            .read(cx)
            .editor
            .active_inspection()
            .and_then(|i| i.object())
        {
            match object.kind {
                zipzi_editor_core::ObjectKind::PhysicsInput => {
                    self.physics.input_id = Some(object.id)
                }
                zipzi_editor_core::ObjectKind::PhysicsOutput => {
                    self.physics.output_id = Some(object.id)
                }
                zipzi_editor_core::ObjectKind::PhysicsNode => {
                    self.physics.node_id = Some(object.id)
                }
                _ => {}
            }
        }
        if let Some(i) = self
            .physics
            .input_id
            .as_ref()
            .and_then(|id| rig.inputs.iter().position(|e| &e.id == id))
        {
            self.physics.input = i;
        }
        if let Some(i) = self
            .physics
            .output_id
            .as_ref()
            .and_then(|id| rig.outputs.iter().position(|e| &e.id == id))
        {
            self.physics.output = i;
        }
        if let Some(i) = self
            .physics
            .node_id
            .as_ref()
            .and_then(|id| rig.vertices.iter().position(|e| &e.id == id))
        {
            self.physics.node = i;
        }
        self.physics.input = self.physics.input.min(rig.inputs.len().saturating_sub(1));
        self.physics.output = self.physics.output.min(rig.outputs.len().saturating_sub(1));
        self.physics.node = self.physics.node.min(rig.vertices.len().saturating_sub(1));
        self.physics.input_id = rig.inputs.get(self.physics.input).map(|e| e.id.clone());
        self.physics.output_id = rig.outputs.get(self.physics.output).map(|e| e.id.clone());
        self.physics.node_id = rig.vertices.get(self.physics.node).map(|e| e.id.clone());
        let targets: Vec<_> = asset
            .data
            .settings
            .iter()
            .flat_map(|g| {
                g.inputs
                    .iter()
                    .map(|v| &v.source)
                    .chain(g.outputs.iter().map(|v| &v.destination))
            })
            .collect::<Vec<_>>();
        let unconfigured = targets
            .iter()
            .filter(|t| matches!(t, zipzi_core::document::PhysicsTarget::Unspecified))
            .count();
        let unresolved = targets
            .iter()
            .filter_map(|t| match t {
                zipzi_core::document::PhysicsTarget::Unresolved { runtime_id } => {
                    Some(runtime_id.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if unconfigured > 0 {
            body = body.child(
                div()
                    .id("physics-unconfigured")
                    .test_support()
                    .text_color(cx.theme().warning)
                    .child(t!("physics.unconfigured", count = unconfigured).into_owned()),
            );
        }
        if !unresolved.is_empty() {
            body = body.child(
                div()
                    .id("physics-unresolved")
                    .test_support()
                    .text_color(cx.theme().danger)
                    .child(t!("physics.unresolved", ids = unresolved.join(", ")).into_owned()),
            );
        }
        body = body.child(self.physics_section(
            "physics-inputs",
            t!("workspace.physics_inputs").into_owned(),
            cx,
        ));
        if !self.collapsed.contains("physics-inputs") {
            body = body.child(self.physics_channel_actions(false, manual, disabled, cx));
            body = body.child(self.physics_table(
                false,
                &asset,
                &parameters,
                rig_index,
                trace.as_ref(),
                &final_values,
                manual.then_some(&settings),
                disabled,
                window,
                cx,
            ));
            body = body.child(self.physics_section(
                "physics-normalization",
                t!("physics.normalization").into_owned(),
                cx,
            ));
            if !self.collapsed.contains("physics-normalization") {
                body = body.child(
                    div()
                        .flex()
                        .gap(compact::rem(4.))
                        .child(
                            self.physics_note(t!("physics.position").into_owned(), cx)
                                .flex_1(),
                        )
                        .child(
                            self.physics_note(t!("physics.angle").into_owned(), cx)
                                .flex_1(),
                        ),
                );
                for (n, label) in [
                    (0, "physics.minimum"),
                    (1, "physics.center"),
                    (2, "physics.maximum"),
                ] {
                    let mut row = div().flex().items_center().gap(compact::rem(4.)).min_w_0();
                    for (angle, range) in [
                        (false, rig.normalization.position),
                        (true, rig.normalization.angle),
                    ] {
                        let value = match n {
                            0 => range.minimum,
                            1 => range.default,
                            _ => range.maximum,
                        };
                        row = row.child(div().flex_1().min_w_0().child(self.physics_number(
                            NumberField::Normalization(angle, n),
                            t!(label).into_owned(),
                            value,
                            (-10000., 10000., 0.1),
                            disabled,
                            window,
                            cx,
                        )));
                    }
                    body = body.child(row);
                }
                body = body
                    .child(self.physics_note(t!("physics.normalization_hint").into_owned(), cx));
            }
        }
        body =
            body.child(self.physics_section("physics-chain", t!("physics.chain").into_owned(), cx));
        if !self.collapsed.contains("physics-chain") {
            let mut settings = div()
                .flex()
                .flex_col()
                .gap(compact::rem(3.))
                .id("physics-pendulum-settings")
                .test_support()
                .flex_1()
                .min_w_0()
                .child(self.physics_particle_actions(rig, disabled, cx));
            if let Some(vertex) = rig
                .vertices
                .get(self.physics.node)
                .filter(|_| self.physics.node > 0)
            {
                for (field, label, value, range) in [
                    (0, "physics.length", vertex.radius, (0.001, 10000., 0.1)),
                    (1, "physics.mobility", vertex.mobility, (0., 1., 0.01)),
                    (2, "physics.delay", vertex.delay, (0., 10., 0.01)),
                    (
                        3,
                        "physics.acceleration",
                        vertex.acceleration,
                        (0., 100., 0.01),
                    ),
                ] {
                    settings = settings.child(self.physics_number(
                        NumberField::Vertex(self.physics.node, field),
                        t!(label).into_owned(),
                        value,
                        range,
                        disabled,
                        window,
                        cx,
                    ));
                }
            }
            let choices = asset
                .data
                .settings
                .iter()
                .flat_map(|r| {
                    let name = group_name(&asset, &r.id).to_owned();
                    r.vertices
                        .iter()
                        .enumerate()
                        .skip(1)
                        .map(move |(i, node)| (node.id.clone(), format!("{name} · {i}")))
                })
                .collect();
            settings = settings.child(self.physics_select(
                SelectField::Reuse,
                choices,
                String::new(),
                t!("physics.reuse").into_owned(),
                disabled || self.physics.node == 0,
                window,
                cx,
            ));
            let canvas = self.physics_chain(rig, trace.as_ref(), manual, disabled, cx);
            body = body.child(
                div()
                    .id("physics-pendulum-layout")
                    .flex()
                    .items_stretch()
                    .gap(compact::rem(6.))
                    .min_w_0()
                    .child(settings)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .relative()
                            .child(div().absolute().inset_0().child(canvas)),
                    ),
            );
        }
        body = body.child(self.physics_section(
            "physics-outputs",
            t!("workspace.physics_outputs").into_owned(),
            cx,
        ));
        if !self.collapsed.contains("physics-outputs") {
            body = body.child(self.physics_channel_actions(true, manual, disabled, cx));
            body = body.child(self.physics_table(
                true,
                &asset,
                &parameters,
                rig_index,
                trace.as_ref(),
                &final_values,
                None,
                disabled,
                window,
                cx,
            ));
            if let Some(output) = rig.outputs.get(self.physics.output) {
                if rig.node_index(&output.node_id) == Some(0) || output.kind != PhysicsAxis::Angle {
                    body = body.child(
                        self.physics_note(
                            if rig.node_index(&output.node_id) == Some(0) {
                                t!("physics.root_output")
                            } else {
                                t!("physics.translation_output")
                            }
                            .into_owned(),
                            cx,
                        ),
                    );
                }
                if let Some(result) = trace
                    .as_ref()
                    .and_then(|trace| {
                        trace
                            .output_ids
                            .iter()
                            .position(|id| id == &output.id)
                            .and_then(|slot| trace.outputs.get(slot))
                    })
                    .and_then(Option::as_ref)
                {
                    if result.clamped {
                        body =
                            body.child(self.physics_note(t!("physics.clamped").into_owned(), cx));
                    }
                }
            }
        }
        body = body.child(self.physics_section(
            "physics-global",
            t!("physics.global").into_owned(),
            cx,
        ));
        if !self.collapsed.contains("physics-global") {
            body = body
                .child(self.physics_number(
                    NumberField::Fps,
                    t!("workspace.frame_rate").into_owned(),
                    asset.data.meta.fps.unwrap_or(0.),
                    (0., 240., 1.),
                    disabled,
                    window,
                    cx,
                ))
                .child(self.physics_note(t!("physics.fps_hint").into_owned(), cx))
                .child(self.physics_note(t!("physics.forces_hint").into_owned(), cx));
        }
        if let Some(error) = &self.physics.error {
            body = body.child(
                div()
                    .id("physics-error")
                    .test_support()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        compact::scroll_frame(body, &self.physics.scroll).into_any_element()
    }
}

pub(crate) fn group_name<'a>(asset: &'a PhysicsAsset, id: &'a str) -> &'a str {
    asset
        .data
        .meta
        .dictionary
        .iter()
        .find(|entry| entry.id == id)
        .map_or(id, |entry| entry.name.as_str())
}
