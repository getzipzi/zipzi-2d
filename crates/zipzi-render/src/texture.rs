//! Source-image preparation. Authored pixels remain straight RGBA; GPU filtering
//! must interpolate premultiplied colors to exclude invisible texels' RGB.

/// Convert straight RGBA8 to premultiplied RGBA8 without changing the source.
pub fn premultiply_rgba8(rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len() % 4, 0);
    let mut result = rgba.to_vec();
    for pixel in result.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        for color in &mut pixel[..3] {
            *color = ((u16::from(*color) * alpha + 127) / 255) as u8;
        }
    }
    result
}

/// Version of the alpha-weighted area average, in the existing channel space.
pub const STRAIGHT_RGBA_AREA_FILTER_VERSION: u32 = 1;
/// Existing recursive mipmap filtering version, including odd-axis footprints.
pub const STRAIGHT_RGBA_MIPMAP_VERSION: u32 = 2;

/// Direct area-average downsampling of straight, top-left row-major RGBA8.
/// RGB is weighted by alpha; alpha is averaged over the exact footprint.
/// Channels round to the nearest integer, with ties rounding upward. Zero
/// accumulated alpha yields zero RGB. An unchanged size preserves every byte,
/// including invisible RGB. Mipmaps may call this for each successive level;
/// observation thumbnails must call it once on the original source pixels.
pub fn area_average_rgba8(
    width: u32,
    height: u32,
    rgba: &[u8],
    target_width: u32,
    target_height: u32,
) -> Vec<u8> {
    area_average_rgba8_with_control(width, height, rgba, target_width, target_height, || true)
        .expect("unconditional area filtering")
}

/// As above, cooperatively cancelling before allocation and during scanlines.
/// Returning None discards partial output. The caller reserves its workspace
/// before entering this function; it does not perform Runtime admission.
pub fn area_average_rgba8_with_control(
    width: u32,
    height: u32,
    rgba: &[u8],
    target_width: u32,
    target_height: u32,
    mut keep_going: impl FnMut() -> bool,
) -> Option<Vec<u8>> {
    assert!(width > 0 && height > 0);
    assert!(target_width > 0 && target_width <= width);
    assert!(target_height > 0 && target_height <= height);
    let source_len = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(4));
    assert_eq!(source_len, Some(rgba.len() as u64));
    if !keep_going() {
        return None;
    }
    if width == target_width && height == target_height {
        return Some(rgba.to_vec());
    }
    let mut next = vec![0u8; target_width as usize * target_height as usize * 4];
    for y in 0..target_height {
        if !keep_going() {
            return None;
        }
        for x in 0..target_width {
            let x0 = x as f64 * width as f64 / target_width as f64;
            let x1 = (x + 1) as f64 * width as f64 / target_width as f64;
            let y0 = y as f64 * height as f64 / target_height as f64;
            let y1 = (y + 1) as f64 * height as f64 / target_height as f64;
            let mut sums = [0.0; 4];
            for sy in y0.floor() as u32..y1.ceil() as u32 {
                if !keep_going() {
                    return None;
                }
                for sx in x0.floor() as u32..x1.ceil() as u32 {
                    if sx % 4096 == 0 && !keep_going() {
                        return None;
                    }
                    let weight = (x1.min((sx + 1) as f64) - x0.max(sx as f64))
                        * (y1.min((sy + 1) as f64) - y0.max(sy as f64));
                    let offset = (sy as usize * width as usize + sx as usize) * 4;
                    let alpha = f64::from(rgba[offset + 3]);
                    for channel in 0..3 {
                        sums[channel] += f64::from(rgba[offset + channel]) * alpha * weight;
                    }
                    sums[3] += alpha * weight;
                }
            }
            let area = (x1 - x0) * (y1 - y0);
            let offset = (y as usize * target_width as usize + x as usize) * 4;
            if sums[3] > 0.0 {
                for channel in 0..3 {
                    next[offset + channel] = (sums[channel] / sums[3]).round() as u8;
                }
            }
            next[offset + 3] = (sums[3] / area).round() as u8;
        }
    }
    Some(next)
}

