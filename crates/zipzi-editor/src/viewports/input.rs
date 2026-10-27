//! Pointer ownership, tool dispatch and canvas input bindings.
use super::*;

impl ViewportSurface {
    pub(super) fn input_target(
        editor: &Editor,
        viewport: zipzi_editor_core::ViewportId,
        kind: zipzi_editor_core::ViewportKind,
    ) -> Option<crate::input_routing::InputTarget> {
        use crate::input_routing::{InputTarget, LocalTarget};
        let binding = editor
            .viewport_binding(viewport)
            .filter(|binding| binding.kind == kind)?;
        let mut target = InputTarget::new(LocalTarget::Viewport(binding));
        if kind == zipzi_editor_core::ViewportKind::Modeling
            && !editor.gesture_active_for(binding)
            && editor.viewport_interaction_binding() != Some(binding)
        {
            target.idle_escape = editor.active_tool().idle_escape_command();
        }
        Some(target)
    }

    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        let token = self.cancel_pointer();
        let _ = self.owner.update(cx, |owner, cx| {
            if token.is_some_and(|token| owner.editor.cancel_viewport_interaction(token)) {
                owner.redraw(cx);
            }
        });
    }
    pub(super) fn local(&self, p: Point<Pixels>) -> ViewportPoint {
        ViewportPoint {
            x: (p.x - self.bounds.origin.x).as_f32(),
            y: (p.y - self.bounds.origin.y).as_f32(),
        }
    }
    pub(super) fn input(
        &mut self,
        input: CanvasInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        (self.on_input)(self, input, window, cx);
    }
    pub(super) fn moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if benchmark::ignore_native_pointer_event() {
            return;
        }
        if window.has_active_dialog(cx) {
            self.cancel(cx);
            return;
        }
        if let Some(PanGesture {
            position,
            button,
            token,
        }) = self.navigation.pan
        {
            if self.owner.upgrade().is_none_or(|o| {
                let editor = &o.read(cx).editor;
                !editor.viewport_interaction_valid(token)
            }) {
                self.navigation.pan = None;
                self.sync_status_bar(cx);
                cx.notify();
                return;
            }
            if event.pressed_button != Some(button) {
                let _ = self.owner.update(cx, |owner, _| {
                    owner.editor.cancel_viewport_interaction(token)
                });
                self.navigation.pan = None;
                self.sync_status_bar(cx);
                cx.notify();
                return;
            }
            let delta = ViewportPoint {
                x: (event.position.x - position.x).as_f32(),
                y: (event.position.y - position.y).as_f32(),
            };
            if delta.x == 0. && delta.y == 0. {
                return;
            }
            self.navigation.pan = Some(PanGesture {
                position: event.position,
                button,
                token,
            });
            let _ = self.owner.update(cx, |owner, cx| {
                if !owner.modal {
                    let _ = owner.editor.pan_viewport(token.binding, delta);
                    owner.redraw_canvas(cx);
                }
            });
        } else {
            self.input(
                CanvasInput::Move {
                    point: self.local(event.position),
                    left_down: event.pressed_button == Some(MouseButton::Left),
                },
                window,
                cx,
            );
        }
    }
    pub(super) fn pointer_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .navigation
            .pan
            .is_some_and(|pan| pan.button == event.button)
        {
            if let Some(pan) = self.navigation.pan.take() {
                let _ = self.owner.update(cx, |owner, _| {
                    owner.editor.cancel_viewport_interaction(pan.token)
                });
            }
            self.sync_status_bar(cx);
            cx.notify();
        } else if self.navigation.pan.is_none() && event.button == MouseButton::Left {
            self.input(
                CanvasInput::Up {
                    point: self.local(event.position),
                },
                window,
                cx,
            );
        }
    }
    fn interaction_allowed(&self, window: &mut Window, cx: &mut App) -> bool {
        !window.has_active_dialog(cx)
            && self.owner.upgrade().is_some_and(|owner| {
                let state = owner.read(cx);
                !state.modal && !state.editor.is_busy()
            })
    }
    fn pointer_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.interaction_allowed(window, cx) {
            return;
        }
        window.focus(&self.focus, cx);
        if event.button == MouseButton::Middle || self.hand_tool(cx) {
            self.cancel(cx);
            let token = self
                .owner
                .update(cx, |owner, _| {
                    let binding = self.binding(&owner.editor)?;
                    owner.editor.begin_viewport_navigation(binding).ok()
                })
                .ok()
                .flatten();
            self.navigation.pan = token.map(|token| PanGesture {
                position: event.position,
                button: event.button,
                token,
            });
            self.sync_status_bar(cx);
            cx.notify();
        } else {
            self.input(
                CanvasInput::Down {
                    primary: event.modifiers.platform,
                    point: self.local(event.position),
                    shift: event.modifiers.shift,
                    alt: event.modifiers.alt,
                },
                window,
                cx,
            );
        }
    }
    pub(super) fn bind_input<E: StatefulInteractiveElement>(
        &self,
        element: E,
        capture: bool,
        cx: &mut Context<Self>,
    ) -> E {
        element
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pointer_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::pointer_down))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.pointer_up(event, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.pointer_up(event, window, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.scroll(event, window, cx);
            }))
            .on_pinch(cx.listener(|this, event: &PinchEvent, window, cx| {
                this.pinch(event, window, cx);
            }))
            .on_modifiers_changed(cx.listener(|this, _, _, cx| {
                if this.kind == zipzi_editor_core::ViewportKind::Modeling
                    && this
                        .owner
                        .upgrade()
                        .is_some_and(|owner| owner.read(cx).editor.manual_active())
                {
                    // Modifier-only presses must update the cursor and guidance
                    // without requiring mouse movement or changing the gesture.
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if !hovered && this.navigation.pan.is_none() {
                    this.input(CanvasInput::Leave, window, cx);
                }
            }))
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                    if !capture {
                        this.moved(event, window, cx);
                    }
                }),
            )
    }
}
