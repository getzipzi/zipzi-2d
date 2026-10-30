//! Input-device-specific navigation. Only discrete wheel input needs interpolation.
use super::*;
use std::time::{Duration, Instant};
use zipzi_core::SceneTrackEvaluation;
use zipzi_editor_core::{MAX_ZOOM, MIN_ZOOM, ViewState, ViewportBinding};

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

#[derive(Default)]
pub(super) struct ViewportNavigation {
    pub pan: Option<PanGesture>,
    pub wheel_zoom: Option<WheelZoom>,
    pub zoom_frame_pending: bool,
}

pub(super) struct WheelZoom {
    binding: ViewportBinding,
    bounds: Bounds<Pixels>,
    metrics: ViewportMetrics,
    expected: ViewState,
    anchor: ViewportPoint,
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
            && owner.editor.viewport_metrics(self.binding.viewport) == Some(self.metrics)
            && bounds == self.bounds
            && owner.editor.viewport_view(self.binding) == Some(self.expected)
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

impl ViewportSurface {
    pub(super) fn scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.touch_phase == TouchPhase::Cancelled {
            self.navigation.wheel_zoom = None;
            return;
        }
        if window.has_active_dialog(cx) || self.navigation.pan.is_some() {
            return;
        }
        let precise = event.delta.precise();
        // Native trackpad momentum is already continuous. Do not add a second tail.
        if precise {
            self.navigation.wheel_zoom = None;
        }
        let delta = event.delta.pixel_delta(px(16.));
        let delta = ViewportPoint {
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
            if owner.modal || owner.editor.is_busy() || owner.editor.gesture_active() {
                self.navigation.wheel_zoom = None;
                return;
            }
            let Some(binding) = self.binding(&owner.editor) else {
                return;
            };
            cx.stop_propagation();
            if !pan && delta.y == 0. {
                return;
            }
            let before = owner.editor.viewport_view(binding).unwrap();
            if pan {
                let _ = owner.editor.pan_viewport(binding, delta);
            } else if animate {
                let base = self
                    .navigation
                    .wheel_zoom
                    .as_ref()
                    .filter(|zoom| {
                        zoom.valid(owner, self.bounds) && zoom.direction == delta.y.signum()
                    })
                    .map_or(before.zoom, |zoom| zoom.target);
                let target = scroll_zoom(base, delta.y / 16.);
                // Reversal discards unfinished movement in the previous direction.
                self.navigation.wheel_zoom = (target != before.zoom).then_some(WheelZoom {
                    binding,
                    bounds: self.bounds,
                    metrics: owner.editor.viewport_metrics(binding.viewport).unwrap(),
                    expected: before,
                    anchor,
                    from: before.zoom,
                    target,
                    direction: delta.y.signum(),
                    started: now,
                    input: frame_trace::current_input(),
                });
            } else {
                self.navigation.wheel_zoom = None;
                let _ = owner.editor.set_viewport_zoom(
                    binding,
                    scroll_zoom(before.zoom, delta.y / 16.),
                    Some(anchor),
                );
            }
            if owner.editor.viewport_view(binding) != Some(before) {
                owner.redraw_canvas(cx);
            }
        });
        self.schedule_zoom_frame(window, cx);
    }

    fn schedule_zoom_frame(&mut self, window: &Window, cx: &Context<Self>) {
        if self.navigation.wheel_zoom.is_none() || self.navigation.zoom_frame_pending {
            return;
        }
        self.navigation.zoom_frame_pending = true;
        let view = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = view.update(cx, |this, cx| {
                this.navigation.zoom_frame_pending = false;
                this.advance_wheel_zoom(window, cx);
            });
        });
    }

    fn advance_wheel_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut zoom) = self.navigation.wheel_zoom.take() else {
            return;
        };
        if window.has_active_dialog(cx) || self.navigation.pan.is_some() {
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
            let binding = zoom.binding;
            let before = owner.editor.viewport_view(binding).unwrap();
            let _ = owner
                .editor
                .set_viewport_zoom(binding, next, Some(zoom.anchor));
            zoom.expected = owner.editor.viewport_view(binding).unwrap();
            if zoom.expected.zoom != before.zoom || zoom.expected.pan != before.pan {
                owner.redraw_canvas_for_input(cx, zoom.input);
            }
            if !done {
                self.navigation.wheel_zoom = Some(zoom);
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
        self.navigation.wheel_zoom = None;
        if window.has_active_dialog(cx)
            || self.navigation.pan.is_some()
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
            if owner.modal || owner.editor.is_busy() || owner.editor.gesture_active() {
                return;
            }
            let Some(binding) = self.binding(&owner.editor) else {
                return;
            };
            let before = owner.editor.viewport_view(binding).unwrap();
            let _ = owner.editor.set_viewport_zoom(
                binding,
                before.zoom * (1. + event.delta),
                Some(anchor),
            );
            if owner.editor.viewport_view(binding) != Some(before) {
                owner.redraw_canvas(cx);
            }
            cx.stop_propagation();
        });
    }
}
