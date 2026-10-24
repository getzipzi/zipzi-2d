//! Navigation belongs to a document and a runtime viewport, never a layout slot.
use crate::{MAX_ZOOM, MIN_ZOOM, ViewState, ViewportMetrics, ViewportPoint};

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
    /// Center the visible content and choose a scale that keeps it in view.
    pub(crate) fn fit(&mut self, content_bounds: Option<[f32; 4]>, metrics: ViewportMetrics) {
        if !metrics.visible() {
            return;
        }
        let mut fitted = ViewState::default();
        if let Some([x0, y0, x1, y1]) = content_bounds {
            if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) || x1 < x0 || y1 < y0 {
                return;
            }
            let w = (f64::from(x1) - f64::from(x0)).max(1.);
            let h = (f64::from(y1) - f64::from(y0)).max(1.);
            fitted.zoom = ((f64::from(metrics.width) / w).min(f64::from(metrics.height) / h) * 0.9)
                .clamp(f64::from(MIN_ZOOM), f64::from(MAX_ZOOM)) as f32;
            let scale = f64::from(fitted.zoom);
            fitted.pan = ViewportPoint {
                x: -((f64::from(x0) + f64::from(x1)) * 0.5 * scale) as f32,
                y: ((f64::from(y0) + f64::from(y1)) * 0.5 * scale) as f32,
            };
            if crate::ViewportTransform::new(metrics, fitted).is_none() {
                return;
            }
        }
        if self.view != fitted {
            self.view = fitted;
            self.revision += 1;
        }
    }
    pub(crate) fn set_zoom(
        &mut self,
        zoom: f32,
        focus: Option<ViewportPoint>,
        metrics: ViewportMetrics,
    ) {
        if zoom.is_finite() {
            self.default_fit = false;
            if self.view.zoom_about(zoom, focus, metrics) {
                self.revision += 1;
            }
        }
    }
    pub(crate) fn pan(&mut self, delta: ViewportPoint) {
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

impl ViewState {
    /// Shared navigation math for editor surfaces and dialog previews. Points
    /// are logical pixels relative to the viewport; zoom is logical pixels per model unit.
    pub fn zoom_about(
        &mut self,
        zoom: f32,
        focus: Option<ViewportPoint>,
        metrics: ViewportMetrics,
    ) -> bool {
        if !zoom.is_finite() {
            return false;
        }
        let next = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if next == self.zoom {
            return false;
        }
        if metrics.visible() && self.zoom > 0. {
            let ratio = next / self.zoom;
            let anchor = focus.unwrap_or(ViewportPoint {
                x: metrics.width / 2.,
                y: metrics.height / 2.,
            });
            if anchor.x.is_finite() && anchor.y.is_finite() {
                self.pan.x += (1. - ratio) * (anchor.x - metrics.width / 2. - self.pan.x);
                self.pan.y += (1. - ratio) * (anchor.y - metrics.height / 2. - self.pan.y);
            }
        }
        self.zoom = next;
        true
    }
}
