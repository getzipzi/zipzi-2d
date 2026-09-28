use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand};
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    checkbox::Checkbox,
    dock::{BasePanel, Panel, PanelEvent},
    input::{Input, InputEvent, InputState},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
};
use gpui_kit::{prelude::*, *};
use std::time::Duration;

pub struct PropertiesPanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    target: Option<(editor_core::DocumentId, Option<String>)>,
    pub name: Entity<InputState>,
    pub opacity: Entity<SliderState>,
    transient_opacity: Option<f32>,
    pending_preview_opacity: Option<f32>,
    throttle_timer: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    _observe: Subscription,
}

impl PropertiesPanel {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let (name, opacity) = Self::build_states(window, cx);
        let entity = cx.new(|cx| {
            let focus = cx.focus_handle();
            let observe = cx.observe(&owner.upgrade().unwrap(), |_this: &mut Self, _, cx| {
                cx.notify();
            });
            Self {
                owner: owner.clone(),
                focus,
                target: None,
                name,
                opacity,
                transient_opacity: None,
                pending_preview_opacity: None,
                throttle_timer: None,
                _subscriptions: Vec::new(),
                _observe: observe,
            }
        });

        entity.update(cx, |this, cx| {
            this.rebind_subscriptions(window, cx);
        });

        entity
    }

    fn build_states(
        window: &mut Window,
        cx: &mut App,
    ) -> (Entity<InputState>, Entity<SliderState>) {
        let name = cx
            .new(|cx| InputState::new(window, cx).placeholder(t!("editor.layer_name_placeholder")));
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(0.)
        });
        (name, opacity)
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.name.focus_handle(cx).is_focused(window)
    }

    fn editable_target(&self, owner: &EditorView, dialog_active: bool) -> Option<DocumentId> {
        let (id, layer_id) = self.target.as_ref()?;
        let selected_id = owner.editor.selected_layer().map(|layer| layer.id);
        if owner.modal
            || dialog_active
            || owner.editor.is_busy()
            || owner.editor.active_id() != Some(*id)
            || selected_id.as_ref() != layer_id.as_ref()
        {
            None
        } else {
            Some(*id)
        }
    }

    fn rebind_subscriptions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_sub = cx.subscribe_in(
            &self.name,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| {
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                if input != &this.name {
                    return;
                }
                let dialog_active = window.has_active_dialog(cx);
                let Some(id) = ({
                    let state = owner.read(cx);
                    this.editable_target(&state, dialog_active)
                }) else {
                    return;
                };
                match event {
                    InputEvent::Change => {
                        let text_val = input.read(cx).value().to_string();
                        owner.update(cx, |owner, cx| {
                            let result = owner
                                .editor
                                .execute_for(id, EditorCommand::PreviewName(text_val));
                            owner.complete_edit(result, cx);
                        });
                    }
                    InputEvent::PressEnter { .. } => return,
                    InputEvent::Blur => {
                        owner.update(cx, |owner, cx| {
                            let result = owner.editor.execute_for(id, EditorCommand::FinishEdit);
                            owner.complete_edit(result, cx);
                        });
                    }
                    InputEvent::Focus => {
                        cx.notify();
                        return;
                    }
                }
            },
        );

        let opacity_sub =
            cx.subscribe_in(
                &self.opacity,
                window,
                |this: &mut Self, slider, event: &SliderEvent, window, cx| {
                    let Some(owner) = this.owner.upgrade() else {
                        return;
                    };
                    if slider != &this.opacity {
                        return;
                    }
                    let dialog_active = window.has_active_dialog(cx);
                    let Some(id) = ({
                        let state = owner.read(cx);
                        this.editable_target(&state, dialog_active)
                    }) else {
                        return;
                    };

                    match event {
                        SliderEvent::Change(SliderValue::Single(val)) => {
                            let normalized = (*val / 100.0).clamp(0.0, 1.0);
                            this.transient_opacity = Some(*val);
                            this.pending_preview_opacity = Some(normalized);
                            cx.notify();

                            if this.throttle_timer.is_none() {
                                let val_to_dispatch =
                                    this.pending_preview_opacity.take().unwrap_or(normalized);
                                owner.update(cx, |owner, cx| {
                                    let result = owner.editor.execute_for(
                                        id,
                                        EditorCommand::PreviewOpacity(val_to_dispatch),
                                    );
                                    owner.complete_edit(result, cx);
                                });

                                let target = this.target.clone();
                                this.throttle_timer =
                                    Some(cx.spawn_in(window, async move |this, cx| {
                                        cx.background_executor()
                                            .timer(Duration::from_millis(16))
                                            .await;
                                        let _ =
                                            this.update_in(cx, |this, window, cx| {
                                                this.throttle_timer = None;
                                                if this.target == target {
                                                    if let Some(next_val) =
                                                        this.pending_preview_opacity.take()
                                                    {
                                                        if let Some(owner) = this.owner.upgrade() {
                                                            let dialog_active =
                                                                window.has_active_dialog(cx);
                                                            owner.update(cx, |owner, cx| {
                                                    if this.editable_target(owner, dialog_active)
                                                        == Some(id)
                                                    {
                                                        let result = owner.editor.execute_for(
                                                            id,
                                                            EditorCommand::PreviewOpacity(next_val),
                                                        );
                                                        owner.complete_edit(result, cx);
                                                    }
                                                });
                                                        }
                                                    }
                                                }
                                                cx.notify();
                                            });
                                    }));
                            }
                        }
                        SliderEvent::Release(_) => {
                            this.throttle_timer = None;
                            let final_val = this.pending_preview_opacity.take().or_else(|| {
                                this.transient_opacity.map(|v| (v / 100.0).clamp(0.0, 1.0))
                            });

                            this.transient_opacity = None;
                            owner.update(cx, |owner, cx| {
                                let result = (|| {
                                    if let Some(val) = final_val {
                                        owner
                                            .editor
                                            .execute_for(id, EditorCommand::PreviewOpacity(val))?;
                                    }
                                    owner.editor.execute_for(id, EditorCommand::FinishEdit)
                                })();
                                owner.complete_edit(result, cx);
                            });
                            cx.notify();
                        }
                        _ => {}
                    }
                },
            );

        self._subscriptions = vec![name_sub, opacity_sub];
    }

    pub fn sync_with_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let (target, name_str, opacity_val) = {
            let state = owner.read(cx);
            let layer = state.editor.selected_layer();
            let target = state
                .editor
                .active_id()
                .map(|id| (id, layer.as_ref().map(|l| l.id.clone())));
            let name = layer.as_ref().map(|l| l.name.clone()).unwrap_or_default();
            let opacity = layer.as_ref().map_or(0., |l| l.opacity * 100.);
            (target, name, opacity)
        };

        if self.target != target {
            self.target = target;
            let (name, opacity) = Self::build_states(window, cx);
            self.name = name;
            self.opacity = opacity;
            self.transient_opacity = None;
            self.pending_preview_opacity = None;
            self.throttle_timer = None;

            self.name.update(cx, |state, cx| {
                state.set_value(name_str, window, cx);
            });
            self.opacity.update(cx, |state, cx| {
                state.set_value(opacity_val, window, cx);
            });

            self.rebind_subscriptions(window, cx);
            cx.notify();
        } else {
            if self.name.read(cx).value().as_ref() != name_str.as_str()
                && !self.name.focus_handle(cx).is_focused(window)
            {
                self.name.update(cx, |state, cx| {
                    state.set_value(name_str, window, cx);
                });
            }
            if self.transient_opacity.is_none() {
                let current_val = match self.opacity.read(cx).value() {
                    SliderValue::Single(v) => v,
                    _ => 0.0,
                };
                if (current_val - opacity_val).abs() > 0.01 {
                    self.opacity.update(cx, |state, cx| {
                        state.set_value(opacity_val, window, cx);
                    });
                }
            }
        }
    }
}

