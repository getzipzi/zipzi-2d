mod identity;
mod interaction;
pub use interaction::*;
mod isolation;
pub use isolation::IsolationDisplay;
mod presentation;
mod state;
pub(crate) use identity::ViewportRegistry;
pub use identity::*;
pub use presentation::AnimationViewportIdentity;
pub(crate) use presentation::ViewportPresentation;
pub use state::ViewportState;

/// Window coordinates enter only at the UI boundary; core accepts viewport-local logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewportPoint {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelPoint {
    // Origin-relative model coordinates, X-right/Y-up.
    pub x: f32,
    pub y: f32,
}
/// Displacement in model units. It has no position or source-origin offset.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelVector {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewportMetrics {
    pub width: f32,
    pub height: f32,
    pub scale_factor: f32,
}
impl ViewportMetrics {
    pub fn visible(self) -> bool {
        [self.width, self.height, self.scale_factor]
            .iter()
            .all(|v| v.is_finite() && *v > 0.)
    }
    pub fn physical_size(self) -> Option<(u32, u32)> {
        self.visible().then(|| {
            (
                (self.width * self.scale_factor).round().clamp(1., 16384.) as u32,
                (self.height * self.scale_factor).round().clamp(1., 16384.) as u32,
            )
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportTransform {
    pub scale: f32,
    pub offset: ViewportPoint,
    pub metrics: ViewportMetrics,
}
impl ViewportTransform {
    pub fn new(metrics: ViewportMetrics, view: crate::ViewState) -> Option<Self> {
        if !metrics.visible() {
            return None;
        }
        // Logical display pixels per model unit; DPI only changes render resolution.
        let scale = view.zoom;
        if !scale.is_finite() || scale <= 0. || !view.pan.x.is_finite() || !view.pan.y.is_finite() {
            return None;
        }
        let offset = ViewportPoint {
            x: metrics.width / 2. + view.pan.x,
            y: metrics.height / 2. + view.pan.y,
        };
        (offset.x.is_finite() && offset.y.is_finite()).then_some(Self {
            scale,
            offset,
            metrics,
        })
    }
    pub fn to_viewport(self, p: ModelPoint) -> ViewportPoint {
        ViewportPoint {
            x: p.x * self.scale + self.offset.x,
            y: -p.y * self.scale + self.offset.y,
        }
    }
    pub fn to_model(self, p: ViewportPoint) -> ModelPoint {
        ModelPoint {
            x: (p.x - self.offset.x) / self.scale,
            y: (self.offset.y - p.y) / self.scale,
        }
    }
}
