//! Native canvas overlay rendering, unified GPUI fallback and measured capture.
use super::*;

#[cfg(target_os = "macos")]
#[derive(Default)]
pub(super) struct CanvasOverlay {
    device: Option<metal::Device>,
    renderer: Option<zipzi_editor_render::MetalOverlayRenderer>,
    error: Option<String>,
}
#[cfg(target_os = "macos")]
impl CanvasOverlay {
    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        list: std::sync::Arc<zipzi_editor_render::canvas_overlay::CanvasDrawList>,
        window: &mut Window,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            benchmark::native_canvas_overlay(),
            "GPUI diagnostic override"
        );
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
        let viewport = zipzi_editor_render::OverlayViewport {
            extent,
            scale,
            origin,
        };
        let renderer = self.renderer(&gpu.device)?;
        let image = renderer.draw_prepared(&gpu.queue, viewport, list)?;
        trace_canvas(renderer.canvas_stats());
        if let Some(image) = image {
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
    ) -> anyhow::Result<&mut zipzi_editor_render::MetalOverlayRenderer> {
        if self
            .device
            .as_ref()
            .is_none_or(|old| !std::ptr::eq(old.as_ref(), device.as_ref()))
        {
            self.device = Some(device.clone());
            self.renderer = None;
            self.error = None;
            match zipzi_editor_render::MetalOverlayRenderer::new(device.clone()) {
                Ok(renderer) => {
                    #[cfg(feature = "bench-instrumentation")]
                    let mut renderer = renderer;
                    #[cfg(feature = "bench-instrumentation")]
                    {
                        renderer.command_observer = benchmark::parameter_overlay::observer();
                    }
                    self.renderer = Some(renderer);
                }
                Err(error) => {
                    eprintln!("Canvas overlay initialization failed; using GPUI: {error}");
                    self.error = Some(error.to_string());
                }
            }
        }
        self.renderer.as_mut().ok_or_else(|| {
            anyhow::anyhow!("{}", self.error.as_deref().unwrap_or("Overlay unavailable"))
        })
    }
}

#[cfg(target_os = "windows")]
#[derive(Default)]
pub(super) struct CanvasOverlay {
    renderer: Option<zipzi_editor_render::WindowsOverlayRenderer>,
    failed_device: Option<::windows::Win32::Graphics::Direct3D11::ID3D11Device>,
}
#[cfg(target_os = "windows")]
impl CanvasOverlay {
    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        list: std::sync::Arc<zipzi_editor_render::canvas_overlay::CanvasDrawList>,
        window: &mut Window,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            benchmark::native_canvas_overlay(),
            "GPUI diagnostic override"
        );
        let device = window
            .directx_device()
            .ok_or_else(|| anyhow::anyhow!("D3D11 context unavailable"))?;
        if self
            .renderer
            .as_ref()
            .is_none_or(|r| !r.uses_device(&device))
        {
            self.renderer = None;
            anyhow::ensure!(
                self.failed_device.as_ref() != Some(&device),
                "Overlay initialization failed"
            );
            match zipzi_editor_render::WindowsOverlayRenderer::new(device.clone()) {
                Ok(renderer) => {
                    self.renderer = Some(renderer);
                    self.failed_device = None;
                }
                Err(e) => {
                    self.failed_device = Some(device);
                    eprintln!("Canvas overlay initialization failed; using GPUI: {e}");
                    return Err(e);
                }
            }
        }
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return Ok(());
        }
        let scale = window.scale_factor();
        let origin = [bounds.origin.x.as_f32(), bounds.origin.y.as_f32()];
        let texture_origin = origin.map(|v| (v * scale).floor());
        let extent = [
            ((bounds.right().as_f32() * scale).ceil() - texture_origin[0]) as u32,
            ((bounds.bottom().as_f32() * scale).ceil() - texture_origin[1]) as u32,
        ];
        let viewport = zipzi_editor_render::OverlayViewport {
            extent,
            scale,
            origin,
        };
        let renderer = self.renderer.as_mut().unwrap();
        let image = renderer.draw_prepared(viewport, list)?;
        trace_canvas(renderer.canvas_stats());
        if let Some(image) = image {
            gpui_kit::frame_trace::emit(
                gpui_kit::frame_trace::next_id(),
                "overlay_shader_surface",
                &[("pass", 0)],
            );
            window.paint_surface(
                Bounds::new(
                    point(px(texture_origin[0] / scale), px(texture_origin[1] / scale)),
                    size(px(extent[0] as f32 / scale), px(extent[1] as f32 / scale)),
                ),
                SurfaceSource::DirectX(image),
            );
        }
        Ok(())
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn trace_canvas(stats: zipzi_editor_render::canvas_overlay::CanvasRenderStats) {
    gpui_kit::frame_trace::emit(
        gpui_kit::frame_trace::next_id(),
        "canvas_overlay_draw",
        &[
            ("instances", stats.instances as u64),
            ("upload_bytes", stats.upload_bytes as u64),
            ("draw_calls", stats.draw_calls as u64),
            ("cached", u64::from(stats.cached)),
        ],
    );
}

