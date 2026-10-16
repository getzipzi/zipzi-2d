//! Navigation belongs to a document and a runtime viewport, never a layout slot.
use crate::{CanvasMetrics, CanvasPoint, MAX_ZOOM, MIN_ZOOM, ViewState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportState {
    pub view: ViewState,
    pub default_fit: bool,
    pub revision: u64,
}
impl Default for ViewportState {
    fn default() -> Self {
        Self {
            view: ViewState::default(),
            default_fit: true,
            revision: 1,
        }
    }
}
impl ViewportState {
    pub(crate) fn fit(&mut self, canvas: kasane_core::Canvas, metrics: CanvasMetrics) {
        if !metrics.visible() || canvas.width <= 0. || canvas.height <= 0. {
            return;
        }
        let fitted = ViewState {
            zoom: ((metrics.width / canvas.width).min(metrics.height / canvas.height)
                * metrics.scale_factor
                * 0.9)
                .clamp(MIN_ZOOM, MAX_ZOOM),
            pan: CanvasPoint::default(),
        };
        if self.view.zoom != fitted.zoom || self.view.pan != fitted.pan {
            self.view = fitted;
            self.revision += 1;
        }
    }
    pub(crate) fn set_zoom(
        &mut self,
        zoom: f32,
        focus: Option<CanvasPoint>,
        metrics: CanvasMetrics,
    ) {
        if !zoom.is_finite() {
            return;
        }
        self.default_fit = false;
        let old_zoom = self.view.zoom;
        let new_zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if new_zoom == old_zoom {
            return;
        }
        if metrics.visible() && old_zoom > 0. {
            let k = new_zoom / old_zoom;
            let anchor = focus.unwrap_or(CanvasPoint {
                x: metrics.width / 2.,
                y: metrics.height / 2.,
            });
            if anchor.x.is_finite() && anchor.y.is_finite() {
                let vx = anchor.x - metrics.width / 2.;
                let vy = anchor.y - metrics.height / 2.;
                self.view.pan.x += (1. - k) * (vx - self.view.pan.x);
                self.view.pan.y += (1. - k) * (vy - self.view.pan.y);
            }
        }
        self.view.zoom = new_zoom;
        self.revision += 1;
    }
    pub(crate) fn pan(&mut self, delta: CanvasPoint) {
        if delta.x.is_finite() && delta.y.is_finite() && (delta.x != 0. || delta.y != 0.) {
            self.default_fit = false;
            self.view.pan.x += delta.x;
            self.view.pan.y += delta.y;
            self.revision += 1;
        }
    }
    pub(crate) fn reset(&mut self) {
        self.default_fit = false;
        self.view = ViewState::default();
        self.revision += 1;
    }
}
