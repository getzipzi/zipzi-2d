//! Concrete viewport panels sharing measured surfaces and navigation.
macro_rules! viewport_panel {
    ($name:ident, $label:literal, $header_width:literal) => {
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
            ) -> Option<zoom_bar::HeaderState> {
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
                px($header_width)
            }
        }
    };
}

mod brush_bar;
mod brush_tick;
mod context_menu;
mod deformer_bar;
mod input;
mod isolation;
#[cfg(target_os = "windows")]
mod native_windows;
mod painting;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use painting::CanvasOverlay;
mod animation;
mod background;
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
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    menu::{ContextMenuExt, PopupMenuItem},
};
use zipzi_editor_core::{
    CanvasInput, CommandId, EditorCommand, ToolId, ViewportMetrics, ViewportPoint,
};

pub struct ViewportSurface {
    #[cfg(target_os = "windows")]
    native: Option<native_windows::Shared>,
    pub(crate) input_region: crate::input_routing::InputRegion,
    pub(crate) owner: WeakEntity<EditorView>,
    pub(crate) focus: FocusHandle,
    pub(crate) viewport: zipzi_editor_core::ViewportId,
    pub(crate) kind: zipzi_editor_core::ViewportKind,
    content: fn(&EditorView, zipzi_editor_core::ViewportId) -> SurfaceContent,
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
    pub(super) show_transparency_grid: bool,
    pub(super) show_origin_crosshair: bool,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    overlay_renderer: std::rc::Rc<std::cell::RefCell<CanvasOverlay>>,
    overlay_cache:
        std::rc::Rc<std::cell::RefCell<zipzi_editor_render::canvas_overlay::CanvasOverlayCache>>,
    bounds: Bounds<Pixels>,
    #[cfg(test)]
    pub(crate) layout_syncs: usize,
    #[cfg(test)]
    pub(crate) tool_cursor: Option<CursorStyle>,
    #[cfg(test)]
    pub(crate) tool_cursor_kind: Option<crate::tool_cursors::ToolCursor>,
    navigation: navigation::ViewportNavigation,
    interaction: Option<zipzi_editor_core::ViewportInteractionToken>,
    brush_tick: Option<(
        zipzi_editor_core::ViewportInteractionToken,
        std::time::Instant,
    )>,
    brush_frame_pending: bool,
    brush_bar: Entity<brush_bar::BrushSelectionBar>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy)]
struct PanGesture {
    position: Point<Pixels>,
    button: MouseButton,
    token: zipzi_editor_core::ViewportInteractionToken,
}

