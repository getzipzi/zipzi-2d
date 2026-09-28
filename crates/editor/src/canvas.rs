//! Measured canvas, pointer ownership and overlay painting. Tool semantics live in core.
use super::*;
use editor_core::{CanvasInput, CanvasMetrics, CanvasPoint};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
};

pub struct CanvasView {
    pub(crate) owner: WeakEntity<EditorView>,
    pub(crate) focus: FocusHandle,
    bounds: Bounds<Pixels>,
    pan: Option<PanGesture>,
    space_held: bool,
    hand_tool: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy)]
struct PanGesture {
    position: Point<Pixels>,
    button: MouseButton,
}

impl CanvasView {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let focus = cx.focus_handle();
            let subscriptions = vec![
                cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
                cx.on_focus_out(&focus, window, |this, _, _, cx| {
                    this.space_held = false;
                    this.cancel(cx);
                }),
                cx.observe_window_visibility(window, |this, _, window, cx| {
                    if !window.is_visible() {
                        this.space_held = false;
                        this.cancel(cx);
                        let _ = this.owner.update(cx, |owner, cx| {
                            owner.editor.set_canvas_metrics(CanvasMetrics::default());
                            cx.notify();
                        });
                    } else {
                        cx.notify();
                    }
                }),
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.space_held = false;
                        this.cancel(cx);
                    }
                }),
            ];
            Self {
                owner,
                focus,
                bounds: Bounds::default(),
                pan: None,
                space_held: false,
                hand_tool: false,
                _subscriptions: subscriptions,
            }
        })
    }
    pub fn hand_tool(&self) -> bool {
        self.hand_tool
    }
    pub fn set_hand_tool(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.hand_tool = enabled;
        self.pan = None;
        cx.notify();
    }
    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.pan = None;
        let _ = self.owner.update(cx, |owner, cx| {
            owner.editor.cancel_gesture();
            owner.redraw(cx);
        });
    }
    fn local(&self, p: Point<Pixels>) -> CanvasPoint {
        CanvasPoint {
            x: (p.x - self.bounds.origin.x).as_f32(),
            y: (p.y - self.bounds.origin.y).as_f32(),
        }
    }
    fn input(&mut self, input: CanvasInput, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            self.cancel(cx);
            return;
        }
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() {
                return;
            }
            if !owner
                .editor
                .has_presented_frame(owner.frame.as_ref().map(|f| f.canvas.version))
            {
                // A revisited tab can retain geometry before its image arrives.
                // Never hit-test controls that the user cannot yet see.
                owner.editor.cancel_gesture();
                owner.redraw(cx);
                return;
            }
            match owner.editor.canvas_input(input) {
                Ok(changed) => {
                    if changed {
                        owner.redraw(cx);
                    } else {
                        cx.notify();
                    }
                }
                Err(e) => {
                    owner.edit_error = Some(e.to_string());
                    owner.redraw(cx);
                }
            }
        });
    }
    fn moved(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(PanGesture { position, button }) = self.pan {
            if event.pressed_button != Some(button) {
                self.pan = None;
                cx.notify();
                return;
            }
            let delta = CanvasPoint {
                x: (event.position.x - position.x).as_f32(),
                y: (event.position.y - position.y).as_f32(),
            };
            self.pan = Some(PanGesture {
                position: event.position,
                button,
            });
            let _ = self.owner.update(cx, |owner, cx| {
                if !owner.modal {
                    owner.editor.pan_by(delta);
                    owner.redraw(cx);
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
    fn pointer_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.pan.is_some_and(|pan| pan.button == event.button) {
            self.pan = None;
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
}
impl EventEmitter<PanelEvent> for CanvasView {}
impl Focusable for CanvasView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for CanvasView {
    fn panel_name(&self) -> &'static str {
        "Canvas"
    }
    fn closable(&self, _: &App) -> bool {
        false
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for CanvasView {
    fn title_bar(&self, _: &App) -> bool {
        false
    }
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}
impl Render for CanvasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let frame = state.frame.clone();
        let overlay = state
            .editor
            .overlay_for_frame(frame.as_ref().map(|f| f.canvas.version));
        let active = state.editor.active_id();
        let modal = state.modal || dialog;
        let error = state.render_error.clone();
        let capture = self.pan.is_some() || state.editor.gesture_active();
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let view = cx.entity();
        let measure = view.clone();
        let mouse = view.clone();
        let body = div()
            .id("canvas-body")
            .test_support()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(cx.theme().muted)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    if this.space_held || this.hand_tool {
                        this.cancel(cx);
                        this.pan = Some(PanGesture {
                            position: event.position,
                            button: MouseButton::Left,
                        });
                        cx.notify();
                        return;
                    }
                    this.input(
                        CanvasInput::Down {
                            point: this.local(event.position),
                            shift: event.modifiers.shift,
                        },
                        window,
                        cx,
                    );
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.cancel(cx);
                    this.pan = Some(PanGesture {
                        position: event.position,
                        button: MouseButton::Middle,
                    });
                    cx.notify();
                }),
            )
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
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta_y = match event.delta {
                    ScrollDelta::Pixels(p) => p.y.as_f32(),
                    ScrollDelta::Lines(p) => p.y * 16.0,
                };
                if delta_y.abs() < 0.1 {
                    return;
                }
                let step = if delta_y > 0.0 { 0.1 } else { -0.1 };
                let _ = this.owner.update(cx, |owner, cx| {
                    if owner.modal || owner.editor.is_busy() {
                        return;
                    }
                    owner.editor.zoom_by(step);
                    owner.redraw(cx);
                });
            }))
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                    if !capture {
                        this.moved(event, window, cx);
                    }
                }),
            )
            .children(frame.map(|frame| {
                let metrics = frame.canvas.transform.metrics;
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(metrics.width))
                    .h(px(metrics.height))
                    .child(preview::element(frame.image))
            }))
            .when(active.is_none(), |e| {
                e.child(
                    div()
                        .id("canvas-welcome")
                        .test_support()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("editor.welcome")),
                )
            })
            .children(error.map(|e| div().absolute().p_4().child(e)))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        measure.update(cx, |this, cx| {
                            let metrics = CanvasMetrics {
                                width: bounds.size.width.as_f32(),
                                height: bounds.size.height.as_f32(),
                                scale_factor: if window.is_visible() {
                                    window.scale_factor()
                                } else {
                                    0.
                                },
                            };
                            this.bounds = bounds;
                            let _ = this.owner.update(cx, |owner, cx| {
                                if owner.editor.canvas_metrics() != metrics {
                                    owner.editor.set_canvas_metrics(metrics);
                                    owner.redraw(cx);
                                }
                            });
                        });
                    },
                    move |bounds, _, window, _cx| {
                        let to_window = |p: CanvasPoint| bounds.origin + point(px(p.x), px(p.y));
                        for (a, b) in &overlay.lines {
                            let mut path = PathBuilder::stroke(px(1.));
                            path.move_to(to_window(*a));
                            path.line_to(to_window(*b));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, muted);
                            }
                        }
                        for vertex in &overlay.vertices {
                            let center = to_window(vertex.position);
                            let r = if vertex.hovered { px(4.) } else { px(3.) };
                            window.paint_quad(fill(
                                Bounds::new(center - point(r, r), size(r * 2., r * 2.)),
                                if vertex.selected { primary } else { foreground },
                            ));
                        }
                        if capture {
                            let mouse = mouse.clone();
                            window.on_mouse_event(
                                move |event: &MouseMoveEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Capture {
                                        mouse.update(cx, |this, cx| this.moved(event, window, cx));
                                        cx.stop_propagation();
                                    }
                                },
                            );
                            let view = view.clone();
                            window.on_mouse_event(
                                move |event: &MouseUpEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Capture {
                                        view.update(cx, |this, cx| {
                                            this.pointer_up(event, window, cx);
                                        });
                                        cx.stop_propagation();
                                    }
                                },
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(workspace::render_floating_toolbar(
                self, &owner, modal, window, cx,
            ))
            .child(workspace::render_floating_zoom_bar(
                self, &owner, modal, window, cx,
            ))
            .children(overlay.unavailable.map(|reason| {
                div()
                    .absolute()
                    .left_3()
                    .bottom_3()
                    .px_2p5()
                    .py_1()
                    .rounded_md()
                    .bg(cx.theme().warning.opacity(0.15))
                    .border_1()
                    .border_color(cx.theme().warning.opacity(0.4))
                    .text_xs()
                    .text_color(cx.theme().warning)
                    .child(reason)
            }));
        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context(if modal {
                "KasaneCanvas KasaneModal"
            } else {
                "KasaneCanvas"
            })
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "space" && !event.is_held {
                    this.space_held = true;
                    cx.notify();
                } else if event.keystroke.key == "h" && !event.is_held {
                    this.hand_tool = true;
                    this.pan = None;
                    let _ = this.owner.update(cx, |owner, cx| {
                        owner.editor.cancel_gesture();
                        owner.redraw(cx);
                    });
                    cx.notify();
                } else if event.keystroke.key == "v" && !event.is_held {
                    this.hand_tool = false;
                    this.pan = None;
                    let _ = this.owner.update(cx, |owner, cx| {
                        owner.editor.cancel_gesture();
                        owner.redraw(cx);
                    });
                    cx.notify();
                }
            }))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| {
                if event.keystroke.key == "space" {
                    this.space_held = false;
                    cx.notify();
                }
            }))
            .child(body)
            .into_any_element()
    }
}
