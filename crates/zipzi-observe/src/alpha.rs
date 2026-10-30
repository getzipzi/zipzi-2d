//! Source-coordinate alpha samples, shared across all poses in one capture.
use crate::{check_capture, ObservationError};
use std::sync::Arc;
use zipzi_contract::runtime::ObservationTextureSize;

pub const SOURCE_ALPHA_SAMPLING_VERSION: u32 = 1;

#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "gpu"), allow(dead_code))]
pub(crate) struct SourceAlphaSamples {
    pub source_size: ObservationTextureSize,
    pub step: u32,
    pub cols: usize,
    pub rows: usize,
    pub alpha: Arc<[u8]>,
    pub version: u32,
}

impl SourceAlphaSamples {
    pub fn extract(
        width: u32,
        height: u32,
        rgba: &[u8],
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Self, ObservationError> {
        check_capture(control)?;
        let length = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || length != Some(rgba.len()) {
            return Err(ObservationError {
                code: "RESOURCE_DIMENSIONS".into(),
                message: "Invalid source alpha dimensions".into(),
                asset_id: None,
            });
        }
        let step = width.max(height).div_ceil(256).max(1);
        let (cols, rows) = (
            width.div_ceil(step) as usize,
            height.div_ceil(step) as usize,
        );
        let mut alpha = Vec::with_capacity(cols * rows);
        for gy in 0..rows {
            check_capture(control)?;
            for gx in 0..cols {
                let offset = ((gy * step as usize * width as usize) + gx * step as usize) * 4 + 3;
                alpha.push(rgba[offset]);
            }
        }
        Ok(Self {
            source_size: ObservationTextureSize { width, height },
            step,
            cols,
            rows,
            alpha: alpha.into(),
            version: SOURCE_ALPHA_SAMPLING_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_keep_original_coordinates_on_odd_and_single_pixel_axes() {
        for (width, height) in [(3, 5), (1, 257), (513, 1), (513, 257)] {
            let rgba: Vec<_> = (0..width * height)
                .flat_map(|i| [0, 0, 0, (i % 251) as u8])
                .collect();
            let samples =
                SourceAlphaSamples::extract(width, height, &rgba, &Default::default()).unwrap();
            assert!(samples.cols <= 256 && samples.rows <= 256);
            assert_eq!(samples.version, SOURCE_ALPHA_SAMPLING_VERSION);
            for y in 0..samples.rows {
                for x in 0..samples.cols {
                    let source =
                        y * samples.step as usize * width as usize + x * samples.step as usize;
                    assert_eq!(samples.alpha[y * samples.cols + x], (source % 251) as u8);
                }
            }
        }
    }
    #[test]
    fn extraction_cancels_before_allocating_samples() {
        let control = zipzi_sdk::ConversionControl::default();
        control.cancel();
        assert_eq!(
            SourceAlphaSamples::extract(1, 1, &[255; 4], &control)
                .unwrap_err()
                .code,
            "CANCELLED"
        );
    }
}
