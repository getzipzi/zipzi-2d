use kasane_core::Canvas;
mod identity;
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
    // Document source pixels, Y-down; SDK normalized and object-local points
    // are converted at their respective boundaries before reaching this type.
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
    pub fn new(canvas: Canvas, metrics: ViewportMetrics, view: crate::ViewState) -> Option<Self> {
        if !metrics.visible()
            || !canvas.width.is_finite()
            || !canvas.height.is_finite()
            || canvas.width <= 0.
            || canvas.height <= 0.
        {
            return None;
        }
        // Zoom is physical display pixels per source pixel. UI coordinates are logical pixels.
        let scale = view.zoom / metrics.scale_factor;
        if !scale.is_finite() || scale <= 0. {
            return None;
        }
        Some(Self {
            scale,
            offset: ViewportPoint {
                x: (metrics.width - canvas.width * scale) / 2. + view.pan.x,
                y: (metrics.height - canvas.height * scale) / 2. + view.pan.y,
            },
            metrics,
        })
    }
    pub fn to_viewport(self, p: ModelPoint) -> ViewportPoint {
        ViewportPoint {
            x: p.x * self.scale + self.offset.x,
            y: p.y * self.scale + self.offset.y,
        }
    }
    pub fn to_model(self, p: ViewportPoint) -> ModelPoint {
        ModelPoint {
            x: (p.x - self.offset.x) / self.scale,
            y: (p.y - self.offset.y) / self.scale,
        }
    }
    /// SDK Drawable positions are normalized Y-up; authoring mesh positions are source pixels Y-down.
    pub fn evaluated_to_source(canvas: Canvas, p: kasane_core::Vec2) -> ModelPoint {
        ModelPoint {
            x: p.x * canvas.pixels_per_unit + canvas.origin.x,
            y: canvas.origin.y - p.y * canvas.pixels_per_unit,
        }
    }
}
