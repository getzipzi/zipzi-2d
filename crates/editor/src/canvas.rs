//! Measured canvas, pointer ownership and overlay painting. Tool semantics live in core.
use super::*;
use editor_core::{CanvasInput, CanvasMetrics, CanvasPoint, CommandId, EditorCommand, ToolId};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
    menu::{ContextMenuExt, PopupMenuItem},
};

pub struct CanvasView {
    pub(crate) owner: WeakEntity<EditorView>,
    pub(crate) focus: FocusHandle,
    pub(crate) toolbar: workspace::toolbar::ToolbarRegistry,
    overlay_renderer: std::rc::Rc<std::cell::RefCell<CanvasOverlay>>,
    bounds: Bounds<Pixels>,
    pan: Option<PanGesture>,
    space_held: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct CanvasOverlay {
    device: Option<metal::Device>,
    renderer: Option<editor_render::MetalOverlayRenderer>,
    error: Option<String>,
}
impl CanvasOverlay {
    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        overlay: &editor_core::OverlaySnapshot,
        style: editor_render::OverlayStyle,
        pass: editor_render::OverlayPass,
        window: &mut Window,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(benchmark::metal_mesh_overlay(), "GPUI diagnostic override");
        let gpu = window
            .metal_context()
            .ok_or_else(|| anyhow::anyhow!("Metal context unavailable"))?;
        let scale = window.scale_factor();
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return Ok(());
        }
        let origin = [bounds.origin.x.as_f32(), bounds.origin.y.as_f32()];
        let texture_origin = origin.map(|v| (v * scale).floor());
        let extent = [
            ((bounds.right().as_f32() * scale).ceil() - texture_origin[0]) as u32,
            ((bounds.bottom().as_f32() * scale).ceil() - texture_origin[1]) as u32,
        ];
        let viewport = editor_render::OverlayViewport {
            extent,
            scale,
            origin,
        };
        if let Some(image) = self
            .renderer(&gpu.device)?
            .draw_pass(&gpu.queue, viewport, overlay, style, pass)?
        {
            // Preserve physical pixel size when logical bounds are fractional.
            window.paint_surface(
                Bounds::new(
                    point(px(texture_origin[0] / scale), px(texture_origin[1] / scale)),
                    size(px(extent[0] as f32 / scale), px(extent[1] as f32 / scale)),
                ),
                SurfaceSource::MetalTexture(image),
            );
        }
        Ok(())
    }

    fn renderer(
        &mut self,
        device: &metal::Device,
    ) -> anyhow::Result<&mut editor_render::MetalOverlayRenderer> {
        if self
            .device
            .as_ref()
            .is_none_or(|old| !std::ptr::eq(old.as_ref(), device.as_ref()))
        {
            self.device = Some(device.clone());
            self.renderer = None;
            self.error = None;
            match editor_render::MetalOverlayRenderer::new(device.clone()) {
                Ok(renderer) => self.renderer = Some(renderer),
                Err(error) => {
                    eprintln!("Mesh overlay initialization failed; using GPUI: {error}");
                    self.error = Some(error.to_string());
                }
            }
        }
        self.renderer.as_mut().ok_or_else(|| {
            anyhow::anyhow!("{}", self.error.as_deref().unwrap_or("Overlay unavailable"))
        })
    }
}

#[derive(Clone, Copy)]
struct PanGesture {
    position: Point<Pixels>,
    button: MouseButton,
    document: Option<editor_core::DocumentId>,
}

