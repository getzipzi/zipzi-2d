#[cfg(test)]
use kasane_core::Canvas;
use kasane_core::ModelSpace;
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
    // Document source pixels, Y-down; SDK origin-relative and object-local points
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
    pub fn new(
        canvas: impl Into<ModelSpace>,
        metrics: ViewportMetrics,
        view: crate::ViewState,
    ) -> Option<Self> {
        let canvas = canvas.into();
        if !metrics.visible() || !canvas.is_valid() {
            return None;
        }
        // Zoom is physical display pixels per source pixel. UI coordinates are logical pixels.
        let scale = view.zoom / metrics.scale_factor;
        if !scale.is_finite() || scale <= 0. || !view.pan.x.is_finite() || !view.pan.y.is_finite() {
            return None;
        }
        let center = canvas.origin;
        let offset = ViewportPoint {
            x: metrics.width / 2. - center.x * scale + view.pan.x,
            y: metrics.height / 2. - center.y * scale + view.pan.y,
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
            y: p.y * self.scale + self.offset.y,
        }
    }
    pub fn to_model(self, p: ViewportPoint) -> ModelPoint {
        ModelPoint {
            x: (p.x - self.offset.x) / self.scale,
            y: (p.y - self.offset.y) / self.scale,
        }
    }
    /// SDK Drawable positions are pixel-sized Y-up model units; authoring mesh positions are source pixels Y-down.
    pub fn evaluated_to_source(canvas: ModelSpace, p: kasane_core::Vec2) -> ModelPoint {
        ModelPoint {
            x: p.x + canvas.origin.x,
            y: canvas.origin.y - p.y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundless_canvas_centers_on_origin() {
        let canvas = ModelSpace::default();
        let metrics = ViewportMetrics {
            width: 800.,
            height: 600.,
            scale_factor: 2.,
        };
        let view = crate::ViewState {
            zoom: 2.,
            pan: ViewportPoint { x: 50., y: -30. },
        };
        let transform = ViewportTransform::new(canvas, metrics, view).unwrap();
        assert_eq!(transform.scale, 1.0);
        // origin (0, 0) should be at viewport center (400, 300) + pan (50, -30) = (450, 270)
        let origin_vp = transform.to_viewport(ModelPoint { x: 0., y: 0. });
        assert_eq!(origin_vp, ViewportPoint { x: 450., y: 270. });
    }

    #[test]
    fn boundless_does_not_accept_invalid_canvas_or_navigation_values() {
        let metrics = ViewportMetrics {
            width: 800.,
            height: 600.,
            scale_factor: 2.,
        };
        for canvas in [
            ModelSpace::new(kasane_core::Vec2::new(f32::INFINITY, 0.)),
            ModelSpace::new(kasane_core::Vec2::new(f32::INFINITY, 0.)),
            ModelSpace::new(kasane_core::Vec2::new(f32::NAN, 0.)),
            ModelSpace::new(kasane_core::Vec2::new(f32::INFINITY, 0.)),
        ] {
            assert!(ViewportTransform::new(canvas, metrics, crate::ViewState::default()).is_none());
        }
        assert!(
            ViewportTransform::new(
                ModelSpace::default(),
                metrics,
                crate::ViewState {
                    pan: ViewportPoint { x: f32::NAN, y: 0. },
                    ..Default::default()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn fixed_cubism_canvas_centers_on_canvas_box() {
        let canvas = Canvas::new(1000., 800., kasane_core::Vec2::new(500., 400.), 100.);
        let metrics = ViewportMetrics {
            width: 800.,
            height: 600.,
            scale_factor: 1.,
        };
        let view = crate::ViewState {
            zoom: 1.,
            pan: ViewportPoint::default(),
        };
        let transform = ViewportTransform::new(canvas, metrics, view).unwrap();
        // center (500, 400) should be at (400, 300)
        let center_vp = transform.to_viewport(ModelPoint { x: 500., y: 400. });
        assert_eq!(center_vp, ViewportPoint { x: 400., y: 300. });
        // top-left (0, 0) should be at (400 - 500, 300 - 400) = (-100, -100)
        let topleft_vp = transform.to_viewport(ModelPoint { x: 0., y: 0. });
        assert_eq!(topleft_vp, ViewportPoint { x: -100., y: -100. });
    }
}
