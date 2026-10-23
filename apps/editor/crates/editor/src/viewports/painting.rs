//! Metal overlay rendering, GPUI fallback and measured canvas capture.
use super::*;

#[derive(Default)]
pub(super) struct CanvasOverlay {
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

impl ViewportSurface {
    pub(super) fn overlay_element(
        &self,
        overlay: editor_core::OverlaySnapshot,
        palette: crate::canvas_overlay::Palette,
        capture: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let measure = view.clone();
        let mouse = view.clone();
        let overlay_renderer = self.overlay_renderer.clone();
        let primary = palette.selected;
        let selection_fill = primary.opacity(0.12);
        canvas(
            move |bounds, window, cx| {
                measure.update(cx, |this, cx| {
                    let metrics = ViewportMetrics {
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
                let result = overlay_renderer.borrow_mut().paint(
                    bounds,
                    &overlay,
                    palette.metal(),
                    editor_render::OverlayPass::Edges,
                    window,
                );
                // Preserve editing visibility if shader/device setup is unavailable.
                if result.is_err() {
                    let to_window = |p: ViewportPoint| bounds.origin + point(px(p.x), px(p.y));
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
                    window.paint_quad(fill(rect, selection_fill));
                    let p = |x, y| ViewportPoint { x, y };
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
                if overlay.lasso.len() >= 2 {
                    if let Some(path) = crate::canvas_overlay::polygon_fill_path(
                        overlay
                            .lasso
                            .iter()
                            .map(|p| bounds.origin + point(px(p.x), px(p.y))),
                    ) {
                        window.paint_path(path, selection_fill);
                    }
                    let edges: Vec<_> = overlay.lasso.windows(2).map(|p| (p[0], p[1])).collect();
                    mesh_edges::paint_edges(&edges, bounds.origin, |path| {
                        window.paint_path(path, primary);
                    });
                    let first = overlay.lasso[0];
                    let last = *overlay.lasso.last().unwrap();
                    let length = (last.x - first.x).hypot(last.y - first.y);
                    // Bound the dash count when the captured pointer is far outside the viewport.
                    let steps = (length / 8.).ceil().clamp(1., 4096.) as usize;
                    let at = |t: f32| ViewportPoint {
                        x: last.x + (first.x - last.x) * t,
                        y: last.y + (first.y - last.y) * t,
                    };
                    let closure: Vec<_> = (0..steps)
                        .map(|i| {
                            (
                                at(i as f32 / steps as f32),
                                at((i as f32 + 0.5) / steps as f32),
                            )
                        })
                        .collect();
                    mesh_edges::paint_edges(&closure, bounds.origin, |path| {
                        window.paint_path(path, primary.opacity(0.7));
                    });
                }
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
