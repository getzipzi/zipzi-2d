//! Normalize packed pages using captured immutable resource versions.
use crate::{ImageResource, ResourceStore, ResourceView, TextureData};
use std::collections::{BTreeMap, HashMap, HashSet};
use zipzi_core::{
    state::{AssetContent, AssetRecord, DocumentBuilder, DocumentState},
    Mesh, Status, Vec2,
};
pub(crate) fn unpack_atlas_images(
    state: &DocumentState,
    resources: &ResourceView,
    store: &ResourceStore,
    known_atlases: &HashSet<String>,
) -> Result<(DocumentState, ResourceView), Status> {
    let mut builder = state.builder();
    let mut images = HashMap::new();
    for (source_id, mesh_ids) in mesh_texture_users(state) {
        if mesh_ids.len() == 1 && !known_atlases.contains(&source_id) {
            continue;
        }
        let AssetContent::Image { resource, .. } = state.asset(&source_id).unwrap().content else {
            continue;
        };
        let texture = resources
            .image(resource)
            .ok_or_else(|| Status::error("MISSING_RESOURCE", &source_id))?
            .texture();
        unpack_atlas_page(
            &mut builder,
            &source_id,
            &mesh_ids,
            &texture,
            store,
            &mut images,
        )?;
    }
    let document = builder.freeze()?.state;
    let resources = resources
        .with_images(images.into_values())
        .for_state(&document)?;
    Ok((document, resources))
}

pub(crate) fn mesh_texture_users(state: &DocumentState) -> BTreeMap<String, Vec<String>> {
    let mut users = BTreeMap::<String, Vec<String>>::new();
    for id in state.mesh_order() {
        let mesh = state.mesh(id).unwrap();
        if let Some(asset) = mesh.texture_asset_id.resolved_id() {
            users.entry(asset.into()).or_default().push(id.clone());
        }
    }
    users
}

pub(crate) fn unpack_atlas_page(
    builder: &mut DocumentBuilder,
    source_id: &str,
    mesh_ids: &[String],
    texture: &TextureData,
    store: &ResourceStore,
    images: &mut HashMap<String, ImageResource>,
) -> Result<(), Status> {
    for id in mesh_ids {
        let mut mesh = builder.mesh(id).unwrap().clone();
        let crop = Crop::new(&mesh, texture.width, texture.height)?;
        let size = crop.width as usize * crop.height as usize * 4;
        let reservation = store.budget().reserve(size.saturating_mul(2))?;
        let pixels: std::sync::Arc<[u8]> = crop.extract(&mesh, texture)?.into();
        reservation.commit(&pixels, crate::ImageBufferKind::Rgba)?;
        let image = store.prepare_rgba(crop.width, crop.height, pixels)?;
        // Generated texture keys include dimensions and the pixel digest.
        // Distinct asset records can retain the same immutable image.
        let image = images.entry(image.texture().cache_key).or_insert(image);
        let asset_id = uuid::Uuid::new_v4().to_string();
        builder.create_asset(AssetRecord {
            id: asset_id.clone(),
            name: mesh.name.clone(),
            content: AssetContent::Image {
                resource: image.key(),
                width: image.width(),
                height: image.height(),
            },
        })?;
        crop.rebase(&mut mesh.uvs, texture.width, texture.height);
        mesh.texture_asset_id = asset_id.into();
        builder.replace_mesh(mesh)?;
    }
    // mesh_ids contains all users of this source page.
    builder.remove_asset(source_id)
}
fn invalid(message: &str) -> Status {
    Status::error("ATLAS_EXTRACTION", message)
}