impl ViewportSurface {
    fn new(
        owner: WeakEntity<EditorView>,
        viewport: zipzi_editor_core::ViewportId,
        kind: zipzi_editor_core::ViewportKind,
        content: fn(&EditorView, zipzi_editor_core::ViewportId) -> SurfaceContent,
        on_input: fn(&mut Self, CanvasInput, &mut Window, &mut Context<Self>),
        toolbar: toolbar::ToolbarRegistry,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let brush_bar = brush_bar::BrushSelectionBar::new(owner.clone(), viewport, window, cx);
            let upgraded = owner.upgrade().unwrap();
            let focus = cx.focus_handle();
            #[allow(unused_mut)]
            let mut subscriptions = vec![
                cx.observe(&upgraded, |this, owner, cx| {
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
                                owner.editor.cancel_viewport_interaction(token);
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
            #[cfg(target_os = "windows")]
            let native = if !cx.is_test()
                && std::env::var("ZIPZI_NATIVE_VIEWPORT").as_deref() == Ok("1")
                && benchmark::native_canvas_overlay()
            {
                match native_windows::NativeCanvas::new(window, cx.weak_entity(), cx) {
                    Ok(native) => {
                        subscriptions.push(cx.on_release_in(window, |this, window, _| {
                            if let Some(native) = &this.native {
                                native.borrow_mut().remove(window);
                            }
                        }));
                        Some(native)
                    }
                    Err(error) => {
                        eprintln!("Native viewport unavailable; using GPUI: {error}");
                        None
                    }
                }
            } else {
                None
            };
            let surface = cx.weak_entity();
            let input_region =
                crate::input_routing::InputRegion::new(focus.clone(), move |editor, _| {
                    ViewportSurface::input_target(editor, viewport, kind)
                })
                .with_capture(move |editor, cx| {
                    let entity = surface.upgrade()?;
                    let token = entity.read(cx).pointer_token()?;
                    let surface = surface.clone();
                    Some(crate::input_routing::CapturedInput::new(
                        crate::input_routing::LocalTarget::Viewport(token.binding),
                        editor.viewport_interaction_valid(token),
                        crate::input_routing::CaptureCancelKey::CommandBinding,
                        move |_, cx| {
                            surface
                                .update(cx, |surface, cx| {
                                    if surface.pointer_token() != Some(token) {
                                        return None;
                                    }
                                    surface.cancel_pointer();
                                    cx.notify();
                                    Some(Box::new(move |owner: &mut EditorView| {
                                        owner.editor.cancel_viewport_interaction(token);
                                    })
                                        as crate::input_routing::CoreCleanup)
                                })
                                .ok()
                                .flatten()
                        },
                    ))
                });
            Self {
                #[cfg(target_os = "windows")]
                native,
                input_region,
                owner,
                viewport,
                kind,
                content,
                on_input,
                object_menu: None,
                focus,
                toolbar,
                show_transparency_grid: false,
                show_origin_crosshair: true,
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                overlay_renderer: Default::default(),
                overlay_cache: Default::default(),
                bounds: Bounds::default(),
                #[cfg(test)]
                layout_syncs: 0,
                #[cfg(test)]
                tool_cursor: None,
                #[cfg(test)]
                tool_cursor_kind: None,
                navigation: Default::default(),
                interaction: None,
                brush_tick: None,
                brush_frame_pending: false,
                brush_bar,
                _subscriptions: subscriptions,
            }
        })
    }
    pub(crate) fn binding(&self, editor: &Editor) -> Option<zipzi_editor_core::ViewportBinding> {
        editor
            .viewport_binding(self.viewport)
            .filter(|binding| binding.kind == self.kind)
    }
    pub(super) fn origin_crosshair_available(&self, owner: &EditorView) -> bool {
        self.kind == zipzi_editor_core::ViewportKind::Modeling
            && owner.editor.has_presented_frame(
                owner
                    .modeling_frame_for(self.viewport)
                    .map(|frame| frame.canvas.version),
            )
    }
    pub(crate) fn command(
        &self,
        editor: &Editor,
        id: CommandId,
        source: zipzi_editor_core::CommandSource,
    ) -> zipzi_editor_core::Command {
        self.binding(editor)
            .map(|binding| editor.command_for_viewport(id, binding, source))
            .unwrap_or(zipzi_editor_core::Command {
                id,
                target: zipzi_editor_core::CommandTarget::Unavailable,
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
        self.owner.upgrade().is_some_and(|owner| {
            self.kind == zipzi_editor_core::ViewportKind::Animation
                || benchmark::parameter_overlay::animation_viewport_style()
                || owner.read(cx).editor.active_tool() == ToolId::Hand
        })
    }
    pub(super) fn sync_status_bar(&self, cx: &mut App) {
        if let Some(owner) = self.owner.upgrade() {
            crate::workspace::status_bar::StatusBar::sync_owner(&owner, cx);
        }
    }
    fn pointer_token(&self) -> Option<zipzi_editor_core::ViewportInteractionToken> {
        self.interaction
            .or_else(|| self.navigation.pan.map(|pan| pan.token))
    }
    pub fn cancel_pointer(&mut self) -> Option<zipzi_editor_core::ViewportInteractionToken> {
        let token = self.pointer_token();
        self.interaction = None;
        self.brush_tick = None;
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
        let _span = benchmark::parameter_overlay::ui_span("ui.viewport_render");
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
        } = (self.content)(state, self.viewport);
        if !self.show_origin_crosshair {
            overlay.origin_crosshair = None;
        }
        if benchmark::hide_mesh_edges() {
            overlay.lines.clear();
        }
        let generation_panel = state
            .mesh_generation
            .as_ref()
            .filter(|panel| panel.read(cx).viewport == self.viewport)
            .cloned();
        let active = state.editor.active_id();
        let modal = state.modal || dialog;
        let cursor_feedback = crate::tool_cursors::resolve(crate::tool_cursors::CursorInput {
            overlay: &overlay,
            tool: state.editor.active_tool(),
            manual_tool: state
                .editor
                .manual_for_viewport(self.viewport)
                .then(|| state.editor.manual_tool())
                .flatten(),
            modifiers: window.modifiers(),
            window_active: window.is_window_active(),
            modal: modal || generation_panel.is_some() || state.editor.is_busy(),
            panning: self.navigation.pan.is_some(),
            hand_tool,
        });
        if cursor_feedback.hide_selection_brush {
            overlay.selection_brush = None;
        }
        let cursor = cursor_feedback
            .cursor
            .map(|cursor| state.tool_cursors.style(cursor));
        #[cfg(test)]
        {
            self.tool_cursor = cursor;
            self.tool_cursor_kind = cursor_feedback.cursor;
        }
        if modal {
            self.cancel_pointer();
        }
        let capture = self.navigation.pan.is_some() || self.interaction.is_some();
        let palette = crate::canvas_overlay::Palette::new(cx.theme().is_dark());
        let muted = cx.theme().muted_foreground;
        let menu_owner = owner.downgrade();
        let menu_canvas = cx.weak_entity();
        let hover_name = overlay.hovered_object_label.clone();
        #[cfg(target_os = "windows")]
        if let Some(native) = &self.native {
            native.borrow_mut().chrome = Some(native_windows::Chrome {
                cursor: cursor_feedback,
                label: hover_name.clone(),
                hand: hand_tool,
                error: error.is_some(),
            });
        }
        #[cfg(target_os = "windows")]
        let native_active = self.native.as_ref().is_some_and(|n| n.borrow().active());
        #[cfg(not(target_os = "windows"))]
        let native_active = false;
        let native_image = native_active.then(|| image.clone()).flatten();
        let body = div()
            .id("canvas-body")
            .test_support()
            .when_some(cursor, |e, cursor| e.cursor(cursor))
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(crate::theme::viewport_background(cx))
            .map(|element| self.bind_input(element, capture, cx))
            .when(capture, |body| {
                body.when_some(cursor, |body, cursor| {
                    body.child(
                        canvas(
                            |_, _, _| (),
                            move |_, _, window, _| {
                                // Captured gestures own the cursor beyond the canvas hitbox.
                                window.set_window_cursor_style(cursor);
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                })
            })
            .when(self.show_transparency_grid, |body| {
                body.child(background::transparency_grid(cx))
            })
            .children(image.filter(|_| !native_active).map(|(image, metrics)| {
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(metrics.width))
                    .h(px(metrics.height))
                    .child(preview::element(image))
            }));
        let chrome = div()
            .absolute()
            .size_full()
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
            .child(self.overlay_element(overlay, palette, capture, native_image, cx))
            .children(hover_name.map(|name| {
                div()
                    .id("hovered-object-name")
                    .test_support()
                    .aria_label(name.clone())
                    .absolute()
                    .bottom_2()
                    .left_2()
                    .max_w_full()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(cx.theme().background)
                    .text_color(cx.theme().foreground)
                    .text_xs()
                    .truncate()
                    .child(name)
            }))
            .when(
                generation_panel.is_none() && !self.toolbar.items().is_empty(),
                |element| {
                    element.child(toolbar::CanvasToolbar {
                        owner: owner.clone(),
                        canvas: cx.entity(),
                        modal,
                        items: self.toolbar.items(),
                    })
                },
            )
            .children(controls.map(|render| render(self, modal, cx)))
            .children(generation_panel.map(|panel| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .px(crate::ui::compact::rem(32.))
                    .flex()
                    .justify_center()
                    .child(panel.clone())
            }))
            // Include our own occluding toolbar/controls in the viewport region.
            // Later popup layers can still occlude the entire region.
            .child(self.input_region.marker());
        #[cfg(target_os = "windows")]
        let chrome = if let Some(native) = &self.native {
            native_windows::Plane {
                child: chrome.into_any_element(),
                plane: native.borrow().overlay_plane,
            }
            .into_any_element()
        } else {
            chrome.into_any_element()
        };
        let body = body.child(chrome).map(|e| {
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
            // The focused container owns keyboard dispatch; the canvas body is
            // its child and is outside the focus node's modifier-event path.
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .key_context(if modal {
                "ZipziViewport ZipziModal"
            } else {
                "ZipziViewport"
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
    ) -> Option<zoom_bar::HeaderState> {
        let dialog = window.has_active_dialog(cx);
        panel.read(cx).owner.upgrade().map(|owner| {
            let owner = owner.read(cx);
            zoom_bar::state(owner, panel.read(cx), owner.modal || dialog)
        })
    }
}

pub(super) struct SurfaceContent {
    image: Option<(preview::Image, ViewportMetrics)>,
    overlay: zipzi_editor_core::OverlaySnapshot,
    error: Option<String>,
    hand_tool: bool,
    controls: Option<fn(&ViewportSurface, bool, &mut Context<ViewportSurface>) -> AnyElement>,
}

impl SurfaceContent {
    fn navigation(image: Option<(preview::Image, ViewportMetrics)>, error: Option<String>) -> Self {
        Self {
            image,
            error,
            overlay: Default::default(),
            hand_tool: true,
            controls: None,
        }
    }
}
