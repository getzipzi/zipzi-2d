//! Export form values are independent of view framing and parameter previews.
use anyhow::{Result, ensure};
use zipzi_core::{Canvas, ExportSettings};
pub use zipzi_project::{TransferFormat, TransferSnapshot};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportCanvas {
    pub manual: bool,
    pub width: f32,
    pub height: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub settings: ExportSettings,
}
impl ExportCanvas {
    pub(crate) fn new(settings: ExportSettings, canvas: Canvas) -> Self {
        Self {
            manual: settings.bounds.is_some(),
            width: canvas.width,
            height: canvas.height,
            origin_x: canvas.origin.x,
            origin_y: canvas.origin.y,
            settings,
        }
    }
    pub fn validated(self) -> Result<ExportSettings> {
        let mut settings = self.settings;
        settings.bounds = if self.manual {
            ensure!(
                [self.width, self.height, self.origin_x, self.origin_y]
                    .iter()
                    .all(|v| v.is_finite())
                    && self.width > 0.
                    && self.height > 0.,
                "Invalid canvas size or origin"
            );
            Some([
                -self.origin_x,
                self.origin_y - self.height,
                self.width - self.origin_x,
                self.origin_y,
            ])
        } else {
            None
        };
        ensure!(settings.is_valid(), "Invalid export settings");
        Ok(settings)
    }
}
