//! Pointer ownership, tool dispatch and canvas input bindings.
use super::*;

impl CanvasView {
    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        let token = self
            .interaction
            .take()
            .or_else(|| self.pan.map(|pan| pan.token));
        self.cancel_pointer();
        let _ = self.owner.update(cx, |owner, cx| {
            if token.is_some_and(|token| owner.editor.cancel_viewport_interaction(token)) {
                owner.redraw(cx);
            }
        });
    }
    pub(super) fn local(&self, p: Point<Pixels>) -> CanvasPoint {
        CanvasPoint {
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
        if matches!(input, CanvasInput::Down { .. }) {
            self.wheel_zoom = None;
        }
        if window.has_active_dialog(cx) {
            self.cancel(cx);
            return;
        }
        let owner = self.owner.clone();
        let _ = owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() {
                return;
            }
            if self
                .interaction
                .is_some_and(|token| !owner.editor.viewport_interaction_valid(token))
            {
                self.cancel_pointer();
                if !matches!(input, CanvasInput::Down { .. }) {
                    return;
                }
            }
            let Some(binding) = self
                .binding(&owner.editor)
                .filter(|binding| binding.kind == editor_core::ViewportKind::Modeling)
            else {
                return;
            };
            if matches!(input, CanvasInput::Up { .. }) && self.interaction.is_none() {
                return;
            }
            if !owner
                .editor
                .has_presented_frame(owner.frame.as_ref().map(|f| f.canvas.version))
            {
                // A revisited tab can retain geometry before its image arrives.
                // Never hit-test controls that the user cannot yet see.
                let editing = owner.editor.gesture_active();
                owner.editor.cancel_gesture();
                if editing {
                    owner.redraw(cx);
                } else {
                    // Layout can deliver hover/leave callbacks before the first
                    // image arrives. There is no document edit to reconcile.
                    owner.redraw_canvas(cx);
                }
                return;
            }
            let moving = matches!(input, CanvasInput::Move { .. } | CanvasInput::Leave);
            if matches!(input, CanvasInput::Down { .. }) {
                match owner.editor.begin_viewport_interaction(
                    binding,
                    editor_core::ViewportInteractionKind::Modeling,
                ) {
                    Ok(token) => self.interaction = Some(token),
                    Err(_) => return,
                }
            }
            let result = if let Some(token) = self.interaction {
                owner.editor.canvas_interaction_input(token, input)
            } else {
                owner.editor.canvas_input_for(binding, input)
            };
            if matches!(input, CanvasInput::Up { .. }) || result.is_err() {
                if let Some(token) = self.interaction.take() {
                    if result.is_err() {
                        owner.editor.cancel_viewport_interaction(token);
                    } else {
                        owner.editor.finish_viewport_interaction(token);
                    }
                }
            }
            match result {
                Ok(changed) => {
                    if matches!(input, CanvasInput::Down { .. }) {
                        owner.edit_error = None;
                    }
                    if changed {
                        if moving {
                            owner.redraw_canvas(cx);
                        } else {
                            owner.redraw(cx);
                        }
                    } else {
                        if moving {
                            owner.notify_canvas(cx);
                        } else {
                            cx.notify();
                        }
                    }
                }
                Err(e) => {
                    owner.edit_error = Some(e.to_string());
                    owner.redraw(cx);
                }
            }
        });
    }
    pub(super) fn moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if benchmark::ignore_native_mouse_move() {
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
        }) = self.pan
        {
            if self.owner.upgrade().is_none_or(|o| {
                let editor = &o.read(cx).editor;
                !editor.viewport_interaction_valid(token)
            }) {
                self.pan = None;
                cx.notify();
                return;
            }
            if event.pressed_button != Some(button) {
                let _ = self.owner.update(cx, |owner, _| {
                    owner.editor.finish_viewport_interaction(token)
                });
                self.pan = None;
                cx.notify();
                return;
            }
            let delta = CanvasPoint {
                x: (event.position.x - position.x).as_f32(),
                y: (event.position.y - position.y).as_f32(),
            };
            if delta.x == 0. && delta.y == 0. {
                return;
            }
            self.pan = Some(PanGesture {
                position: event.position,
                button,
                token,
            });
            let _ = self.owner.update(cx, |owner, cx| {
                if !owner.modal {
                    owner.editor.pan_by(delta);
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
        if self.pan.is_some_and(|pan| pan.button == event.button) {
            if let Some(pan) = self.pan.take() {
                let _ = self.owner.update(cx, |owner, _| {
                    owner.editor.finish_viewport_interaction(pan.token)
                });
            }
            cx.notify();
        } else if self.pan.is_none() && event.button == MouseButton::Left {
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
                    owner
                        .editor
                        .begin_viewport_interaction(
                            binding,
                            editor_core::ViewportInteractionKind::Navigation,
                        )
                        .ok()
                })
                .ok()
                .flatten();
            self.pan = token.map(|token| PanGesture {
                position: event.position,
                button: event.button,
                token,
            });
            cx.notify();
        } else {
            self.input(
                CanvasInput::Down {
                    point: self.local(event.position),
                    shift: event.modifiers.shift,
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
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if !hovered && this.pan.is_none() {
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
