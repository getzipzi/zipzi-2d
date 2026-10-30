//! Explicit model-space views for inspection renders.

use crate::ObservationError;
use zipzi_sdk::TopologyOverlayValidation;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelRoi {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderRequest {
    pub width: u32,
    pub height: u32,
    pub roi: ModelRoi,
    pub padding_model: f32,
    /// Optional topology diagnostics composited over this exact view.
    pub topology: Option<zipzi_sdk::TopologyOverlay>,
    /// Include mesh quality and alpha coverage reports. Alpha threshold is 8.
    pub quality: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewMapping {
    /// Entire output viewport expressed in model units. Its extent
    /// may exceed the padded ROI when the output aspect ratio differs.
    pub visible_roi: ModelRoi,
    pub scale: f32,
    pub offset: (f32, f32),
}

fn invalid(message: &'static str) -> ObservationError {
    ObservationError {
        code: "INVALID_VIEW".into(),
        message: message.into(),
        asset_id: None,
    }
}

impl RenderRequest {
    pub fn needs_quality(&self) -> bool {
        self.quality || self.topology.as_ref().is_some_and(|o| o.needs_quality())
    }
    pub fn mapping(&self) -> Result<ViewMapping, ObservationError> {
        if let Some(overlay) = &self.topology {
            overlay.validate().map_err(|e| ObservationError {
                code: e.code.into(),
                message: e.message.into(),
                asset_id: None,
            })?;
        }
        let ModelRoi { x0, y0, x1, y1 } = self.roi;
        if self.width == 0 || self.height == 0 {
            return Err(invalid("Output dimensions must be positive"));
        }
        if ![x0, y0, x1, y1, self.padding_model]
            .into_iter()
            .all(f32::is_finite)
            || x1 <= x0
            || y1 <= y0
            || self.padding_model < 0.0
        {
            return Err(invalid(
                "ROI must be finite, nonempty, and have nonnegative padding",
            ));
        }
        let padded_roi = ModelRoi {
            x0: x0 - self.padding_model,
            y0: y0 - self.padding_model,
            x1: x1 + self.padding_model,
            y1: y1 + self.padding_model,
        };
        let roi_width = padded_roi.x1 - padded_roi.x0;
        let roi_height = padded_roi.y1 - padded_roi.y0;
        let scale = (self.width as f32 / roi_width).min(self.height as f32 / roi_height);
        if ![
            padded_roi.x0,
            padded_roi.y0,
            padded_roi.x1,
            padded_roi.y1,
            scale,
        ]
        .into_iter()
        .all(f32::is_finite)
            || scale <= 0.0
        {
            return Err(invalid("ROI scale is outside the representable range"));
        }
        let offset = (
            (self.width as f32 - scale * roi_width) * 0.5 - scale * padded_roi.x0,
            (self.height as f32 - scale * roi_height) * 0.5 + scale * padded_roi.y1,
        );
        if ![offset.0, offset.1].into_iter().all(f32::is_finite) {
            return Err(invalid("ROI offset is outside the representable range"));
        }
        let visible_roi = ModelRoi {
            x0: -offset.0 / scale,
            y0: (offset.1 - self.height as f32) / scale,
            x1: (self.width as f32 - offset.0) / scale,
            y1: offset.1 / scale,
        };
        if ![
            visible_roi.x0,
            visible_roi.y0,
            visible_roi.x1,
            visible_roi.y1,
            1.0 / scale,
        ]
        .into_iter()
        .all(f32::is_finite)
        {
            return Err(invalid(
                "Inverse ROI mapping is outside the representable range",
            ));
        }
        Ok(ViewMapping {
            visible_roi,
            scale,
            offset,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_square_view_preserves_aspect_and_centers_roi() {
        let mapping = RenderRequest {
            width: 1024,
            height: 512,
            roi: ModelRoi {
                x0: -10.25,
                y0: 17.5,
                x1: 89.75,
                y1: 67.5,
            },
            padding_model: 5.0,
            topology: None,
            quality: false,
        }
        .mapping()
        .unwrap();
        assert!(mapping.visible_roi.x0 < -15.25);
        assert!(mapping.visible_roi.x1 > 94.75);
        let center = (39.75, 42.5);
        assert!((center.0 * mapping.scale + mapping.offset.0 - 512.).abs() < 0.0001);
        assert!((-center.1 * mapping.scale + mapping.offset.1 - 256.).abs() < 0.0001);
    }

    #[test]
    fn rejects_overflowing_inverse_view() {
        let request = RenderRequest {
            width: 4096,
            height: 1,
            roi: ModelRoi {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: f32::MAX,
            },
            padding_model: 0.0,
            topology: None,
            quality: false,
        };
        assert_eq!(request.mapping().unwrap_err().code, "INVALID_VIEW");
    }

    #[test]
    fn rejects_nonfinite_or_empty_view() {
        let request = RenderRequest {
            width: 128,
            height: 64,
            roi: ModelRoi {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
            },
            padding_model: 0.0,
            topology: None,
            quality: false,
        };
        assert_eq!(
            RenderRequest {
                roi: ModelRoi {
                    x1: 0.0,
                    ..request.roi
                },
                ..request.clone()
            }
            .mapping()
            .unwrap_err()
            .code,
            "INVALID_VIEW"
        );
        assert_eq!(
            RenderRequest {
                padding_model: f32::NAN,
                ..request
            }
            .mapping()
            .unwrap_err()
            .code,
            "INVALID_VIEW"
        );
    }
}