#[derive(Debug)]
struct Crop {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}
impl Crop {
    fn new(mesh: &Mesh, width: u32, height: u32) -> Result<Self, Status> {
        if mesh.uvs.len() != mesh.vertex_ids.len()
            || mesh
                .uvs
                .iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(invalid("Mesh has invalid texture coordinates"));
        }
        if mesh.triangles.is_empty() || mesh.uvs.is_empty() {
            return Ok(Self {
                left: 0,
                top: 0,
                width,
                height,
            });
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for uv in &mesh.uvs {
            let p = [
                uv.x as f64 * width as f64,
                (1. - uv.y as f64) * height as f64,
            ];
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        // Integer crop and two texels of padding retain the linear sampler's
        // footprint without rescaling or baking the image through the mesh.
        let left = (min[0].floor() - 2.).clamp(0., width.saturating_sub(1) as f64) as u32;
        let top = (min[1].floor() - 2.).clamp(0., height.saturating_sub(1) as f64) as u32;
        let right = (max[0].ceil() + 2.).clamp((left + 1) as f64, width as f64) as u32;
        let bottom = (max[1].ceil() + 2.).clamp((top + 1) as f64, height as f64) as u32;
        Ok(Self {
            left,
            top,
            width: right - left,
            height: bottom - top,
        })
    }
    fn rebase(&self, uvs: &mut [Vec2], width: u32, height: u32) {
        for uv in uvs {
            uv.x = ((uv.x as f64 * width as f64 - self.left as f64) / self.width as f64) as f32;
            uv.y = (1.
                - ((1. - uv.y as f64) * height as f64 - self.top as f64) / self.height as f64)
                as f32;
        }
    }
    fn extract(&self, mesh: &Mesh, texture: &TextureData) -> Result<Vec<u8>, Status> {
        let mut pixels = vec![0; self.width as usize * self.height as usize * 4];
        if mesh.triangles.is_empty() {
            pixels.copy_from_slice(&texture.rgba);
            return Ok(pixels);
        }
        let points: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .copied()
            .zip(mesh.uvs.iter().map(|uv| {
                [
                    uv.x as f64 * texture.width as f64,
                    (1. - uv.y as f64) * texture.height as f64,
                ]
            }))
            .collect();
        for triangle in &mesh.triangles {
            let [a, b, c] = triangle.map(|id| points.get(&id).copied());
            let points = [
                a.ok_or_else(|| invalid("Unknown triangle vertex"))?,
                b.ok_or_else(|| invalid("Unknown triangle vertex"))?,
                c.ok_or_else(|| invalid("Unknown triangle vertex"))?,
            ];
            let min_y = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            let max_y = points
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max);
            let start = (min_y - 1.5)
                .ceil()
                .clamp(self.top as f64, (self.top + self.height - 1) as f64)
                as u32;
            let end = ((max_y + 0.5).floor() + 1.)
                .clamp((self.top + 1) as f64, (self.top + self.height) as f64)
                as u32;
            for y in start..end {
                // A linear sample can use a texel when the triangle intersects
                // the texel center's 2x2 support square. Clip to its horizontal
                // strip, then copy the exact intersecting x span. Border texels
                // have unbounded support because the renderer clamps UVs.
                let low = if y == 0 {
                    f64::NEG_INFINITY
                } else {
                    y as f64 - 0.5
                };
                let high = if y + 1 == texture.height {
                    f64::INFINITY
                } else {
                    y as f64 + 1.5
                };
                let mut x_min = f64::INFINITY;
                let mut x_max = f64::NEG_INFINITY;
                for i in 0..3 {
                    let a = points[i];
                    let b = points[(i + 1) % 3];
                    if a[1] >= low && a[1] <= high {
                        x_min = x_min.min(a[0]);
                        x_max = x_max.max(a[0]);
                    }
                    if a[1] != b[1] {
                        for edge in [low, high] {
                            let t = (edge - a[1]) / (b[1] - a[1]);
                            if (0.0..=1.0).contains(&t) {
                                let x = a[0] + t * (b[0] - a[0]);
                                x_min = x_min.min(x);
                                x_max = x_max.max(x);
                            }
                        }
                    }
                }
                if x_min > x_max {
                    continue;
                }
                let left = (x_min - 1.5)
                    .ceil()
                    .clamp(self.left as f64, (self.left + self.width - 1) as f64)
                    as u32;
                let right = ((x_max + 0.5).floor() + 1.)
                    .clamp((self.left + 1) as f64, (self.left + self.width) as f64)
                    as u32;
                if right <= left {
                    continue;
                }
                let src = (y as usize * texture.width as usize + left as usize) * 4;
                let dst = ((y - self.top) as usize * self.width as usize
                    + (left - self.left) as usize)
                    * 4;
                let len = (right - left) as usize * 4;
                pixels[dst..dst + len].copy_from_slice(&texture.rgba[src..src + len]);
            }
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(image: &TextureData, uv: Vec2) -> [f64; 4] {
        let x = uv.x as f64 * image.width as f64 - 0.5;
        let y = (1. - uv.y as f64) * image.height as f64 - 0.5;
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let pixel = |dx: f64, dy: f64, channel| {
            let x = (x.floor() + dx).clamp(0., (image.width - 1) as f64) as usize;
            let y = (y.floor() + dy).clamp(0., (image.height - 1) as f64) as usize;
            image.rgba[(y * image.width as usize + x) * 4 + channel] as f64
        };
        std::array::from_fn(|c| {
            (pixel(0., 0., c) * (1. - fx) + pixel(1., 0., c) * fx) * (1. - fy)
                + (pixel(0., 1., c) * (1. - fx) + pixel(1., 1., c) * fx) * fy
        })
    }
    #[test]
    fn atlas_linear_samples_survive_rotation_shear_and_out_of_range_uvs() {
        let texture = TextureData {
            width: 17,
            height: 13,
            rgba: (0..17 * 13)
                .flat_map(|i| [(i * 17 % 251) as u8, (i * 29 % 251) as u8, 47, 255])
                .collect::<Vec<_>>()
                .into(),
            cache_key: "fixture".into(),
        };
        for uvs in [
            [[0.2, 0.3], [0.8, 0.4], [0.7, 0.9]],
            [[0.4, 0.1], [0.7, 0.7], [0.2, 0.8]],
            [[-0.4, 0.3], [0.6, 1.3], [1.2, -0.3]],
            [[-1.2, 0.3], [-0.7, 0.6], [-0.3, 0.9]],
            [[1.2, 1.3], [1.5, 1.2], [1.6, 1.7]],
        ] {
            let mesh = Mesh {
                vertex_ids: vec![0, 1, 2],
                uvs: uvs.map(|p| Vec2::new(p[0], p[1])).into(),
                triangles: vec![[0, 1, 2]],
                ..Default::default()
            };
            let crop = Crop::new(&mesh, texture.width, texture.height).unwrap();
            let image = TextureData {
                width: crop.width,
                height: crop.height,
                rgba: crop.extract(&mesh, &texture).unwrap().into(),
                cache_key: "crop".into(),
            };
            let mut rebased = mesh.uvs.clone();
            crop.rebase(&mut rebased, texture.width, texture.height);
            for a in 0..=20 {
                for b in 0..=20 - a {
                    let weights = [a as f32 / 20., b as f32 / 20., (20 - a - b) as f32 / 20.];
                    let interpolate = |points: &[Vec2]| {
                        Vec2::new(
                            (0..3).map(|i| points[i].x * weights[i]).sum(),
                            (0..3).map(|i| points[i].y * weights[i]).sum(),
                        )
                    };
                    let expected = sample(&texture, interpolate(&mesh.uvs));
                    let actual = sample(&image, interpolate(&rebased));
                    for c in 0..4 {
                        assert!(
                            (actual[c] - expected[c]).abs() < 0.01,
                            "{uvs:?}: {expected:?} != {actual:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn atlas_normalization_keeps_an_independent_original_image_intact() {
        let mut document = DocumentState::new(
            uuid::Uuid::new_v4().to_string(),
            zipzi_core::Canvas::new(64., 64., Vec2::default(), 1.).into(),
        )
        .unwrap()
        .builder();
        let store = ResourceStore::default();
        let image = store
            .prepare_rgba(8, 8, vec![127; 8 * 8 * 4].into())
            .unwrap();
        let asset = AssetRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Original artwork".into(),
            content: AssetContent::Image {
                resource: image.key(),
                width: 8,
                height: 8,
            },
        };
        assert!(document.create_asset(asset.clone()).is_ok());
        assert!(document
            .create_mesh(Mesh {
                id: uuid::Uuid::new_v4().to_string(),
                runtime_id: "Piece".into(),
                name: "Piece".into(),
                texture_asset_id: asset.id.clone().into(),
                vertex_ids: vec![0, 1, 2],
                base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                uvs: vec![
                    Vec2::new(0.2, 0.2),
                    Vec2::new(0.8, 0.2),
                    Vec2::new(0.2, 0.8)
                ],
                triangles: vec![[0, 1, 2]],
                ..Default::default()
            })
            .is_ok());
        let document = document.freeze().unwrap().state;
        let resources = ResourceView::default().with_images([image.clone()]);
        let (next, view) =
            unpack_atlas_images(&document, &resources, &store, &HashSet::new()).unwrap();
        assert_eq!(document.content_id(), next.content_id());
        assert_eq!(
            view.read_asset(&asset).unwrap().rgba.as_slice(),
            image.texture().rgba.as_ref()
        );

        let mut builder = document.builder();
        let mut copy = document.mesh(&document.mesh_order()[0]).unwrap().clone();
        copy.id = uuid::Uuid::new_v4().to_string();
        copy.runtime_id = "OtherPiece".into();
        copy.name = "Other piece".into();
        builder.create_mesh(copy).unwrap();
        let document = builder.freeze().unwrap().state;
        let (next, view) =
            unpack_atlas_images(&document, &resources, &store, &HashSet::new()).unwrap();
        assert_eq!(next.asset_order().len(), 2);
        assert_eq!(view.len(), 1);
    }
}