impl CanvasView {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let focus = cx.focus_handle();
            let subscriptions = vec![
                cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
                cx.on_focus_out(&focus, window, |this, _, window, cx| {
                    this.space_held = false;
                    window.suppress_key_repeats("space", false);
                    this.cancel(cx);
                }),
                cx.observe_window_visibility(window, |this, _, window, cx| {
                    if !window.is_visible() {
                        this.space_held = false;
                        window.suppress_key_repeats("space", false);
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
                        window.suppress_key_repeats("space", false);
                        this.cancel(cx);
                    }
                }),
            ];
            Self {
                owner,
                focus,
                toolbar: workspace::toolbar::ToolbarRegistry::with_defaults(),
                overlay_renderer: Default::default(),
                bounds: Bounds::default(),
                pan: None,
                space_held: false,
                _subscriptions: subscriptions,
            }
        })
    }
    pub(crate) fn benchmark_center(&self) -> Option<Point<Pixels>> {
        (self.bounds.size.width > px(0.) && self.bounds.size.height > px(0.))
            .then(|| self.bounds.center())
    }
    pub(crate) fn benchmark_bounds(&self) -> Option<Bounds<Pixels>> {
        self.benchmark_center().map(|_| self.bounds)
    }
    pub fn active_tool(&self, cx: &App) -> ToolId {
        self.owner
            .upgrade()
            .map_or(ToolId::Select, |owner| owner.read(cx).editor.active_tool())
    }
    pub fn hand_tool(&self, cx: &App) -> bool {
        self.active_tool(cx) == ToolId::Hand
    }
    pub fn cancel_pointer(&mut self) {
        self.pan = None;
    }
    pub(crate) fn set_temporary_hand(
        &mut self,
        held: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.space_held == held {
            return;
        }
        self.space_held = held;
        window.suppress_key_repeats("space", held);
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
            let moving = matches!(input, CanvasInput::Move { .. } | CanvasInput::Leave);
            match owner.editor.canvas_input(input) {
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
    fn moved(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if benchmark::ignore_native_mouse_move() {
            return;
        }
        if let Some(PanGesture {
            position,
            button,
            document,
        }) = self.pan
        {
            if self
                .owner
                .upgrade()
                .is_none_or(|o| o.read(cx).editor.active_id() != document)
            {
                self.pan = None;
                cx.notify();
                return;
            }
            if event.pressed_button != Some(button) {
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
                document,
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
        let mut overlay = state
            .editor
            .overlay_for_frame(frame.as_ref().map(|f| f.canvas.version));
        if benchmark::hide_mesh_edges() {
            overlay.lines.clear();
        }
        let vertex_notice = overlay.unavailable.clone();
        let active_handle = overlay.active_handle;
        let interaction_label = overlay.interaction_label.clone().or_else(|| {
            active_handle.map(|handle| {
                use editor_core::DeformerHandle as H;
                t!(match handle {
                    H::Move => "editor.deformer_move_hint",
                    H::Rotate => "editor.deformer_rotate_hint",
                    H::Scale(_) | H::RotationScale => "editor.deformer_scale_hint",
                })
                .into_owned()
            })
        });
        let target_label = overlay.target_label.as_ref().map(|label| {
            if label == "Base shape" {
                t!("editor.edit_base").into_owned()
            } else if let Some(keys) = label.strip_prefix("Keyform · ") {
                format!("{} · {}", t!("editor.edit_keyform"), keys)
            } else {
                label.clone()
            }
        });
        let hand_tool = state.editor.active_tool() == ToolId::Hand;
        let active = state.editor.active_id();
        let modal = state.modal || dialog;
        let error = state.render_error.clone();
        let capture = self.pan.is_some() || state.editor.gesture_active();
        let palette = crate::canvas_overlay::Palette::new(cx.theme().is_dark());
        let primary = palette.selected;
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let background = cx.theme().background;
        let view = cx.entity();
        let measure = view.clone();
        let mouse = view.clone();
        let overlay_renderer = self.overlay_renderer.clone();
        let menu_owner = owner.downgrade();
        let menu_canvas = cx.weak_entity();
        let body = div()
            .id("canvas-body")
            .test_support()
            .when(!modal && !hand_tool && active_handle.is_some(), |e| {
                use editor_core::DeformerHandle as H;
                let cursor = match active_handle.unwrap() {
                    H::Move => CursorStyle::OpenHand,
                    H::Rotate => CursorStyle::Crosshair,
                    H::Scale(1 | 5) => CursorStyle::ResizeUpDown,
                    H::Scale(3 | 7) => CursorStyle::ResizeLeftRight,
                    H::Scale(0 | 4) => CursorStyle::ResizeUpLeftDownRight,
                    H::Scale(_) => CursorStyle::ResizeUpRightDownLeft,
                    H::RotationScale => CursorStyle::Crosshair,
                };
                e.cursor(cursor)
            })
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(cx.theme().muted)
            .when(!modal && self.pan.is_some(), |e| e.cursor_grabbing())
            .when(
                !modal && self.pan.is_none() && (self.space_held || hand_tool),
                |e| e.cursor_grab(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if window.has_active_dialog(cx)
                        || this
                            .owner
                            .upgrade()
                            .is_none_or(|o| o.read(cx).modal || o.read(cx).editor.is_busy())
                    {
                        return;
                    }
                    window.focus(&this.focus, cx);
                    if this.space_held || this.hand_tool(cx) {
                        this.cancel(cx);
                        this.pan = Some(PanGesture {
                            position: event.position,
                            button: MouseButton::Left,
                            document: this
                                .owner
                                .upgrade()
                                .and_then(|o| o.read(cx).editor.active_id()),
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
                    if window.has_active_dialog(cx)
                        || this
                            .owner
                            .upgrade()
                            .is_none_or(|o| o.read(cx).modal || o.read(cx).editor.is_busy())
                    {
                        return;
                    }
                    window.focus(&this.focus, cx);
                    this.cancel(cx);
                    this.pan = Some(PanGesture {
                        position: event.position,
                        button: MouseButton::Middle,
                        document: this
                            .owner
                            .upgrade()
                            .and_then(|o| o.read(cx).editor.active_id()),
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
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                if window.has_active_dialog(cx)
                    || this.pan.is_some()
                    || event.touch_phase == TouchPhase::Cancelled
                {
                    return;
                }
                let delta = event.delta.pixel_delta(px(16.));
                let delta = CanvasPoint {
                    x: delta.x.as_f32(),
                    y: delta.y.as_f32(),
                };
                if !delta.x.is_finite() || !delta.y.is_finite() || (delta.x == 0. && delta.y == 0.)
                {
                    return;
                }
                // Precise scrolling follows both trackpad axes, including the
                // platform's natural-scroll direction and momentum. A mouse
                // wheel or Ctrl/Cmd-scroll retains cursor-anchored zooming.
                let pan =
                    event.delta.precise() && !event.modifiers.control && !event.modifiers.platform;
                let point = this.local(event.position);
                let _ = this.owner.update(cx, |owner, cx| {
                    if owner.modal || owner.editor.is_busy() || owner.editor.gesture_active() {
                        return;
                    }
                    let before = owner.editor.view();
                    if pan {
                        owner.editor.pan_by(delta);
                    } else {
                        owner.editor.zoom_at(delta.y * (0.1 / 16.), Some(point));
                    }
                    if owner.editor.view().zoom != before.zoom
                        || owner.editor.view().pan != before.pan
                    {
                        owner.redraw_canvas(cx);
                    }
                    cx.stop_propagation();
                });
            }))
            .on_pinch(cx.listener(|this, event: &PinchEvent, window, cx| {
                if window.has_active_dialog(cx)
                    || this.pan.is_some()
                    || event.phase == TouchPhase::Cancelled
                    || !event.delta.is_finite()
                    || event.delta == 0.
                    || event.delta <= -1.
                {
                    return;
                }
                let point = this.local(event.position);
                let _ = this.owner.update(cx, |owner, cx| {
                    if owner.modal || owner.editor.is_busy() || owner.editor.gesture_active() {
                        return;
                    }
                    let before = owner.editor.view();
                    // Native magnification is relative to the current zoom.
                    owner.editor.zoom_at(before.zoom * event.delta, Some(point));
                    if owner.editor.view().zoom != before.zoom
                        || owner.editor.view().pan != before.pan
                    {
                        owner.redraw_canvas(cx);
                    }
                    cx.stop_propagation();
                });
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
                        let result = overlay_renderer.borrow_mut().paint(
                            bounds,
                            &overlay,
                            palette.metal(),
                            editor_render::OverlayPass::Edges,
                            window,
                        );
                        // Preserve editing visibility if shader/device setup is unavailable.
                        if result.is_err() {
                            let to_window =
                                |p: CanvasPoint| bounds.origin + point(px(p.x), px(p.y));
                            if benchmark::batch_mesh_edges() {
                                mesh_edges::paint_edges(&overlay.lines, bounds.origin, |path| {
                                    window.paint_path(path, palette.mesh.opacity(0.45));
                                });
                            } else {
                                for (a, b) in &overlay.lines {
                                    let mut path = PathBuilder::stroke(px(1.));
                                    path.move_to(to_window(*a));
                                    path.line_to(to_window(*b));
                                    if let Ok(path) = path.build() {
                                        window.paint_path(path, palette.mesh.opacity(0.45));
                                    }
                                }
                            }
                        }
                        crate::canvas_overlay::paint_lines(&overlay, bounds, palette, window);
                        let marker_result = overlay_renderer.borrow_mut().paint(
                            bounds,
                            &overlay,
                            palette.metal(),
                            editor_render::OverlayPass::Vertices,
                            window,
                        );
                        crate::canvas_overlay::paint_markers(
                            &overlay,
                            bounds.origin,
                            palette,
                            marker_result.is_err(),
                            window,
                        );
                        if let Some((a, b)) = overlay.marquee {
                            let rect = Bounds::new(
                                bounds.origin + point(px(a.x.min(b.x)), px(a.y.min(b.y))),
                                size(px((b.x - a.x).abs()), px((b.y - a.y).abs())),
                            );
                            window.paint_quad(fill(rect, primary.opacity(0.12)));
                            let p = |x, y| CanvasPoint { x, y };
                            mesh_edges::paint_edges(
                                &[
                                    (a, p(b.x, a.y)),
                                    (p(b.x, a.y), b),
                                    (b, p(a.x, b.y)),
                                    (p(a.x, b.y), a),
                                ],
                                bounds.origin,
                                |path| window.paint_path(path, primary),
                            );
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
            .child(workspace::render_floating_toolbar(self, modal, cx))
            .child(workspace::render_floating_zoom_bar(&owner, modal, cx))
            .child(workspace::deformer_bar::render_deformer_bar(
                self, modal, cx,
            ))
            .children(interaction_label.map(|label| {
                div()
                    .absolute()
                    .left(px(52.))
                    .top(px(48.))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(background)
                    .text_color(foreground)
                    .text_xs()
                    .child(label)
            }))
            .children(vertex_notice.map(|reason| {
                div()
                    .id("canvas-vertex-notice")
                    .test_support()
                    .absolute()
                    .right_3()
                    .bottom(px(56.))
                    .max_w(px(360.))
                    .px_2p5()
                    .py_1()
                    .rounded_md()
                    .bg(cx.theme().warning.opacity(0.15))
                    .border_1()
                    .border_color(cx.theme().warning.opacity(0.4))
                    .text_xs()
                    .text_color(cx.theme().warning)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(reason)
            }))
            .children(target_label.map(|label| {
                div()
                    .absolute()
                    .left_2()
                    .bottom_2()
                    .text_xs()
                    .text_color(muted)
                    .child(label)
            }))
            .context_menu(move |mut menu, window, cx| {
                let Some(owner) = menu_owner.upgrade() else {
                    return menu;
                };
                let Some(canvas) = menu_canvas.upgrade() else {
                    return menu;
                };
                if window.has_active_dialog(cx) {
                    return menu;
                }
                let state = owner.read(cx);
                if state.modal || state.editor.is_busy() {
                    return menu;
                }
                let point = canvas.read(cx).local(window.mouse_position());
                let candidates = state
                    .editor
                    .pick_candidates(point, state.frame.as_ref().map(|f| f.canvas.version));
                let document = state.editor.active_id();
                let visibility = state.editor.deformer_visibility();
                if candidates.is_empty() {
                    menu = menu
                        .item(PopupMenuItem::new(t!("editor.no_canvas_objects")).disabled(true));
                }
                for candidate in candidates {
                    let owner = owner.downgrade();
                    let canvas = canvas.downgrade();
                    menu =
                        menu.item(PopupMenuItem::new(candidate.label).on_click(
                            move |_, window, cx| {
                                let Some(owner) = owner.upgrade() else {
                                    return;
                                };
                                owner.update(cx, |owner, cx| {
                                    if owner.editor.active_id() != document
                                        || owner.modal
                                        || owner.editor.is_busy()
                                    {
                                        return;
                                    }
                                    let result =
                                        owner.editor.execute(EditorCommand::SelectCanvasObjects(
                                            vec![candidate.object.clone()],
                                        ));
                                    owner.complete_edit(result, cx);
                                });
                                if let Some(canvas) = canvas.upgrade() {
                                    let focus = canvas.read(cx).focus.clone();
                                    // Let menu dismissal finish before returning keyboard input.
                                    window.defer(cx, move |window, cx| window.focus(&focus, cx));
                                }
                            },
                        ));
                }
                menu = menu.separator();
                for (command, checked) in [
                    (CommandId::ToggleAncestorDeformers, visibility.ancestors),
                    (CommandId::ToggleDeformers, visibility.all),
                ] {
                    let owner = owner.downgrade();
                    let canvas = canvas.downgrade();
                    menu = menu.item(
                        PopupMenuItem::new(t!(command.spec().label).into_owned())
                            .checked(checked)
                            .on_click(move |_, window, cx| {
                                if let Some(owner) = owner.upgrade() {
                                    owner.update(cx, |owner, cx| {
                                        if owner.editor.active_id() == document {
                                            owner.run_command(command, window, cx);
                                        }
                                    });
                                }
                                if let Some(canvas) = canvas.upgrade() {
                                    let focus = canvas.read(cx).focus.clone();
                                    window.defer(cx, move |window, cx| window.focus(&focus, cx));
                                }
                            }),
                    );
                }
                menu
            });
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
            .child(body)
            .into_any_element()
    }
}
