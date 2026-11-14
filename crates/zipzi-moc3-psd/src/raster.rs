use zipzi_core::evaluation::Drawable;
use zipzi_core::image::DecodedImage;
use zipzi_core::types::{BlendMode, Canvas};

/// Cropped RGBA pixels for one ArtMesh, positioned on the full canvas.
#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    pub part_id: String,
    pub hidden: bool,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub blend_mode: BlendMode,
    /// Grayscale user mask, cropped to the same bounds as this layer.
    pub mask: Option<Vec<u8>>,
}

pub fn rasterize(
    d: &Drawable,
    texture: &DecodedImage,
    canvas: Canvas,
    width: u32,
    height: u32,
) -> Option<Layer> {
    rasterize_impl(d, texture, canvas, width, height, false)
}

/// Raw clipping sources ignore appearance, visibility and their own masks.
pub(crate) fn rasterize_mask(
    d: &Drawable,
    texture: &DecodedImage,
    canvas: Canvas,
    width: u32,
    height: u32,
) -> Option<Layer> {
    rasterize_impl(d, texture, canvas, width, height, true)
}

fn rasterize_impl(
    d: &Drawable,
    texture: &DecodedImage,
    canvas: Canvas,
    width: u32,
    height: u32,
    raw_mask: bool,
) -> Option<Layer> {
    if d.positions.len() != d.uvs.len() || d.indices.len() < 3 {
        return None;
    }
    let points: Vec<(f32, f32)> = d
        .positions
        .iter()
        .map(|p| (p.x + canvas.origin.x, canvas.origin.y - p.y))
        .collect();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for &(x, y) in &points {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let left = min_x.floor().max(0.0).min(width as f32) as u32;
    let top = min_y.floor().max(0.0).min(height as f32) as u32;
    let right = max_x.ceil().max(0.0).min(width as f32) as u32;
    let bottom = max_y.ceil().max(0.0).min(height as f32) as u32;
    if left >= right || top >= bottom {
        return None;
    }
    let layer_width = right - left;
    let layer_height = bottom - top;
    let mut rgba = vec![0; layer_width as usize * layer_height as usize * 4];
    // An ArtMesh with zero default opacity still needs usable pixels when its
    // hidden PSD layer is toggled on. PSD visibility carries the zero state.
    let opacity = if !d.visible && d.opacity == 0.0 {
        1.0
    } else {
        d.opacity.clamp(0.0, 1.0)
    };
    for triangle in d.indices.as_chunks::<3>().0 {
        let [a, b, c] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if a >= points.len() || b >= points.len() || c >= points.len() {
            continue;
        }
        let (ax, ay) = points[a];
        let (bx, by) = points[b];
        let (cx, cy) = points[c];
        let area = edge(ax, ay, bx, by, cx, cy);
        if area.abs() < 1e-8 {
            continue;
        }
        let x0 = ax.min(bx).min(cx).floor().max(left as f32) as u32;
        let y0 = ay.min(by).min(cy).floor().max(top as f32) as u32;
        let x1 = ax.max(bx).max(cx).ceil().min(right as f32) as u32;
        let y1 = ay.max(by).max(cy).ceil().min(bottom as f32) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let wa = edge(bx, by, cx, cy, px, py) / area;
                let wb = edge(cx, cy, ax, ay, px, py) / area;
                let wc = 1.0 - wa - wb;
                if wa < -1e-5 || wb < -1e-5 || wc < -1e-5 {
                    continue;
                }
                // A shared triangle edge belongs to one face. Otherwise a
                // translucent mask is accumulated twice along a mesh diagonal.
                if raw_mask
                    && ![
                        (wa, (bx, by), (cx, cy)),
                        (wb, (cx, cy), (ax, ay)),
                        (wc, (ax, ay), (bx, by)),
                    ]
                    .into_iter()
                    .all(|(weight, mut a, mut b)| {
                        if area < 0.0 {
                            std::mem::swap(&mut a, &mut b);
                        }
                        weight > 0.0 || (weight == 0.0 && (b.1 > a.1 || (b.1 == a.1 && b.0 < a.0)))
                    })
                {
                    continue;
                }
                let u = wa * d.uvs[a].x + wb * d.uvs[b].x + wc * d.uvs[c].x;
                let v = wa * d.uvs[a].y + wb * d.uvs[b].y + wc * d.uvs[c].y;
                let sample = sample(texture, u, v);
                let offset = ((y - top) as usize * layer_width as usize + (x - left) as usize) * 4;
                if raw_mask {
                    let previous = f32::from(rgba[offset + 3]) / 255.0;
                    rgba[offset + 3] =
                        ((sample[3] + previous * (1.0 - sample[3])) * 255.0).round() as u8;
                    continue;
                }
                for channel in 0..3 {
                    let value = if sample[3] > 0.0 {
                        sample[channel] / sample[3]
                    } else {
                        0.0
                    };
                    let multiplied = value * d.multiply_color[channel];
                    let colored =
                        (multiplied + d.screen_color[channel] * (1.0 - multiplied)).clamp(0.0, 1.0);
                    rgba[offset + channel] = (colored * 255.0).round() as u8;
                }
                rgba[offset + 3] = (sample[3] * opacity * 255.0).round() as u8;
            }
        }
    }
    Some(Layer {
        name: if d.runtime_id.is_empty() {
            d.id.clone()
        } else {
            d.runtime_id.clone()
        },
        part_id: d.part_id.clone(),
        hidden: !d.visible,
        left,
        top,
        width: layer_width,
        height: layer_height,
        rgba,
        blend_mode: d.blend_mode,
        mask: None,
    })
}

fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

