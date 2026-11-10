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
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let measure = view.clone();
        let mouse = view.clone();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let overlay_renderer = self.overlay_renderer.clone();
        let overlay_cache = self.overlay_cache.clone();
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
                benchmark::parameter_overlay::capture(4, || {
                    if benchmark::parameter_overlay::paint_any() {
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
pub(super) fn glue_markers(
    overlay: &zipzi_editor_core::OverlaySnapshot,
    cx: &App,
) -> Vec<AnyElement> {
    let theme = cx.theme();
    let mut markers = Vec::new();
    let badge = |side, weight: f32, color| {
        div()
            .px(px(3.))
            .text_size(px(10.))
            .text_color(theme.foreground)
            .bg(gpui_kit::rgb(color)
                .opacity(overlay.glue_weight_opacity * (0.2 + 0.8 * weight.clamp(0., 1.))))
            .child(side)
    };
    if overlay.glue_weight_opacity > 0. {
        for pair in &overlay.glue_weights {
            let marker = |at: zipzi_editor_core::ViewportPoint| {
                div()
                    .absolute()
                    .left(px(at.x - 6.))
                    .top(px(at.y - 7.))
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .overflow_hidden()
            };
            let a = || badge("A", pair.weights[1], 0xf54e59);
            let b = || badge("B", pair.weights[0], 0x39bf69);
            if (pair.a.x - pair.b.x).hypot(pair.a.y - pair.b.y) < 5. {
                markers.push(marker(pair.a).child(a()).child(b()).into_any_element());
            } else {
                markers.push(marker(pair.a).child(a()).into_any_element());
                markers.push(marker(pair.b).child(b()).into_any_element());
            }
        }
    }
    for label in &overlay.glue_labels {
        markers.push(
            div()
                .id(SharedString::from(format!(
                    "glue-label-{}",
                    label.object.id
                )))
                .test_support()
                .aria_label(label.name.clone())
                .absolute()
                .left(px(label.position.x))
                .top(px(label.position.y))
                .w(px(zipzi_editor_core::OverlayGlueLabel::WIDTH))
                .h(px(zipzi_editor_core::OverlayGlueLabel::HEIGHT))
                .px(px(5.))
                .rounded_sm()
                .border_1()
                .border_color(if label.locked {
                    theme.border
                } else {
                    theme.primary
                })
                .bg(if label.selected && !label.locked {
                    theme.primary.opacity(0.22)
                } else {
                    theme.background
                })
                .text_color(if label.locked {
                    theme.muted_foreground
                } else {
                    theme.foreground
                })
                .text_size(px(11.))
                .cursor_pointer()
                .truncate()
                .child(label.name.clone())
                .into_any_element(),
        );
    }
    markers
}
