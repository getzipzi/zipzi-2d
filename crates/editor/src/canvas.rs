//! Measured canvas, pointer ownership and overlay painting. Tool semantics live in core.
mod context_menu;
mod deformer_bar;
mod input;
mod painting;
use painting::CanvasOverlay;
mod navigation;
pub(crate) mod toolbar;
mod zoom_bar;

use super::*;
use editor_core::{
    CanvasInput, CanvasMetrics, CanvasPoint, CommandId, EditorCommand, ToolId, WorkspaceKind,
};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
    menu::{ContextMenuExt, PopupMenuItem},
};

pub struct CanvasView {
    pub(crate) owner: WeakEntity<EditorView>,
    pub(crate) focus: FocusHandle,
    viewport_slots: [editor_core::ViewportId; 2],
    pub(crate) toolbar: toolbar::ToolbarRegistry,
    overlay_renderer: std::rc::Rc<std::cell::RefCell<CanvasOverlay>>,
    bounds: Bounds<Pixels>,
    pan: Option<PanGesture>,
    interaction: Option<editor_core::ViewportInteractionToken>,
    wheel_zoom: Option<navigation::WheelZoom>,
    zoom_frame_pending: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy)]
struct PanGesture {
    position: Point<Pixels>,
    button: MouseButton,
    token: editor_core::ViewportInteractionToken,
}