impl EventEmitter<PanelEvent> for PropertiesPanel {}
impl Focusable for PropertiesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for PropertiesPanel {
    fn panel_name(&self) -> &'static str {
        "Properties"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for PropertiesPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.properties")
    }
}

impl Render for PropertiesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_with_editor(window, cx);

        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };

        let dialog_active = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let target = state.editor.active_id();
        let selected = state.editor.selected_layer();
        let disabled = state.modal || dialog_active || state.editor.is_busy() || selected.is_none();
        let render_target = self.target.clone();
        let theme = cx.theme();

        let display_opacity = self
            .transient_opacity
            .unwrap_or_else(|| selected.as_ref().map_or(0., |s| s.opacity * 100.));

        div()
            .size_full()
            .p_3()
            .flex()
            .flex_col()
            .gap_4()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .track_focus(&self.focus)
            // Layer Name Section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(t!("editor.name")),
                    )
                    .child(
                        Input::new(&self.name)
                            .id("property-name")
                            .disabled(disabled),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(t!("editor.name_help")),
                    ),
            )
            // Appearance Section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                Checkbox::new("visible")
                                    .label(SharedString::from(t!("editor.visible")))
                                    .checked(selected.as_ref().is_some_and(|s| s.visible))
                                    .disabled(disabled)
                                    .on_change(cx.listener(move |this, value, window, cx| {
                                        let Some(owner) = this.owner.upgrade() else {
                                            return;
                                        };
                                        let Some(target) = target else {
                                            return;
                                        };
                                        let dialog_active = window.has_active_dialog(cx);
                                        owner.update(cx, |owner, cx| {
                                            if this.target != render_target
                                                || this.editable_target(owner, dialog_active)
                                                    != Some(target)
                                            {
                                                return;
                                            }
                                            let result = owner.editor.execute_for(
                                                target,
                                                EditorCommand::SetVisible(*value),
                                            );
                                            owner.complete_edit(result, cx);
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .rounded(px(4.))
                                    .bg(theme.muted)
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{:.0}%", display_opacity)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(theme.muted_foreground).child(t!(
                                "editor.opacity",
                                percent = format!("{:.0}", display_opacity)
                            )))
                            .child(Slider::new(&self.opacity).disabled(disabled)),
                    ),
            )
            // Mesh Stats Section
            .child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(theme.muted)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(selected.map_or_else(
                        || t!("editor.no_layer").into_owned(),
                        |s| {
                            t!(
                                "editor.mesh_info",
                                vertices = s.vertices,
                                triangles = s.triangles
                            )
                            .into_owned()
                        },
                    )),
            )
            .into_any_element()
    }
}
