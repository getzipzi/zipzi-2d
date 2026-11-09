//! Opt-in independent viewport presentation. UI chrome stays on GPUI's overlay
//! plane; accepted image/geometry pairs are submitted together to one DXGI buffer.
use super::*;
use ::windows::{
    Win32::Graphics::{Direct2D::Common::D2D_RECT_F, DirectComposition::IDCompositionVisual},
    core::{IUnknown, Interface},
};
use anyhow::{Context as _, Result, ensure};
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use zipzi_editor_render::{WindowsOverlayRenderer, WindowsViewportPresenter};

pub(super) type Shared = Rc<RefCell<NativeCanvas>>;

#[derive(PartialEq, Eq)]
pub(super) struct Chrome {
    pub cursor: crate::tool_cursors::CursorFeedback,
    pub label: Option<String>,
    pub hand: bool,
    pub error: bool,
}

pub(super) struct NativeCanvas {
    pub window: AnyWindowHandle,
    surface: WindowCompositionSurface,
    pub overlay_plane: CompositionSurfaceId,
    presenter: Option<WindowsViewportPresenter>,
    overlay: Option<WindowsOverlayRenderer>,
    cache: zipzi_editor_render::canvas_overlay::CanvasOverlayCache,
    pub chrome: Option<Chrome>,
    bounds: Bounds<Pixels>,
    mask: Bounds<Pixels>,
    scale: f32,
    pending: Option<Pending>,
    in_flight: Option<Pending>,
    retrying: bool,
    failed: bool,
    #[cfg(feature = "bench-instrumentation")]
    fail_after: Option<usize>,
}

struct Pending {
    image: preview::Image,
    overlay: Option<Arc<DirectXSurface>>,
    rect: [f32; 4],
}

impl NativeCanvas {
    pub fn new(window: &Window, view: WeakEntity<ViewportSurface>, cx: &App) -> Result<Shared> {
        let composition = window
            .enable_window_composition()
            .context("enable window composition")?;
        let surface = composition
            .create_native_surface()
            .context("create native viewport surface")?;
        surface
            .platform_surface()?
            .set_visible(false)
            .context("hide new native viewport")?;
        let this = Rc::new(RefCell::new(Self {
            window: window.window_handle(),
            surface,
            overlay_plane: composition.overlay_surface(),
            presenter: None,
            overlay: None,
            cache: Default::default(),
            chrome: None,
            bounds: Bounds::default(),
            mask: Bounds::default(),
            scale: 0.,
            pending: None,
            in_flight: None,
            retrying: false,
            failed: false,
            #[cfg(feature = "bench-instrumentation")]
            fail_after: std::env::var("ZIPZI_NATIVE_VIEWPORT_FAIL_AFTER")
                .ok()
                .and_then(|value| value.parse().ok()),
        }));
        let weak = Rc::downgrade(&this);
        let async_cx = cx.to_async();
        this.borrow()
            .surface
            .platform_surface()?
            .set_compositor_recreated_callback(Rc::new(move |_| {
                if let Some(this) = weak.upgrade() {
                    // Old-device textures are never reattached to a recovered visual.
                    this.borrow_mut().disable();
                    // Recovery may replay a retained scene which has no GPUI
                    // model image. Invalidate it after the renderer releases its
                    // borrow, so fallback does not wait for another user input.
                    let view = view.clone();
                    async_cx
                        .spawn(async move |cx| {
                            let _ = view.update(cx, |_, cx| cx.notify());
                        })
                        .detach();
                }
                Ok(())
            }))?;
        Ok(this)
    }

