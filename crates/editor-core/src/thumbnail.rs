//! Static texture-region previews masked by UV triangles, without atlas-sized intermediates.
use crate::SceneTexture;
use kasane_core::{Mesh, Vec2};
use std::collections::HashMap;
use std::sync::Arc;

pub const MESH_THUMBNAIL_SIZE: u32 = 40;

#[derive(Clone, Debug)]
pub struct MeshTextureRegion {
    texture: SceneTexture,
    // Top-down pixel bounds, with an exclusive right/bottom edge.
    bounds: [u32; 4],
    triangles: Arc<[[Vec2; 3]]>,
}

impl PartialEq for MeshTextureRegion {
    fn eq(&self, other: &Self) -> bool {
        self.bounds == other.bounds
            && self.triangles == other.triangles
            && self.texture.width == other.texture.width
            && self.texture.height == other.texture.height
            && Arc::ptr_eq(&self.texture.rgba, &other.texture.rgba)
    }
}

impl MeshTextureRegion {
    pub(crate) fn new(texture: &SceneTexture, mesh: &Mesh) -> Option<Self> {
        let expected = (texture.width as usize)
            .checked_mul(texture.height as usize)?
            .checked_mul(4)?;
        if texture.width == 0 || texture.height == 0 || texture.rgba.len() != expected {
            return None;
        }
        if mesh.vertex_ids.len() != mesh.uvs.len() || mesh.triangles.is_empty() {
            return None;
        }
        // Triangles contain stable vertex IDs, not offsets into the UV array.
        let vertices: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .copied()
            .zip(mesh.uvs.iter().copied())
            .collect();
        let mut triangles = Vec::with_capacity(mesh.triangles.len());
        for ids in &mesh.triangles {
            let [Some(a), Some(b), Some(c)] = ids.map(|id| vertices.get(&id).copied()) else {
                return None;
            };
            triangles.push([a, b, c]);
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        // Ignore unused vertices; only rendered triangles define the artwork's region.
        for uv in triangles.iter().flatten() {
            if !uv.x.is_finite() || !uv.y.is_finite() {
                return None;
            }
            for (axis, value) in [uv.x, uv.y].into_iter().enumerate() {
                min[axis] = min[axis].min(value.clamp(0., 1.));
                max[axis] = max[axis].max(value.clamp(0., 1.));
            }
        }
        if min[0] >= max[0] || min[1] >= max[1] {
            return None;
        }
        // Mesh UVs are bottom-up; decoded texture pixels are top-down.
        let bounds = [
            (min[0] as f64 * texture.width as f64).floor() as u32,
            ((1. - max[1] as f64) * texture.height as f64).floor() as u32,
            (max[0] as f64 * texture.width as f64).ceil() as u32,
            ((1. - min[1] as f64) * texture.height as f64).ceil() as u32,
        ];
        if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
            return None;
        }
        Some(Self {
            texture: texture.clone(),
            bounds,
            triangles: triangles.into(),
        })
    }

    /// Union triangles into a tiny supersampled mask. Scanline spans avoid testing
    /// every output sample against every triangle; overlaps never accumulate alpha.
    fn coverage(&self, width: usize, height: usize) -> Vec<bool> {
        let [left, top, right, bottom] = self.bounds;
        let scale_x = width as f64 / (right - left) as f64;
        let scale_y = height as f64 / (bottom - top) as f64;
        let mut mask = vec![false; width * height];
        for triangle in self.triangles.iter() {
            let points = triangle.map(|uv| {
                [
                    (uv.x as f64 * self.texture.width as f64 - left as f64) * scale_x,
                    ((1. - uv.y as f64) * self.texture.height as f64 - top as f64) * scale_y,
                ]
            });
            let [a, b, c] = points;
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area == 0. {
                continue;
            }
            let y_start = a[1].min(b[1]).min(c[1]).floor().clamp(0., height as f64) as usize;
            let y_end = a[1].max(b[1]).max(c[1]).ceil().clamp(0., height as f64) as usize;
            for row in y_start..y_end {
                let y = row as f64 + 0.5;
                let mut left = f64::INFINITY;
                let mut right = f64::NEG_INFINITY;
                for [a, b] in [[a, b], [b, c], [c, a]] {
                    if (a[1] <= y && y < b[1]) || (b[1] <= y && y < a[1]) {
                        let x = a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                        left = left.min(x);
                        right = right.max(x);
                    }
                }
                if left > right {
                    continue;
                }
                let start = (left - 0.5).ceil().clamp(0., width as f64) as usize;
                let end = ((right - 0.5).floor() + 1.).clamp(0., width as f64) as usize;
                if start < end {
                    mask[row * width + start..row * width + end].fill(true);
                }
            }
        }
        mask
    }

