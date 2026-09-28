//! Measured canvas, pointer ownership and overlay painting. Tool semantics live in core.
use super::*;
use editor_core::{CanvasInput, CanvasMetrics, CanvasPoint};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
};

pub struct CanvasView {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
    pan: Option<Point<Pixels>>,
    _subscriptions: Vec<Subscription>,
}
impl CanvasView {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let focus = cx.focus_handle();
            let subscriptions = vec![
                cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
                cx.on_focus_out(&focus, window, |this, _, _, cx| this.cancel(cx)),
                cx.observe_window_visibility(window, |this, _, window, cx| {
                    if !window.is_visible() {
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
                        this.cancel(cx);
                    }
                }),
            ];
            Self {
                owner,
                focus,
                bounds: Bounds::default(),
                pan: None,
                _subscriptions: subscriptions,
            }
        })
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
        if let Some(previous) = self.pan {
            if event.pressed_button != Some(MouseButton::Middle) {
                self.pan = None;
                return;
            }
            let delta = CanvasPoint {
                x: (event.position.x - previous.x).as_f32(),
                y: (event.position.y - previous.y).as_f32(),
            };
            self.pan = Some(event.position);
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
        let tabs = state.editor.documents();
        let error = state.render_error.clone();
        let capture = self.pan.is_some() || state.editor.gesture_active();
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let view = cx.entity();
        let measure = view.clone();
        let mouse = view.clone();
        let body = div()
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
                    this.pan = Some(event.position);
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.input(
                        CanvasInput::Up {
                            point: this.local(event.position),
                        },
                        window,
                        cx,
                    )
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, _, _, cx| {
                    this.pan = None;
                    cx.notify();
                }),
            )
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
                e.child(div().p_4().child(t!("editor.welcome")))
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
                                            if event.button == MouseButton::Middle {
                                                this.pan = None;
                                                cx.notify();
                                            } else if event.button == MouseButton::Left {
                                                this.input(
                                                    CanvasInput::Up {
                                                        point: this.local(event.position),
                                                    },
                                                    window,
                                                    cx,
                                                );
                                            }
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
            );
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
            .child(
                div()
                    .id("document-tabs")
                    .h(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .gap_1()
                    .overflow_x_scroll()
                    .children(tabs.into_iter().map(|document| {
                        let id = document.id;
                        let owner = owner.clone();
                        Button::new(SharedString::from(format!("tab-{id:?}")))
                            .small()
                            .disabled(modal)
                            .label(format!(
                                "{}{}",
                                document
                                    .title
                                    .unwrap_or_else(|| t!("editor.untitled_project").into_owned()),
                                if document.modified { " •" } else { "" }
                            ))
                            .when(active == Some(id), |b| b.primary())
                            .on_click(move |_, window, cx| {
                                owner.update(cx, |this, cx| this.activate_document(id, window, cx))
                            })
                    })),
            )
            .child(body)
            .children(
                overlay
                    .unavailable
                    .map(|reason| div().text_xs().p_1().child(reason)),
            )
            .into_any_element()
    }
}