/// Match normalized GPU linear sampling: texel centers are (i + 0.5) / size.
/// Interpolate premultiplied colors so invisible RGB cannot bleed into edges.
fn sample(texture: &DecodedImage, u: f32, v: f32) -> [f32; 4] {
    if !u.is_finite() || !v.is_finite() || texture.width == 0 || texture.height == 0 {
        return [0.0; 4];
    }
    let x = (u.clamp(0.0, 1.0) * texture.width as f32 - 0.5).clamp(0.0, (texture.width - 1) as f32);
    // Zipzi UVs have a bottom-left origin; PNG rows have a top-left origin.
    let y = ((1.0 - v).clamp(0.0, 1.0) * texture.height as f32 - 0.5)
        .clamp(0.0, (texture.height - 1) as f32);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = (
        (x0 + 1).min(texture.width - 1),
        (y0 + 1).min(texture.height - 1),
    );
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut result = [0.0; 4];
    for (x, y, weight) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        let offset = ((y as usize * texture.width as usize) + x as usize) * 4;
        let alpha = f32::from(texture.rgba[offset + 3]) / 255.0;
        for channel in 0..3 {
            result[channel] += f32::from(texture.rgba[offset + channel]) / 255.0 * alpha * weight;
        }
        result[3] += alpha * weight;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{rasterize, sample};
    use crate::psd;
    use ag_psd::psd::ReadOptions;
    use ag_psd::read_psd;
    use std::sync::Arc;
    use zipzi_core::evaluation::Drawable;
    use zipzi_core::image::DecodedImage;
    use zipzi_core::types::{Canvas, Vec2};

    #[test]
    fn samples_moc3_uvs_with_the_same_vertical_origin_as_the_renderer() {
        let texture = DecodedImage {
            width: 1,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        };
        assert_eq!(sample(&texture, 0.0, 1.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(sample(&texture, 0.0, 0.0), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn linear_sampling_matches_gpu_texel_centers_and_excludes_invisible_rgb() {
        let texture = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        };
        assert_eq!(sample(&texture, 0.25, 0.5), [1., 0., 0., 1.]);
        assert_eq!(sample(&texture, 0.75, 0.5), [0., 0., 1., 1.]);
        assert_eq!(sample(&texture, 0.5, 0.5), [0.5, 0., 0.5, 1.]);
        assert_eq!(sample(&texture, 0.375, 0.5), [0.75, 0., 0.25, 1.]);
        let transparent_blue = DecodedImage {
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 0],
            ..texture
        };
        assert_eq!(sample(&transparent_blue, 0.5, 0.5), [0.5, 0., 0., 0.5]);
    }

    #[test]
    fn enlarged_color_and_pixel_masks_have_smooth_alpha_without_color_fringe() {
        let texture = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 0],
        };
        let drawable = Drawable {
            positions: vec![
                Vec2::new(0., 0.),
                Vec2::new(4., 0.),
                Vec2::new(4., -1.),
                Vec2::new(0., -1.),
            ],
            uvs: vec![
                Vec2::new(0., 1.),
                Vec2::new(1., 1.),
                Vec2::new(1., 0.),
                Vec2::new(0., 0.),
            ]
            .into(),
            indices: vec![0, 1, 2, 0, 2, 3].into(),
            ..Default::default()
        };
        let canvas = Canvas::new(4., 1., Vec2::new(0., 0.), 1.);
        let layer = rasterize(&drawable, &texture, canvas, 4, 1).unwrap();
        assert_eq!(
            layer.rgba,
            vec![255, 0, 0, 255, 255, 0, 0, 191, 255, 0, 0, 64, 0, 0, 0, 0]
        );
        let mask = super::rasterize_mask(&drawable, &texture, canvas, 4, 1).unwrap();
        assert_eq!(
            mask.rgba.chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>(),
            vec![255, 191, 64, 0]
        );
    }

    #[test]
    fn multiply_and_screen_colors_follow_the_renderer_formula() {
        let texture = DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![128; 4],
        };
        let drawable = Drawable {
            positions: vec![Vec2::new(0., 0.), Vec2::new(2., 0.), Vec2::new(0., -2.)],
            uvs: vec![Vec2::new(0.5, 0.5); 3].into(),
            indices: vec![0, 1, 2].into(),
            multiply_color: [0.5; 4],
            screen_color: [0.25; 4],
            ..Default::default()
        };
        let layer = rasterize(
            &drawable,
            &texture,
            Canvas::new(2., 2., Vec2::new(0., 0.), 1.),
            2,
            2,
        )
        .unwrap();
        assert_eq!(&layer.rgba[..4], &[112, 112, 112, 128]);
    }

    #[test]
    fn zero_opacity_artmesh_exports_as_hidden_layer_with_pixels() {
        let texture = DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };
        let drawable = Drawable {
            id: "hidden-mesh".into(),
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(0.0, -2.0),
            ],
            uvs: Arc::from([Vec2::new(0.0, 0.0); 3]),
            indices: Arc::from([0, 1, 2]),
            opacity: 0.0,
            visible: false,
            ..Default::default()
        };
        let layer = rasterize(
            &drawable,
            &texture,
            Canvas::new(2.0, 2.0, Vec2::new(0.0, 0.0), 1.0),
            2,
            2,
        )
        .unwrap();
        assert!(layer.hidden);
        assert!(layer
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0));

        let bytes = psd::encode(2, 2, vec![layer]).unwrap();
        let parsed = read_psd(
            &bytes,
            &ReadOptions {
                use_image_data: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(parsed.children.as_ref().unwrap()[0].hidden, Some(true));
        assert!(parsed.children.as_ref().unwrap()[0]
            .image_data
            .as_ref()
            .unwrap()
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0));
        assert!(parsed
            .image_data
            .as_ref()
            .unwrap()
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 0));
    }
}