    /// Produce straight RGBA, aspect-fitted into a transparent 40px square.
    /// At most 16 source texels per output pixel, including for very large atlases.
    /// Alpha-weighted filtering avoids dark/color fringes around transparent artwork.
    pub fn rgba(&self) -> Vec<u8> {
        let [left, top, right, bottom] = self.bounds;
        let width = right - left;
        let height = bottom - top;
        let scale = MESH_THUMBNAIL_SIZE as f32 / width.max(height) as f32;
        let out_w = (width as f32 * scale).round().max(1.) as u32;
        let out_h = (height as f32 * scale).round().max(1.) as u32;
        let offset_x = (MESH_THUMBNAIL_SIZE - out_w) / 2;
        let offset_y = (MESH_THUMBNAIL_SIZE - out_h) / 2;
        let step_x = width as f32 / out_w as f32;
        let step_y = height as f32 / out_h as f32;
        // Four coverage samples also antialias the UV outline when magnifying.
        let samples_x = 2;
        let samples_y = 2;
        let mask_width = out_w * samples_x;
        let mask = self.coverage(mask_width as usize, (out_h * samples_y) as usize);
        let weight = 1. / (samples_x * samples_y) as f32;
        let mut output = vec![0; (MESH_THUMBNAIL_SIZE * MESH_THUMBNAIL_SIZE * 4) as usize];
        for y in 0..out_h {
            for x in 0..out_w {
                let mut sum = [0.; 4];
                for sy in 0..samples_y {
                    for sx in 0..samples_x {
                        if !mask[((y * samples_y + sy) * mask_width + x * samples_x + sx) as usize]
                        {
                            continue;
                        }
                        let px = left as f32
                            + (x as f32 + (sx as f32 + 0.5) / samples_x as f32) * step_x
                            - 0.5;
                        let py = top as f32
                            + (y as f32 + (sy as f32 + 0.5) / samples_y as f32) * step_y
                            - 0.5;
                        let ix = px.floor();
                        let iy = py.floor();
                        let fx = px - ix;
                        let fy = py - iy;
                        for (dy, wy) in [(0, 1. - fy), (1, fy)] {
                            for (dx, wx) in [(0, 1. - fx), (1, fx)] {
                                // Keep the bilinear footprint inside the texture-region bounds.
                                let tx =
                                    (ix as i64 + dx).clamp(left as i64, right as i64 - 1) as usize;
                                let ty =
                                    (iy as i64 + dy).clamp(top as i64, bottom as i64 - 1) as usize;
                                let i = (ty * self.texture.width as usize + tx) * 4;
                                let pixel = &self.texture.rgba[i..i + 4];
                                let a = pixel[3] as f32 * wx * wy * weight;
                                for channel in 0..3 {
                                    sum[channel] += pixel[channel] as f32 * a;
                                }
                                sum[3] += a;
                            }
                        }
                    }
                }
                let i = (((y + offset_y) * MESH_THUMBNAIL_SIZE + x + offset_x) * 4) as usize;
                if sum[3] > 0. {
                    for channel in 0..3 {
                        output[i + channel] = (sum[channel] / sum[3]).round() as u8;
                    }
                    output[i + 3] = sum[3].round() as u8;
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture(width: u32, height: u32, pixels: Vec<u8>) -> SceneTexture {
        SceneTexture {
            asset_id: "atlas".into(),
            cache_key: "test".into(),
            width,
            height,
            rgba: pixels.into(),
        }
    }

    fn rectangle(texture: &SceneTexture, uvs: &[Vec2]) -> Option<MeshTextureRegion> {
        let a = *uvs.first()?;
        let b = *uvs.get(1)?;
        MeshTextureRegion::new(
            texture,
            &Mesh {
                vertex_ids: vec![10, 20, 30, 40],
                uvs: vec![a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)],
                triangles: vec![[10, 20, 30], [10, 30, 40]],
                ..Default::default()
            },
        )
    }

    #[test]
    fn crops_bottom_up_uvs_without_neighbor_bleed_and_preserves_aspect() {
        let mut pixels = vec![0; 4 * 4 * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 0, 255, 255]);
        }
        // Top-right 2x1 region is red; everything else is blue.
        pixels[8..16].copy_from_slice(&[255, 0, 0, 255, 255, 0, 0, 255]);
        let source = texture(4, 4, pixels);
        let region = rectangle(&source, &[Vec2::new(0.5, 0.75), Vec2::new(1., 1.)]).unwrap();
        let image = region.rgba();
        for y in 0..40 {
            for x in 0..40 {
                let i = (y * 40 + x) * 4;
                assert_eq!(
                    &image[i..i + 4],
                    if (10..30).contains(&y) {
                        &[255, 0, 0, 255]
                    } else {
                        &[0, 0, 0, 0]
                    }
                );
            }
        }
    }

    #[test]
    fn alpha_filter_does_not_mix_hidden_rgb_into_edges() {
        let source = texture(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 0]);
        let image = rectangle(&source, &[Vec2::new(0., 0.), Vec2::new(1., 1.)])
            .unwrap()
            .rgba();
        assert!(image.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
        for p in image.chunks_exact(4).filter(|p| p[3] > 0) {
            assert_eq!(&p[..3], &[255, 0, 0]);
        }
    }

    #[test]
    fn minification_averages_alpha_without_color_bleed() {
        let mut pixels = Vec::new();
        for _ in 0..80 * 40 / 2 {
            pixels.extend_from_slice(&[255, 0, 0, 255, 0, 255, 0, 0]);
        }
        let source = texture(80, 40, pixels);
        let image = rectangle(&source, &[Vec2::new(0., 0.), Vec2::new(1., 1.)])
            .unwrap()
            .rgba();
        for y in 10..30 {
            for x in 0..40 {
                let i = (y * 40 + x) * 4;
                assert_eq!(&image[i..i + 4], &[255, 0, 0, 128]);
            }
        }
    }

    #[test]
    fn uv_triangles_exclude_other_atlas_parts_inside_the_bounding_box() {
        let mut pixels = [255, 0, 0, 255].repeat(40 * 40);
        // An unrelated blue atlas part lies in the diamond's rectangular bounds.
        for y in 0..6 {
            for x in 0..6 {
                let i = (y * 40 + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let source = texture(40, 40, pixels);
        let mesh = Mesh {
            vertex_ids: vec![11, 22, 33, 44, 55],
            uvs: vec![
                Vec2::new(0.5, 0.),
                Vec2::new(1., 0.5),
                Vec2::new(0.5, 1.),
                Vec2::new(0., 0.5),
                Vec2::new(20., 20.),
            ],
            triangles: vec![[11, 22, 33], [11, 33, 44]],
            ..Default::default()
        };
        let region = MeshTextureRegion::new(&source, &mesh).unwrap();
        let output = region.rgba();
        assert_eq!(&output[..4], &[0, 0, 0, 0]);
        let center = (20 * 40 + 20) * 4;
        assert_eq!(&output[center..center + 4], &[255, 0, 0, 255]);
        assert!(
            output.chunks_exact(4).all(|p| p[2] == 0),
            "neighboring blue artwork must not leak into the thumbnail"
        );
        assert!(
            output.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255),
            "UV edges should be antialiased"
        );
        // Both winding orders and overlapping triangles give the same union.
        let mut reversed = mesh.clone();
        reversed.triangles = vec![[33, 22, 11], [44, 33, 11], [11, 22, 33]];
        assert_eq!(
            MeshTextureRegion::new(&source, &reversed).unwrap().rgba(),
            output
        );
    }

    #[test]
    fn disconnected_uv_islands_leave_the_gap_transparent() {
        let source = texture(40, 40, [255, 0, 0, 255].repeat(40 * 40));
        let mesh = Mesh {
            vertex_ids: (0..8).collect(),
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(0.25, 0.),
                Vec2::new(0.25, 1.),
                Vec2::new(0., 1.),
                Vec2::new(0.75, 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0.75, 1.),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
            ..Default::default()
        };
        let output = MeshTextureRegion::new(&source, &mesh).unwrap().rgba();
        for y in 0..40 {
            for x in 0..40 {
                let i = (y * 40 + x) * 4;
                assert_eq!(
                    &output[i..i + 4],
                    if (10..30).contains(&x) {
                        &[0, 0, 0, 0]
                    } else {
                        &[255, 0, 0, 255]
                    }
                );
            }
        }
    }

    #[test]
    fn topology_changes_invalidate_regions_even_with_identical_bounds() {
        let source = texture(1, 1, vec![255; 4]);
        let mut mesh = Mesh {
            vertex_ids: vec![10, 20, 30, 40],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[10, 20, 30], [10, 30, 40]],
            ..Default::default()
        };
        let full = MeshTextureRegion::new(&source, &mesh).unwrap();
        mesh.triangles.pop();
        let half = MeshTextureRegion::new(&source, &mesh).unwrap();
        assert_eq!(full.bounds, half.bounds);
        assert_ne!(full, half);
        mesh.triangles = vec![[10, 20, 999]];
        assert!(MeshTextureRegion::new(&source, &mesh).is_none());
        mesh.triangles.clear();
        assert!(MeshTextureRegion::new(&source, &mesh).is_none());
    }

    #[test]
    fn invalid_regions_fall_back_instead_of_panicking() {
        let source = texture(1, 1, vec![255; 4]);
        for uvs in [
            vec![],
            vec![Vec2::new(0., 0.)],
            vec![Vec2::new(0., 0.), Vec2::new(1., 1e-30)],
            vec![Vec2::new(f32::NAN, 0.), Vec2::new(1., 1.)],
            vec![Vec2::new(2., 2.), Vec2::new(3., 3.)],
        ] {
            assert!(rectangle(&source, &uvs).is_none());
        }
        assert!(
            rectangle(
                &texture(2, 2, vec![]),
                &[Vec2::new(0., 0.), Vec2::new(1., 1.)]
            )
            .is_none()
        );
    }
}