impl ViewportSurface {
    pub(super) fn overlay_element(
        &self,
        overlay: zipzi_editor_core::OverlaySnapshot,
        palette: crate::canvas_overlay::Palette,
        capture: bool,
        native_image: Option<(preview::Image, ViewportMetrics)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let measure = view.clone();
        let mouse = view.clone();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let overlay_renderer = self.overlay_renderer.clone();
        let overlay_cache = self.overlay_cache.clone();
        #[cfg(target_os = "windows")]
        let native = self.native.clone();
        #[cfg(target_os = "windows")]
        let native_view = cx.weak_entity();
        canvas(
            move |bounds, window, cx| {
                let metrics = ViewportMetrics {
                    width: bounds.size.width.as_f32(),
                    height: bounds.size.height.as_f32(),
                    scale_factor: if window.is_visible() {
                        window.scale_factor()
                    } else {
                        0.
                    },
                };
                let surface = measure.read(cx);
                let metrics_changed = surface.owner.upgrade().is_some_and(|owner| {
                    let owner = owner.read(cx);
                    surface.binding(&owner.editor).is_some_and(|binding| {
                        owner.editor.viewport_metrics(binding.viewport) != Some(metrics)
                    })
                });
                // An update during prepaint counts as a write to GPUI's retained
                // dependencies even when it changes no fields. Avoid invalidating
                // the owner and its readers on every unchanged canvas frame.
                if surface.bounds == bounds
                    && !metrics_changed
                    && !benchmark::parameter_overlay::eager_viewport_layout()
                {
                    return;
                }
                let _span = benchmark::parameter_overlay::ui_span("ui.viewport_layout_sync");
                measure.update(cx, |this, cx| {
                    #[cfg(test)]
                    {
                        this.layout_syncs += 1;
                    }
                    this.bounds = bounds;
                    let _ = this.owner.update(cx, |owner, cx| {
                        let Some(binding) = this.binding(&owner.editor) else {
                            return;
                        };
                        if owner.editor.viewport_metrics(binding.viewport) != Some(metrics) {
                            let editing = owner.editor.gesture_active();
                            owner.editor.set_viewport_metrics(binding.viewport, metrics);
                            if editing && !owner.editor.gesture_active() {
                                // Resizing cancels an authoring gesture; its
                                // rollback must also reach document panels.
                                owner.redraw(cx);
                            } else {
                                owner.redraw_canvas(cx);
                            }
                        }
                    });
                });
            },
            move |bounds, _, window, _cx| {
                #[allow(unused_mut)]
                let mut native_painted = false;
                #[cfg(target_os = "windows")]
                if let (Some(native), Some((image, metrics))) = (&native, &native_image) {
                    // During resize retain the old image at its original size on
                    // GPUI until a canvas matching the measured viewport arrives.
                    if metrics.width == bounds.size.width.as_f32()
                        && metrics.height == bounds.size.height.as_f32()
                        && !window.content_mask().bounds.intersect(&bounds).is_empty()
                    {
                        native_painted = native_windows::submit(
                            native,
                            image.clone(),
                            std::borrow::Cow::Borrowed(&overlay),
                            bounds,
                            window.content_mask().bounds,
                            palette,
                            native_view.clone(),
                            window,
                            _cx,
                        );
                    }
                    if !native_painted {
                        frame_trace::emit(
                            preview::image_trace_id(image),
                            "native_viewport_gpui_fallback",
                            &[
                                ("image_width", metrics.width.to_bits() as u64),
                                ("image_height", metrics.height.to_bits() as u64),
                                ("width", bounds.size.width.as_f32().to_bits() as u64),
                                ("height", bounds.size.height.as_f32().to_bits() as u64),
                                ("fully_clipped", u64::from(window.content_mask().bounds.intersect(&bounds).is_empty())),
                            ],
                        );
                        native.borrow_mut().hide();
                        let content =
                            Bounds::new(bounds.origin, size(px(metrics.width), px(metrics.height)));
                        let _ = preview::paint_in_bounds(image, bounds, content, window);
                    }
                }
                #[cfg(target_os = "windows")]
                if native_image.is_none() {
                    if let Some(native) = &native {
                        native.borrow_mut().hide();
                    }
                }
                #[cfg(not(target_os = "windows"))]
                let _ = native_image;
                benchmark::parameter_overlay::capture(4, || {
                    if !native_painted && benchmark::parameter_overlay::paint_any() {
                        if bounds.is_empty() {
                            return;
                        }
                        let options = zipzi_editor_render::canvas_overlay::CanvasOptions {
                            lines: benchmark::parameter_overlay::paint_edges(),
                            markers: benchmark::parameter_overlay::paint_vertices(),
                        };
                        #[cfg(feature = "bench-instrumentation")]
                        let _prepare_span = zipzi_core::benchmark::span("overlay.canvas_prepare");
                        let prepared = overlay_cache.borrow_mut().prepare(
                            &overlay,
                            [bounds.size.width.as_f32(), bounds.size.height.as_f32()],
                            palette.canvas(),
                            options,
                        );
                        #[cfg(feature = "bench-instrumentation")]
                        drop(_prepare_span);
                        match prepared {
                            Ok(list) => {
                                #[cfg(any(target_os = "macos", target_os = "windows"))]
                                let native = overlay_renderer
                                    .borrow_mut()
                                    .paint(bounds, list.clone(), window)
                                    .is_ok();
                                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                                let native = false;
                                if !native {
                                    #[cfg(feature = "bench-instrumentation")]
                                    let _span =
                                        zipzi_core::benchmark::span("overlay.gpui_canvas_fallback");
                                    crate::canvas_overlay::paint_prepared_with_options(
                                        &list,
                                        bounds.origin,
                                        crate::canvas_overlay::FallbackOptions {
                                            layer_quads:
                                                benchmark::parameter_overlay::layer_markers(),
                                            batch_lines: benchmark::batch_mesh_edges(),
                                            tessellated_edges:
                                                benchmark::parameter_overlay::tessellated_edges(),
                                        },
                                        window,
                                    );
                                }
                            }
                            Err(error) => eprintln!("Canvas overlay preparation failed: {error}"),
                        }
                    }
                });
                if capture {
                    let mouse = mouse.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            mouse.update(cx, |this, cx| this.moved(event, window, cx));
                            cx.stop_propagation();
                        }
                    });
                    let view = view.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            view.update(cx, |this, cx| {
                                this.pointer_up(event, window, cx);
                            });
                            cx.stop_propagation();
                        }
                    });
                }
            },
        )
        .absolute()
        .size_full()
    }
}