    pub fn active(&self) -> bool {
        !self.failed
    }
    pub fn hide(&mut self) {
        self.pending = None;
        self.in_flight = None;
        self.presenter = None;
        self.chrome = None;
        let _ = self
            .surface
            .platform_surface()
            .and_then(|surface| surface.set_visible(false));
    }
    pub fn disable(&mut self) {
        self.hide();
        self.overlay = None;
        self.cache = Default::default();
        self.failed = true;
    }
    pub fn remove(&mut self, window: &Window) {
        self.hide();
        if let Ok(composition) = window.enable_window_composition() {
            let _ = composition.remove_surface(self.surface.id());
        }
    }

    fn prepare(
        &mut self,
        image: preview::Image,
        overlay: Cow<'_, zipzi_editor_core::OverlaySnapshot>,
        bounds: Bounds<Pixels>,
        mask: Bounds<Pixels>,
        palette: crate::canvas_overlay::Palette,
        window: &Window,
    ) -> Result<()> {
        let _prepare_span = benchmark::parameter_overlay::ui_span("ui.native_prepare");
        #[cfg(feature = "bench-instrumentation")]
        if let Some(remaining) = &mut self.fail_after {
            ensure!(*remaining > 0, "Injected native viewport failure");
            *remaining -= 1;
        }
        ensure!(
            !self.failed && !bounds.is_empty(),
            "Native viewport unavailable"
        );
        let device = window
            .directx_device()
            .context("D3D11 device unavailable")?;
        let source = preview::native_surface(&image).context("No native image")?;
        ensure!(source.device == device, "Stale viewport device");
        let scale = window.scale_factor();
        let origin = [bounds.origin.x.as_f32(), bounds.origin.y.as_f32()];
        let physical_origin = origin.map(|v| (v * scale).floor());
        let extent = [
            ((bounds.right().as_f32() * scale).ceil() - physical_origin[0]) as u32,
            ((bounds.bottom().as_f32() * scale).ceil() - physical_origin[1]) as u32,
        ];
        let native_bounds = Bounds::new(
            point(
                DevicePixels(physical_origin[0] as i32),
                DevicePixels(physical_origin[1] as i32),
            ),
            size(
                DevicePixels(extent[0] as i32),
                DevicePixels(extent[1] as i32),
            ),
        );
        let clip = bounds.intersect(&mask);
        ensure!(!clip.is_empty(), "Native viewport fully clipped");
        let mut reattached = false;
        if self
            .presenter
            .as_ref()
            .is_none_or(|p| !p.matches(&device, extent))
        {
            self.pending = None;
            self.in_flight = None;
            self.surface.platform_surface()?.set_visible(false)?;
            let presenter = WindowsViewportPresenter::new(device.clone(), extent)?;
            let handle = self.surface.platform_surface()?.platform_handle()?;
            let unknown = handle
                .downcast::<IUnknown>()
                .map_err(|_| anyhow::anyhow!("Not a DComp visual"))?;
            let visual: IDCompositionVisual = unknown.cast()?;
            unsafe {
                visual.SetContent(presenter.swap_chain())?;
            }
            self.presenter = Some(presenter);
            reattached = true;
            // set_bounds commits the newly attached swap chain, even at the same size.
            self.surface.platform_surface()?.set_bounds(native_bounds)?;
        } else if self.surface.platform_surface()?.bounds() != native_bounds {
            self.surface.platform_surface()?.set_bounds(native_bounds)?;
        }
        if reattached || self.bounds != bounds || self.mask != mask || self.scale != scale {
            let handle = self.surface.platform_surface()?.platform_handle()?;
            let unknown = handle
                .downcast::<IUnknown>()
                .map_err(|_| anyhow::anyhow!("Not a DComp visual"))?;
            let visual: IDCompositionVisual = unknown.cast()?;
            unsafe {
                visual.SetClip2(&D2D_RECT_F {
                    left: clip.left().as_f32() * scale - physical_origin[0],
                    top: clip.top().as_f32() * scale - physical_origin[1],
                    right: clip.right().as_f32() * scale - physical_origin[0],
                    bottom: clip.bottom().as_f32() * scale - physical_origin[1],
                })?;
            }
            // Commit the local clip even when visibility and bounds are unchanged.
            self.surface.platform_surface()?.set_bounds(native_bounds)?;
        }
        if self
            .overlay
            .as_ref()
            .is_none_or(|r| !r.uses_device(&device))
        {
            self.overlay = Some(WindowsOverlayRenderer::new(device)?);
        }
        let pack_span = benchmark::parameter_overlay::ui_span("ui.native_overlay_pack");
        let list = self.cache.prepare_snapshot(
            overlay,
            [bounds.size.width.as_f32(), bounds.size.height.as_f32()],
            palette.canvas(),
            zipzi_editor_render::canvas_overlay::CanvasOptions {
                lines: benchmark::parameter_overlay::paint_edges(),
                markers: benchmark::parameter_overlay::paint_vertices(),
            },
        )?;
        drop(pack_span);
        let draw_span = benchmark::parameter_overlay::ui_span("ui.native_overlay_draw");
        let overlay = self.overlay.as_mut().unwrap().draw_prepared(
            zipzi_editor_render::OverlayViewport {
                extent,
                scale,
                origin,
            },
            list,
        )?;
        drop(draw_span);
        self.pending = Some(Pending {
            image,
            overlay,
            rect: [
                origin[0] * scale - physical_origin[0],
                origin[1] * scale - physical_origin[1],
                bounds.size.width.as_f32() * scale,
                bounds.size.height.as_f32() * scale,
            ],
        });
        self.bounds = bounds;
        self.mask = mask;
        self.scale = scale;
        Ok(())
    }