/// Generate area-filtered mip levels, returned as straight RGBA for image upload.
/// RGB is weighted by alpha before averaging. Fractional footprints include the
/// last row/column of odd-sized levels and work on single-pixel axes.
pub fn straight_rgba_mipmaps(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    assert!(width > 0 && height > 0);
    assert_eq!(rgba.len(), width as usize * height as usize * 4);
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let (mut source_width, mut source_height) = (width, height);
    while source_width > 1 || source_height > 1 {
        let source = levels
            .last()
            .map_or(rgba, |(_, _, pixels)| pixels.as_slice());
        let next_width = (source_width / 2).max(1);
        let next_height = (source_height / 2).max(1);
        let next = area_average_rgba8(source_width, source_height, source, next_width, next_height);
        levels.push((next_width, next_height, next));
        (source_width, source_height) = (next_width, next_height);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_size_preserves_transparent_rgb_exactly() {
        let pixels = [123, 231, 9, 0, 5, 10, 15, 1];
        assert_eq!(area_average_rgba8(2, 1, &pixels, 2, 1), pixels);
    }

    #[test]
    fn direct_filter_uses_source_area_and_alpha_weights() {
        let pixels = [255, 0, 255, 0, 255, 0, 0, 255, 0, 255, 0, 85];
        assert_eq!(area_average_rgba8(3, 1, &pixels, 1, 1), [191, 64, 0, 113]);
        assert_eq!(area_average_rgba8(1, 3, &pixels, 1, 1), [191, 64, 0, 113]);
        assert_eq!(
            area_average_rgba8(2, 1, &[99, 88, 77, 0, 11, 22, 33, 0], 1, 1),
            [0; 4]
        );
    }

    #[test]
    fn fractional_odd_footprints_have_fixed_rounding() {
        let pixels = [0, 0, 0, 255, 100, 100, 100, 255, 200, 200, 200, 255];
        // 1.5-pixel-wide areas: (0 + 0.5*100)/1.5 and (0.5*100 + 200)/1.5.
        assert_eq!(
            area_average_rgba8(3, 1, &pixels, 2, 1),
            [33, 33, 33, 255, 167, 167, 167, 255]
        );
        assert_eq!(
            area_average_rgba8(1, 3, &pixels, 1, 2),
            [33, 33, 33, 255, 167, 167, 167, 255]
        );
        assert_eq!(
            area_average_rgba8(2, 1, &[0, 0, 0, 255, 1, 1, 1, 255], 1, 1),
            [1, 1, 1, 255]
        );
    }

    #[test]
    fn direct_thumbnail_is_not_resized_from_an_intermediate_mip() {
        let mut pixels = vec![0; 5 * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        pixels[0] = 2;
        pixels[4] = 255;
        let direct = area_average_rgba8(5, 1, &pixels, 1, 1);
        assert_eq!(direct, [51, 0, 0, 255]);
        let mips = straight_rgba_mipmaps(5, 1, &pixels);
        assert_eq!(mips.last().unwrap().2, [52, 0, 0, 255]);
    }

    #[test]
    fn alpha_rounding_preserves_existing_straight_color_convention() {
        // A positive weighted alpha can round to zero. Color is still the
        // straight weighted RGB; a subsequent filter ignores its zero alpha.
        let pixels = [1, 2, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(area_average_rgba8(4, 1, &pixels, 1, 1), [1, 2, 3, 0]);
    }

    #[test]
    fn filtering_cancels_before_allocation_and_during_source_scanlines() {
        assert_eq!(
            area_average_rgba8_with_control(1, 1, &[1; 4], 1, 1, || false),
            None
        );
        let mut checks = 0;
        let result = area_average_rgba8_with_control(1, 8, &[255; 32], 1, 1, || {
            checks += 1;
            checks < 5
        });
        assert_eq!(result, None);
        assert_eq!(checks, 5);
    }

    #[test]
    fn invisible_rgb_does_not_contribute_to_uploaded_color() {
        let rgba = [255, 0, 255, 0, 255, 100, 20, 128];
        assert_eq!(premultiply_rgba8(&rgba), [0, 0, 0, 0, 128, 50, 10, 128]);
        assert_eq!(rgba[0], 255);
    }

    #[test]
    fn mipmaps_exclude_invisible_color_and_weight_partial_alpha() {
        let levels = straight_rgba_mipmaps(3, 1, &[255, 0, 255, 0, 0, 255, 0, 255, 0, 0, 255, 255]);
        assert_eq!(levels[0], (1, 1, vec![0, 128, 128, 170]));
        let levels = straight_rgba_mipmaps(2, 1, &[255, 0, 0, 255, 0, 0, 255, 85]);
        assert_eq!(levels[0], (1, 1, vec![191, 0, 64, 170]));
        assert_eq!(
            straight_rgba_mipmaps(2, 1, &[255, 0, 255, 0, 0, 255, 0, 0])[0].2,
            [0; 4]
        );
    }

    #[test]
    fn mipmaps_include_odd_edges_and_single_pixel_axes() {
        for (width, height) in [(3, 1), (1, 3), (3, 3), (5, 1)] {
            let mut rgba = vec![0; width * height * 4];
            rgba[(width * height - 1) * 4..].fill(255);
            let levels = straight_rgba_mipmaps(width as u32, height as u32, &rgba);
            let last = levels.last().unwrap();
            assert_eq!((last.0, last.1), (1, 1));
            assert_eq!(
                last.2,
                [
                    255,
                    255,
                    255,
                    (255.0 / (width * height) as f64).round() as u8
                ]
            );
        }
    }
}
