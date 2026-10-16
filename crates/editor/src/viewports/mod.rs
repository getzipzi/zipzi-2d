//! Concrete viewport panels sharing measured surfaces and navigation.
macro_rules! viewport_panel {
    ($name:ident, $label:literal) => {
        impl EventEmitter<gpui_kit::component::dock::PanelEvent> for $name {}
        impl Focusable for $name {
            fn focus_handle(&self, cx: &App) -> FocusHandle {
                self.surface.read(cx).focus.clone()
            }
        }
        impl gpui_kit::component::dock::BasePanel for $name {
            fn panel_name(&self) -> &'static str {
                stringify!($name)
            }
            fn closable(&self, _: &App) -> bool {
                false
            }
        }
        impl gpui_kit::component::dock::Panel for $name {
            fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                t!($label)
            }
            fn inner_padding(&self, _: &App) -> bool {
                false
            }
        }
        impl Render for $name {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full().child(self.surface.clone())
            }
        }
        impl $name {
            pub(crate) fn header_state(
                panel: &Entity<Self>,
                window: &mut Window,
                cx: &mut App,
            ) -> Option<(i32, [bool; 3])> {
                ViewportSurface::header_state(&panel.read(cx).surface.clone(), window, cx)
            }
        }
        impl crate::docking::header::PanelHeader for $name {
            fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
                let surface = self.surface.read(cx);
                let Some(owner) = surface.owner.upgrade() else {
                    return div().into_any_element();
                };
                let modal = owner.read(cx).modal || window.has_active_dialog(cx);
                zoom_bar::CanvasZoomBar {
                    owner,
                    canvas: self.surface.clone(),
                    modal,
                }
                .into_any_element()
            }
            fn header_min_width(&self) -> Pixels {
                px(126.)
            }
        }
    };
}

mod context_menu;
mod deformer_bar;
mod input;
mod painting;
use painting::CanvasOverlay;
mod animation;
#[cfg(any(test, feature = "test-support"))]
mod test_scene;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use test_scene::ViewportScene;
mod modeling;
mod navigation;
pub(crate) mod toolbar;
pub(crate) mod zoom_bar;
pub use animation::AnimationViewport;
pub use modeling::ModelingViewport;

use super::*;
use editor_core::{CanvasInput, CanvasMetrics, CanvasPoint, CommandId, EditorCommand, ToolId};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    menu::{ContextMenuExt, PopupMenuItem},
};

pub struct ViewportSurface {
    pub(crate) owner: WeakEntity<EditorView>,
    pub(crate) focus: FocusHandle,
    pub(crate) viewport: editor_core::ViewportId,
    pub(crate) kind: editor_core::ViewportKind,
    content: fn(&EditorView, editor_core::ViewportId) -> SurfaceContent,
    on_input: fn(&mut Self, CanvasInput, &mut Window, &mut Context<Self>),
    object_menu: Option<
        fn(
            &WeakEntity<EditorView>,
            &WeakEntity<Self>,
            gpui_kit::component::menu::PopupMenu,
            &mut Window,
            &mut Context<gpui_kit::component::menu::PopupMenu>,
        ) -> gpui_kit::component::menu::PopupMenu,
    >,
    pub(crate) toolbar: toolbar::ToolbarRegistry,
    overlay_renderer: std::rc::Rc<std::cell::RefCell<CanvasOverlay>>,
    bounds: Bounds<Pixels>,
    navigation: navigation::ViewportNavigation,
    interaction: Option<editor_core::ViewportInteractionToken>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy)]
struct PanGesture {
    position: Point<Pixels>,
    button: MouseButton,
    token: editor_core::ViewportInteractionToken,
}

