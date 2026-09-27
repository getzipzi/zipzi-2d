/// Match the straight-RGBA area-filtered mip chain used by kasane-sdk-observe.
/// Fractional source footprints include the last row and column of odd levels.
pub(crate) fn straight_rgba_mipmaps(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let (mut source_width, mut source_height) = (width, height);
    while source_width > 1 || source_height > 1 {
        let source = levels
            .last()
            .map_or(rgba, |(_, _, pixels)| pixels.as_slice());
        let next_width = (source_width / 2).max(1);
        let next_height = (source_height / 2).max(1);
        let mut next = vec![0u8; next_width as usize * next_height as usize * 4];
        for y in 0..next_height {
            for x in 0..next_width {
                let x0 = x as f64 * source_width as f64 / next_width as f64;
                let x1 = (x + 1) as f64 * source_width as f64 / next_width as f64;
                let y0 = y as f64 * source_height as f64 / next_height as f64;
                let y1 = (y + 1) as f64 * source_height as f64 / next_height as f64;
                let mut sums = [0.0; 4];
                for sy in y0.floor() as u32..y1.ceil() as u32 {
                    for sx in x0.floor() as u32..x1.ceil() as u32 {
                        let weight = (x1.min((sx + 1) as f64) - x0.max(sx as f64))
                            * (y1.min((sy + 1) as f64) - y0.max(sy as f64));
                        let offset = (sy as usize * source_width as usize + sx as usize) * 4;
                        for channel in 0..4 {
                            sums[channel] += f64::from(source[offset + channel]) * weight;
                        }
                    }
                }
                let area = (x1 - x0) * (y1 - y0);
                let offset = (y as usize * next_width as usize + x as usize) * 4;
                for channel in 0..4 {
                    next[offset + channel] = (sums[channel] / area).round() as u8;
                }
            }
        }
        levels.push((next_width, next_height, next));
        (source_width, source_height) = (next_width, next_height);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::straight_rgba_mipmaps;

    #[test]
    fn includes_odd_edges_and_preserves_straight_rgba() {
        let rgba = [255, 0, 0, 0, 0, 255, 0, 255, 0, 0, 255, 255];
        let levels = straight_rgba_mipmaps(3, 1, &rgba);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0], (1, 1, vec![85, 85, 85, 170]));
    }

    #[test]
    fn reaches_one_pixel_for_non_square_textures() {
        let levels = straight_rgba_mipmaps(4, 2, &[64; 32]);
        assert_eq!(levels[0], (2, 1, vec![64; 8]));
        assert_eq!(levels[1], (1, 1, vec![64; 4]));
    }
}
