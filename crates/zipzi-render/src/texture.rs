//! Source-image preparation. Authored pixels remain straight RGBA; GPU filtering
//! must interpolate premultiplied colors to exclude invisible texels' RGB.

/// Convert straight RGBA8 to premultiplied RGBA8 without changing the source.
pub fn premultiply_rgba8(rgba: &[u8]) -> Vec<u8> {
    let mut result = rgba.to_vec();
    premultiply_rgba8_in_place(&mut result);
    result
}

/// Premultiply an owned upload buffer without allocating another image.
pub fn premultiply_rgba8_in_place(rgba: &mut [u8]) {
    assert_eq!(rgba.len() % 4, 0);
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha == 255 {
            continue;
        }
        if alpha == 0 {
            pixel.fill(0);
            continue;
        }
        for color in &mut pixel[..3] {
            *color = ((u16::from(*color) * alpha + 127) / 255) as u8;
        }
    }
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
        let next = half_rgba8(source_width, source_height, source, next_width, next_height);
        levels.push((next_width, next_height, next));
        (source_width, source_height) = (next_width, next_height);
    }
    levels
}

// Mip levels only halve each axis. Reuse their small footprints across rows
// rather than recomputing fractional bounds for every source pixel/channel.
// Keep the general area filter above as the independent thumbnail/reference path.
fn half_rgba8(width: u32, height: u32, rgba: &[u8], next_width: u32, next_height: u32) -> Vec<u8> {
    let mut next = vec![0; next_width as usize * next_height as usize * 4];
    if width.is_multiple_of(2) && height.is_multiple_of(2) {
        for (out, rows) in next
            .chunks_exact_mut(next_width as usize * 4)
            .zip(rgba.chunks_exact(width as usize * 8))
        {
            let (top, bottom) = rows.split_at(width as usize * 4);
            for (pixel, (a, b)) in out
                .chunks_exact_mut(4)
                .zip(top.chunks_exact(8).zip(bottom.chunks_exact(8)))
            {
                let alpha = [
                    u32::from(a[3]),
                    u32::from(a[7]),
                    u32::from(b[3]),
                    u32::from(b[7]),
                ];
                let sum = alpha.iter().sum::<u32>();
                if sum == 1020 {
                    for c in 0..3 {
                        pixel[c] = ((u32::from(a[c])
                            + u32::from(a[c + 4])
                            + u32::from(b[c])
                            + u32::from(b[c + 4])
                            + 2)
                            / 4) as u8;
                    }
                } else if sum != 0 {
                    for c in 0..3 {
                        let color = u32::from(a[c]) * alpha[0]
                            + u32::from(a[c + 4]) * alpha[1]
                            + u32::from(b[c]) * alpha[2]
                            + u32::from(b[c + 4]) * alpha[3];
                        pixel[c] = ((color + sum / 2) / sum) as u8;
                    }
                }
                pixel[3] = ((sum + 2) / 4) as u8;
            }
        }
        return next;
    }
    struct Span {
        first: usize,
        count: usize,
        weights: [f64; 4],
        extent: f64,
    }
    let spans = |source: u32, target: u32| -> Vec<Span> {
        (0..target)
            .map(|i| {
                let low = i as f64 * source as f64 / target as f64;
                let high = (i + 1) as f64 * source as f64 / target as f64;
                let first = low.floor() as usize;
                let count = high.ceil() as usize - first;
                let mut weights = [0.; 4];
                for (offset, weight) in weights[..count].iter_mut().enumerate() {
                    let pixel = (first + offset) as f64;
                    *weight = high.min(pixel + 1.) - low.max(pixel);
                }
                Span {
                    first,
                    count,
                    weights,
                    extent: high - low,
                }
            })
            .collect()
    };
    let xs = spans(width, next_width);
    let ys = spans(height, next_height);
    for (out, y) in next.chunks_exact_mut(next_width as usize * 4).zip(&ys) {
        for (pixel, x) in out.chunks_exact_mut(4).zip(&xs) {
            let mut sums = [0.; 4];
            for (dy, wy) in y.weights[..y.count].iter().enumerate() {
                let row = ((y.first + dy) * width as usize + x.first) * 4;
                for (dx, wx) in x.weights[..x.count].iter().enumerate() {
                    let p = &rgba[row + dx * 4..row + dx * 4 + 4];
                    let alpha = u32::from(p[3]);
                    if alpha == 0 {
                        continue;
                    }
                    let weight = wx * wy;
                    for c in 0..3 {
                        // Integer color*alpha is exact in f64. Preserve the
                        // reference's multiply/add order, including tie rounding.
                        sums[c] += f64::from(u32::from(p[c]) * alpha) * weight;
                    }
                    sums[3] += f64::from(alpha) * weight;
                }
            }
            if sums[3] > 0. {
                for c in 0..3 {
                    pixel[c] = (sums[c] / sums[3]).round() as u8;
                }
            }
            pixel[3] = (sums[3] / (x.extent * y.extent)).round() as u8;
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mipmap_fast_paths_match_the_general_area_filter_byte_for_byte() {
        let mut state = 0x12345678u32;
        let sizes = (1..=33).flat_map(|w| (1..=33).map(move |h| (w, h))).chain([
            (64, 64),
            (128, 96),
            (257, 129),
        ]);
        for (width, height) in sizes {
            for alpha_kind in 0..4 {
                let mut pixels = vec![0; width as usize * height as usize * 4];
                for pixel in pixels.chunks_exact_mut(4) {
                    for value in pixel.iter_mut() {
                        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                        *value = (state >> 24) as u8;
                    }
                    match alpha_kind {
                        1 => pixel[3] = 255,
                        2 => pixel[3] = 0,
                        3 => pixel[3] %= 4,
                        _ => {}
                    }
                }
                let mut source = pixels.clone();
                let (mut w, mut h) = (width, height);
                for (next_w, next_h, actual) in straight_rgba_mipmaps(width, height, &pixels) {
                    let expected = area_average_rgba8(w, h, &source, next_w, next_h);
                    let difference = actual.iter().zip(&expected).position(|(a, b)| a != b);
                    assert_eq!(
                        difference, None,
                        "{width}x{height}, alpha {alpha_kind}, level {w}x{h}"
                    );
                    source = expected;
                    (w, h) = (next_w, next_h);
                }
            }
        }
    }

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
