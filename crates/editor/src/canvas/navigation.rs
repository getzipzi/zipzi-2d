//! Input-device-specific navigation. Only discrete wheel input needs interpolation.
use super::*;
use editor_core::{MAX_ZOOM, MIN_ZOOM, ViewState, ViewportBinding};
use std::time::{Duration, Instant};

const WHEEL_ZOOM_DURATION: Duration = Duration::from_millis(80);

// One wheel line scales by 1.1; reversing the same delta restores the old scale.
fn scroll_zoom(zoom: f32, lines: f32) -> f32 {
    let log_zoom = zoom.ln() + lines * 1.1_f32.ln();
    if log_zoom <= MIN_ZOOM.ln() {
        MIN_ZOOM
    } else if log_zoom >= MAX_ZOOM.ln() {
        MAX_ZOOM
    } else {
        log_zoom.exp().clamp(MIN_ZOOM, MAX_ZOOM)
    }
}

pub(super) struct WheelZoom {
    binding: ViewportBinding,
    bounds: Bounds<Pixels>,
    metrics: CanvasMetrics,
    expected: ViewState,
    anchor: CanvasPoint,
    from: f32,
    target: f32,
    direction: f32,
    started: Instant,
    input: u64,
}

impl WheelZoom {
    pub(super) fn valid(&self, owner: &EditorView, bounds: Bounds<Pixels>) -> bool {
        !owner.modal
            && !owner.editor.is_busy()
            && !owner.editor.gesture_active()
            && owner.editor.viewport_binding_valid(self.binding)
            && owner.editor.canvas_metrics() == self.metrics
            && bounds == self.bounds
            && owner.editor.view().zoom == self.expected.zoom
            && owner.editor.view().pan == self.expected.pan
    }

    fn sample(&self, now: Instant) -> (f32, bool) {
        let progress = (now.duration_since(self.started).as_secs_f32()
            / WHEEL_ZOOM_DURATION.as_secs_f32())
        .clamp(0., 1.);
        if progress == 1. {
            return (self.target, true);
        }
        let eased = 1. - (1. - progress).powi(3);
        let zoom = (self.from.ln() + (self.target.ln() - self.from.ln()) * eased).exp();
        (zoom, false)
    }
}

impl CanvasView {
    pub(super) fn scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.touch_phase == TouchPhase::Cancelled {
            self.wheel_zoom = None;
            return;
        }
        if window.has_active_dialog(cx) || self.pan.is_some() {
            return;
        }
        let precise = event.delta.precise();
        // Native trackpad momentum is already continuous. Do not add a second tail.
        if precise {
            self.wheel_zoom = None;
        }
        let delta = event.delta.pixel_delta(px(16.));
        let delta = CanvasPoint {
            x: delta.x.as_f32(),
            y: delta.y.as_f32(),
        };
        if !delta.x.is_finite() || !delta.y.is_finite() || (delta.x == 0. && delta.y == 0.) {
            return;
        }
        let pan = precise && !event.modifiers.control && !event.modifiers.platform;
        let anchor = self.local(event.position);
        let now = cx.background_executor().now();
        let animate = !precise && !cx.reduce_motion();
        let owner = self.owner.clone();
        let _ = owner.update(cx, |owner, cx| {
            if owner.modal
                || owner.editor.is_busy()
                || owner.editor.gesture_active()
                || self.binding(&owner.editor).is_none()
            {
                self.wheel_zoom = None;
                return;
            }
            cx.stop_propagation();
            if !pan && delta.y == 0. {
                return;
            }
            let before = owner.editor.view();
            if pan {
                owner.editor.pan_by(delta);
            } else if animate {
                let Some(binding) = self.binding(&owner.editor) else {
                    return;
                };
                let base = self
                    .wheel_zoom
                    .as_ref()
                    .filter(|zoom| {
                        zoom.valid(owner, self.bounds) && zoom.direction == delta.y.signum()
                    })
                    .map_or(before.zoom, |zoom| zoom.target);
                let target = scroll_zoom(base, delta.y / 16.);
                // Reversal discards unfinished movement in the previous direction.
                self.wheel_zoom = (target != before.zoom).then_some(WheelZoom {
                    binding,
                    bounds: self.bounds,
                    metrics: owner.editor.canvas_metrics(),
                    expected: before,
                    anchor,
                    from: before.zoom,
                    target,
                    direction: delta.y.signum(),
                    started: now,
                    input: frame_trace::current_input(),
                });
            } else {
                self.wheel_zoom = None;
                owner
                    .editor
                    .set_zoom_at(scroll_zoom(before.zoom, delta.y / 16.), Some(anchor));
            }
            if owner.editor.view().zoom != before.zoom || owner.editor.view().pan != before.pan {
                owner.redraw_canvas(cx);
            }
        });
        self.schedule_zoom_frame(window, cx);
    }

    fn schedule_zoom_frame(&mut self, window: &Window, cx: &Context<Self>) {
        if self.wheel_zoom.is_none() || self.zoom_frame_pending {
            return;
        }
        self.zoom_frame_pending = true;
        let view = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = view.update(cx, |this, cx| {
                this.zoom_frame_pending = false;
                this.advance_wheel_zoom(window, cx);
            });
        });
    }

    fn advance_wheel_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut zoom) = self.wheel_zoom.take() else {
            return;
        };
        if window.has_active_dialog(cx) || self.pan.is_some() {
            return;
        }
        let now = cx.background_executor().now();
        let owner = self.owner.clone();
        let _ = owner.update(cx, |owner, cx| {
            if !zoom.valid(owner, self.bounds) {
                return;
            }
            let (next, done) = if cx.reduce_motion() {
                (zoom.target, true)
            } else {
                zoom.sample(now)
            };
            let before = owner.editor.view();
            owner.editor.set_zoom_at(next, Some(zoom.anchor));
            zoom.expected = owner.editor.view();
            if zoom.expected.zoom != before.zoom || zoom.expected.pan != before.pan {
                owner.redraw_canvas_for_input(cx, zoom.input);
            }
            if !done {
                self.wheel_zoom = Some(zoom);
            }
        });
        self.schedule_zoom_frame(window, cx);
    }

    pub(super) fn pinch(
        &mut self,
        event: &PinchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Even a zero-delta gesture start takes ownership from the wheel.
        self.wheel_zoom = None;
        if window.has_active_dialog(cx)
            || self.pan.is_some()
            || event.phase == TouchPhase::Cancelled
            || !event.delta.is_finite()
            || event.delta == 0.
            || event.delta <= -1.
        {
            return;
        }
        let anchor = self.local(event.position);
        let owner = self.owner.clone();
        let _ = owner.update(cx, |owner, cx| {
            if owner.modal
                || owner.editor.is_busy()
                || owner.editor.gesture_active()
                || self.binding(&owner.editor).is_none()
            {
                return;
            }
            let before = owner.editor.view();
            owner
                .editor
                .set_zoom_at(before.zoom * (1. + event.delta), Some(anchor));
            if owner.editor.view().zoom != before.zoom || owner.editor.view().pan != before.pan {
                owner.redraw_canvas(cx);
            }
            cx.stop_propagation();
        });
    }
}