    fn flush(&mut self) -> Result<bool> {
        if let Some(frame) = &self.in_flight {
            if !self
                .presenter
                .as_mut()
                .context("Native presenter missing")?
                .retry()?
            {
                return Ok(false);
            }
            frame_trace::emit(
                preview::image_trace_id(&frame.image),
                "native_viewport_present_return",
                &[],
            );
            self.surface.platform_surface()?.set_visible(true)?;
            self.in_flight = None;
        }
        let Some(frame) = self.pending.take() else {
            return Ok(true);
        };
        let trace = preview::image_trace_id(&frame.image);
        frame_trace::emit(trace, "native_viewport_submit_begin", &[]);
        let submitted = self
            .presenter
            .as_mut()
            .context("Native presenter missing")?
            .present(
                preview::native_surface(&frame.image).context("Native image missing")?,
                frame.overlay.as_ref(),
                frame.rect,
            )?;
        frame_trace::emit(
            trace,
            "native_viewport_submit_end",
            &[("submitted", u64::from(submitted))],
        );
        if submitted {
            frame_trace::emit(trace, "native_viewport_present_return", &[]);
            self.surface.platform_surface()?.set_visible(true)?;
        } else {
            self.in_flight = Some(frame);
        }
        Ok(submitted)
    }
}