impl ViewportSurface {
    fn new(
        owner: WeakEntity<EditorView>,
        viewport: editor_core::ViewportId,
        kind: editor_core::ViewportKind,
        content: fn(&EditorView, editor_core::ViewportId) -> SurfaceContent,
        on_input: fn(&mut Self, CanvasInput, &mut Window, &mut Context<Self>),
        toolbar: toolbar::ToolbarRegistry,
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
                            .navigation
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
                        .navigation
                        .wheel_zoom
                        .as_ref()
                        .is_some_and(|zoom| !zoom.valid(owner.read(cx), this.bounds))
                    {
                        this.navigation.wheel_zoom = None;
                    }
                    cx.notify();
                }),
                cx.on_focus_in(&focus, window, |this, _, cx| {
                    let id = cx.entity_id();
                    let owner = this.owner.clone();
                    _ = owner.update(cx, |owner, _| {
                        owner.focused_viewport = Some(this.viewport);
                        if let Some(dock) = &mut owner.dock {
                            dock.activate_viewport(id);
                        }
                    });
                }),
                cx.on_focus_out(&focus, window, |this, _, _, cx| {
                    this.cancel(cx);
                }),
            ];
            Self {
                owner,
                viewport,
                kind,
                content,
                on_input,
                object_menu: None,
                focus,
                toolbar,
                overlay_renderer: Default::default(),
                bounds: Bounds::default(),
                navigation: Default::default(),
                interaction: None,
                _subscriptions: subscriptions,
            }
        })
    }
    pub(crate) fn binding(&self, editor: &Editor) -> Option<editor_core::ViewportBinding> {
        editor
            .viewport_binding(self.viewport)
            .filter(|binding| binding.kind == self.kind)
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
    pub fn hand_tool(&self, cx: &App) -> bool {
        self.owner
            .upgrade()
            .is_some_and(|owner| (self.content)(owner.read(cx), self.viewport).hand_tool)
    }
    pub fn cancel_pointer(&mut self) -> Option<editor_core::ViewportInteractionToken> {
        let token = self
            .interaction
            .take()
            .or_else(|| self.navigation.pan.map(|pan| pan.token));
        self.navigation.wheel_zoom = None;
        self.navigation.pan = None;
        token
    }
}
impl Focusable for ViewportSurface {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for ViewportSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let SurfaceContent {
            image,
            mut overlay,
            error,
            hand_tool,
            controls,
            cursor,
            interaction_label,
            target_label,
            vertex_notice,
        } = (self.content)(state, self.viewport);
        if benchmark::hide_mesh_edges() {
            overlay.lines.clear();
        }
        let active = state.editor.active_id();
        let modal = state.modal || dialog;
        if modal {
            self.cancel_pointer();
        }
        let capture = self.navigation.pan.is_some() || self.interaction.is_some();
        let palette = crate::canvas_overlay::Palette::new(cx.theme().is_dark());
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let background = cx.theme().background;
        let menu_owner = owner.downgrade();
        let menu_canvas = cx.weak_entity();
        let body = div()
            .id("canvas-body")
            .test_support()
            .when_some(cursor.filter(|_| !modal && !hand_tool), |e, cursor| {
                e.cursor(cursor)
            })
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(cx.theme().muted)
            .when(!modal && self.navigation.pan.is_some(), |e| {
                e.cursor_grabbing()
            })
            .when(!modal && self.navigation.pan.is_none() && hand_tool, |e| {
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
            .children(controls.map(|render| render(self, modal, cx)))
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
            .map(|e| {
                if let Some(handler) = self.object_menu {
                    e.context_menu(move |menu, window, cx| {
                        handler(&menu_owner, &menu_canvas, menu, window, cx)
                    })
                    .into_any_element()
                } else {
                    e.into_any_element()
                }
            });
        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context(if modal {
                "KasaneViewport KasaneModal"
            } else {
                "KasaneViewport"
            })
            .child(body)
            .into_any_element()
    }
}

impl ViewportSurface {
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

pub(super) struct SurfaceContent {
    image: Option<(preview::Image, CanvasMetrics)>,
    overlay: editor_core::OverlaySnapshot,
    error: Option<String>,
    hand_tool: bool,
    controls: Option<fn(&ViewportSurface, bool, &mut Context<ViewportSurface>) -> AnyElement>,
    cursor: Option<CursorStyle>,
    interaction_label: Option<String>,
    target_label: Option<String>,
    vertex_notice: Option<String>,
}