impl CanvasView {
    pub fn new(
        owner: WeakEntity<EditorView>,
        viewport_slots: [editor_core::ViewportId; 2],
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let focus = cx.focus_handle();
            let subscriptions = vec![
                cx.observe(&owner.upgrade().unwrap(), |this, owner, cx| {
                    let state = owner.read(cx);
                    if state.modal
                        || this.interaction.is_some_and(|token| {
                            !state.editor.viewport_interaction_valid(token)
                                || !state.editor.gesture_active()
                        })
                        || this
                            .pan
                            .is_some_and(|pan| !state.editor.viewport_interaction_valid(pan.token))
                    {
                        if let Some(token) = this.cancel_pointer() {
                            owner.update(cx, |owner, _| {
                                owner.editor.finish_viewport_interaction(token);
                            });
                        }
                    }
                    if this
                        .wheel_zoom
                        .as_ref()
                        .is_some_and(|zoom| !zoom.valid(owner.read(cx), this.bounds))
                    {
                        this.wheel_zoom = None;
                    }
                    cx.notify();
                }),
                cx.on_focus_in(&focus, window, |this, _, cx| {
                    let id = cx.entity_id();
                    let owner = this.owner.clone();
                    _ = owner.update(cx, |owner, _| {
                        if let Some(dock) = &mut owner.dock {
                            dock.activate_canvas(id);
                        }
                    });
                }),
                cx.on_focus_out(&focus, window, |this, _, _, cx| {
                    this.cancel(cx);
                }),
                cx.observe_window_visibility(window, |this, _, window, cx| {
                    if !window.is_visible() {
                        this.cancel(cx);
                        let _ = this.owner.update(cx, |owner, cx| {
                            if let Some(document) = owner.editor.active_id() {
                                let _ = owner.editor.end_editing(
                                    document,
                                    editor_core::EditingLifecycle::WindowDeactivated,
                                );
                            }
                            for viewport in owner.viewport_slots {
                                owner.editor.set_viewport_visible(viewport, false);
                            }
                            owner.editor.set_canvas_metrics(CanvasMetrics::default());
                            owner.redraw(cx);
                            cx.notify();
                        });
                    } else {
                        let _ = this.owner.update(cx, |owner, _| {
                            for viewport in owner.viewport_slots {
                                owner.editor.set_viewport_visible(viewport, true);
                            }
                        });
                        cx.notify();
                    }
                }),
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.cancel(cx);
                        let _ = this.owner.update(cx, |owner, cx| {
                            if let Some(document) = owner.editor.active_id() {
                                let _ = owner.editor.end_editing(
                                    document,
                                    editor_core::EditingLifecycle::WindowDeactivated,
                                );
                            }
                            owner.redraw(cx);
                        });
                    }
                }),
            ];
            Self {
                owner,
                viewport_slots,
                focus,
                toolbar: toolbar::ToolbarRegistry::with_defaults(),
                overlay_renderer: Default::default(),
                bounds: Bounds::default(),
                pan: None,
                interaction: None,
                wheel_zoom: None,
                zoom_frame_pending: false,
                _subscriptions: subscriptions,
            }
        })
    }
    pub(crate) fn binding(&self, editor: &Editor) -> Option<editor_core::ViewportBinding> {
        editor.viewport_binding(self.viewport_slots[editor.workspace().index()])
    }
    pub(crate) fn command(
        &self,
        editor: &Editor,
        id: CommandId,
        source: editor_core::CommandSource,
    ) -> editor_core::Command {
        self.binding(editor)
            .map(|binding| editor.command_for_viewport(id, binding, source))
            .unwrap_or(editor_core::Command {
                id,
                target: editor_core::CommandTarget::Unavailable,
                source,
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
    pub fn cancel_pointer(&mut self) -> Option<editor_core::ViewportInteractionToken> {
        let token = self
            .interaction
            .take()
            .or_else(|| self.pan.map(|pan| pan.token));
        self.wheel_zoom = None;
        self.pan = None;
        token
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
}
impl Panel for CanvasView {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("dock.canvas")
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
        let modeling = state.editor.workspace() == WorkspaceKind::Modeling;
        let frame = modeling.then(|| state.frame.clone()).flatten();
        let image = if modeling {
            frame
                .as_ref()
                .map(|f| (f.image.clone(), f.canvas.transform.metrics))
        } else {
            state
                .animation_frame
                .as_ref()
                .filter(|f| state.editor.can_display_animation_image(f.state.identity))
                .map(|f| (f.image.clone(), f.state.transform.metrics))
        };
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
        if modal {
            self.cancel_pointer();
        }
        let error = state.render_error.clone().or_else(|| {
            (!modeling)
                .then(|| {
                    state.editor.animation().and_then(|a| {
                        a.error
                            .as_ref()
                            .map(crate::panels::animation_feedback::failure_text)
                    })
                })
                .flatten()
        });
        let capture = self.pan.is_some() || state.editor.gesture_active();
        let palette = crate::canvas_overlay::Palette::new(cx.theme().is_dark());
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let background = cx.theme().background;
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
            .when(!modal && self.pan.is_none() && hand_tool, |e| {
                e.cursor_grab()
            })
            .map(|element| self.bind_input(element, capture, cx))
            .children(image.map(|(image, metrics)| {
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(metrics.width))
                    .h(px(metrics.height))
                    .child(preview::element(image))
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
            .child(self.overlay_element(overlay, palette, capture, cx))
            .child(toolbar::CanvasToolbar {
                owner: owner.clone(),
                canvas: cx.entity(),
                modal,
                items: self.toolbar.items(),
            })
            .when(modeling, |e| {
                e.child(deformer_bar::render_deformer_bar(self, modal, cx))
            })
            .children(interaction_label.map(|label| {
                div()
                    .absolute()
                    .left(px(48.))
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
                    .bottom_3()
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
            .context_menu(move |menu, window, cx| {
                Self::object_menu(&menu_owner, &menu_canvas, menu, window, cx)
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

impl CanvasView {
    pub(crate) fn header_state(
        panel: &Entity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(i32, [bool; 3])> {
        let dialog = window.has_active_dialog(cx);
        panel.read(cx).owner.upgrade().map(|owner| {
            let owner = owner.read(cx);
            zoom_bar::state(owner, panel.read(cx), owner.modal || dialog)
        })
    }
}

impl crate::docking::header::PanelHeader for CanvasView {
    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let modal = owner.read(cx).modal || window.has_active_dialog(cx);
        zoom_bar::CanvasZoomBar {
            owner,
            canvas: cx.entity(),
            modal,
        }
        .into_any_element()
    }

    fn header_min_width(&self) -> Pixels {
        px(126.)
    }
}
