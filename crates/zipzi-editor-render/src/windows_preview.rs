use anyhow::{Result, ensure};
use zipzi_editor_core::ViewportTransform;

pub(crate) const MAX_SLOTS: usize = 8;
pub(crate) const MAX_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_FRAME_BYTES: u64 = MAX_BYTES / MAX_SLOTS as u64;

/// Bound raster resolution without changing logical viewport coordinates.
pub fn fit_windows_preview(
    mut extent: [u32; 2],
    mut transform: ViewportTransform,
) -> Result<([u32; 2], ViewportTransform)> {
    ensure!(
        extent.into_iter().all(|v| (1..=16384).contains(&v)),
        "Invalid Windows preview extent"
    );
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 4;
    if bytes > MAX_FRAME_BYTES {
        let scale = (MAX_FRAME_BYTES as f64 / bytes as f64).sqrt();
        extent = extent.map(|value| (f64::from(value) * scale).floor().max(1.) as u32);
        transform.metrics.scale_factor *= scale as f32;
    }
    Ok((extent, transform))
}