impl EditorView {
    pub(crate) fn refresh_native_viewport(
        &self,
        viewport: zipzi_editor_core::ViewportId,
        cx: &mut App,
    ) -> bool {
        let Some(view) = self.viewport_surface(viewport, cx) else {
            return false;
        };
        let surface = view.read(cx);
        let Some(native) = surface.native.clone().filter(|n| n.borrow().active()) else {
            return false;
        };
        if self.modal
            || self.editor.is_busy()
            || self.mesh_generation.is_some()
            || self.editor.manual_for_viewport(viewport)
            || surface.interaction.is_some()
            || surface.navigation.pan.is_some()
            || self.editor.active_tool() == ToolId::BrushSelect
        {
            return false;
        }
        let bounds = surface.bounds;
        let show_origin = surface.show_origin_crosshair;
        let content = (surface.content)(self, viewport);
        let Some((image, metrics)) = content.image else {
            return false;
        };
        if content.error.is_some()
            || metrics.width != bounds.size.width.as_f32()
            || metrics.height != bounds.size.height.as_f32()
        {
            return false;
        }
        let handle = native.borrow().window;
        let weak_view = view.downgrade();
        handle
            .update(cx, |_, window, cx| {
                if !self.viewport_displayed(viewport, window, cx)
                    || window.has_active_dialog(cx)
                    || native.borrow().bounds != bounds
                    || native.borrow().scale != window.scale_factor()
                {
                    return false;
                }
                let mut overlay = content.overlay;
                if !show_origin {
                    overlay.origin_crosshair = None;
                }
                let cursor = crate::tool_cursors::resolve(crate::tool_cursors::CursorInput {
                    overlay: &overlay,
                    tool: self.editor.active_tool(),
                    manual_tool: None,
                    modifiers: window.modifiers(),
                    window_active: window.is_window_active(),
                    modal: false,
                    panning: false,
                    hand_tool: content.hand_tool,
                });
                if cursor.hide_selection_brush {
                    overlay.selection_brush = None;
                }
                let chrome = Chrome {
                    cursor,
                    label: overlay.hovered_object_label.clone(),
                    hand: content.hand_tool,
                    error: false,
                };
                if native.borrow().chrome.as_ref() != Some(&chrome) {
                    return false;
                }
                let mask = native.borrow().mask;
                submit(
                    &native,
                    image,
                    Cow::Owned(overlay),
                    bounds,
                    mask,
                    crate::canvas_overlay::Palette::new(cx.theme().is_dark()),
                    weak_view,
                    window,
                    cx,
                )
            })
            .unwrap_or(false)
    }

    pub(crate) fn hide_native_viewport(&self, viewport: zipzi_editor_core::ViewportId, cx: &App) {
        if let Some(view) = self.viewport_surface(viewport, cx) {
            if let Some(native) = &view.read(cx).native {
                native.borrow_mut().hide();
            }
        }
    }
}

pub(super) fn submit(
    native: &Shared,
    image: preview::Image,
    overlay: Cow<'_, zipzi_editor_core::OverlaySnapshot>,
    bounds: Bounds<Pixels>,
    mask: Bounds<Pixels>,
    palette: crate::canvas_overlay::Palette,
    view: WeakEntity<ViewportSurface>,
    window: &Window,
    cx: &mut App,
) -> bool {
    let result = {
        let mut state = native.borrow_mut();
        state
            .prepare(image, overlay, bounds, mask, palette, window)
            .and_then(|_| state.flush())
    };
    match result {
        Ok(true) => true,
        Ok(false) => {
            if !native.borrow().retrying {
                native.borrow_mut().retrying = true;
                let weak = Rc::downgrade(native);
                cx.spawn(async move |cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(2))
                            .await;
                        let keep_running = cx.update(|cx| {
                            let Some(native) = weak.upgrade() else {
                                return false;
                            };
                            let result = native.borrow_mut().flush();
                            match result {
                                Ok(false) => true,
                                Ok(true) => {
                                    native.borrow_mut().retrying = false;
                                    false
                                }
                                Err(error) => {
                                    eprintln!("Native viewport retry failed; using GPUI: {error}");
                                    native.borrow_mut().disable();
                                    let _ = view.update(cx, |_, cx| cx.notify());
                                    false
                                }
                            }
                        });
                        if !keep_running {
                            break;
                        }
                    }
                })
                .detach();
            }
            true
        }
        Err(error) => {
            eprintln!("Native viewport failed; using GPUI: {error}");
            native.borrow_mut().disable();
            false
        }
    }
}

/// Switch only painting, preserving layout, focus, hitboxes and popup ordering.
pub(super) struct Plane {
    pub child: AnyElement,
    pub plane: CompositionSurfaceId,
}
impl IntoElement for Plane {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Plane {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window
            .with_composition_surface(self.plane, |window| self.child.paint(window, cx))
            .expect("viewport overlay plane exists for the window lifetime");
    }
}
