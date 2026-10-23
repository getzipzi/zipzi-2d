//! Map source-image pixels to authored coordinates without using a previous mesh
//! as an alpha mask. Only a consistent affine UV mapping can be reconstructed.
use anyhow::{Result, ensure};
use kasane_core::Vec2;
use kasane_sdk::MeshGeometry;

/// Add only the transparent pixels needed by the generated UV range. The extra
/// guard texel prevents clamp-to-edge sampling from extending edge colors.
pub(super) fn pad_texture(
    texture: &crate::SceneTexture,
    geometry: &mut MeshGeometry,
) -> Result<Option<crate::SceneTexture>> {
    let (w, h) = (texture.width as f64, texture.height as f64);
    let mut bounds = [0.0_f64, 0.0, w, h];
    for uv in &geometry.uvs {
        bounds[0] = bounds[0].min(uv.x as f64 * w);
        bounds[1] = bounds[1].min((1. - uv.y as f64) * h);
        bounds[2] = bounds[2].max(uv.x as f64 * w);
        bounds[3] = bounds[3].max((1. - uv.y as f64) * h);
    }
    let pad = |distance: f64| {
        if distance > 0.0 {
            distance.ceil() as u32 + 1
        } else {
            0
        }
    };
    let [left, top, right, bottom] = [
        pad(-bounds[0]),
        pad(-bounds[1]),
        pad(bounds[2] - w),
        pad(bounds[3] - h),
    ];
    if left + top + right + bottom == 0 {
        return Ok(None);
    }
    let width = texture
        .width
        .checked_add(left)
        .and_then(|n| n.checked_add(right));
    let height = texture
        .height
        .checked_add(top)
        .and_then(|n| n.checked_add(bottom));
    let (Some(width), Some(height)) = (width, height) else {
        anyhow::bail!("Padded texture is too large");
    };
    ensure!(
        width <= 16384 && height <= 16384 && width as u64 * height as u64 <= 64 * 1024 * 1024,
        "Padded texture is too large"
    );
    let mut rgba = vec![0; width as usize * height as usize * 4];
    for y in 0..texture.height as usize {
        let src = y * texture.width as usize * 4;
        let dst = ((y + top as usize) * width as usize + left as usize) * 4;
        rgba[dst..dst + texture.width as usize * 4]
            .copy_from_slice(&texture.rgba[src..src + texture.width as usize * 4]);
    }
    for uv in &mut geometry.uvs {
        uv.x = ((uv.x as f64 * w + left as f64) / width as f64) as f32;
        uv.y = ((uv.y as f64 * h + bottom as f64) / height as f64) as f32;
    }
    Ok(Some(crate::SceneTexture {
        asset_id: texture.asset_id.clone(),
        cache_key: format!(
            "{}:padding:{left}:{top}:{right}:{bottom}",
            texture.cache_key
        ),
        width,
        height,
        rgba: rgba.into(),
    }))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TextureTransform {
    origin: [f64; 2],
    u: [f64; 2],
    v: [f64; 2],
}

impl TextureTransform {
    pub fn from_geometry(g: &MeshGeometry) -> Result<Self> {
        ensure!(
            g.uvs.len() >= 3 && g.uvs.len() == g.positions.len(),
            "Missing texture coordinates"
        );
        ensure!(
            g.uvs
                .iter()
                .chain(&g.positions)
                .all(|p| p.x.is_finite() && p.y.is_finite()),
            "Invalid texture mapping"
        );
        let a = g.uvs[0];
        let b = (1..g.uvs.len())
            .max_by(|&i, &j| {
                let distance =
                    |p: Vec2| (p.x as f64 - a.x as f64).powi(2) + (p.y as f64 - a.y as f64).powi(2);
                distance(g.uvs[i]).total_cmp(&distance(g.uvs[j]))
            })
            .unwrap();
        let cross = |p: Vec2| {
            (g.uvs[b].x as f64 - a.x as f64) * (p.y as f64 - a.y as f64)
                - (g.uvs[b].y as f64 - a.y as f64) * (p.x as f64 - a.x as f64)
        };
        let c = (0..g.uvs.len())
            .max_by(|&i, &j| cross(g.uvs[i]).abs().total_cmp(&cross(g.uvs[j]).abs()))
            .unwrap();
        let determinant = cross(g.uvs[c]);
        ensure!(determinant.abs() > 1e-10, "Degenerate texture mapping");
        let mut result = Self {
            origin: [0.; 2],
            u: [0.; 2],
            v: [0.; 2],
        };
        for axis in 0..2 {
            let coordinate = |p: Vec2| if axis == 0 { p.x as f64 } else { p.y as f64 };
            let db = coordinate(g.positions[b]) - coordinate(g.positions[0]);
            let dc = coordinate(g.positions[c]) - coordinate(g.positions[0]);
            result.u[axis] = (db * (g.uvs[c].y as f64 - a.y as f64)
                - dc * (g.uvs[b].y as f64 - a.y as f64))
                / determinant;
            result.v[axis] = (dc * (g.uvs[b].x as f64 - a.x as f64)
                - db * (g.uvs[c].x as f64 - a.x as f64))
                / determinant;
            result.origin[axis] = coordinate(g.positions[0])
                - result.u[axis] * a.x as f64
                - result.v[axis] * a.y as f64;
        }
        ensure!(
            (result.u[0] * result.v[1] - result.u[1] * result.v[0]).abs() > 1e-8,
            "Degenerate mesh bounds"
        );
        let tolerance = result
            .u
            .iter()
            .chain(&result.v)
            .fold(1.0_f64, |n, v| n.max(v.abs()))
            * 1e-5;
        ensure!(
            g.uvs.iter().zip(&g.positions).all(|(&uv, p)| {
                let q = result.point(uv);
                (q.x as f64 - p.x as f64).abs() <= tolerance
                    && (q.y as f64 - p.y as f64).abs() <= tolerance
            }),
            "Alpha generation requires an undeformed, affine texture mapping"
        );
        Ok(result)
    }

    fn point(self, uv: Vec2) -> Vec2 {
        Vec2::new(
            (self.origin[0] + self.u[0] * uv.x as f64 + self.v[0] * uv.y as f64) as f32,
            (self.origin[1] + self.u[1] * uv.x as f64 + self.v[1] * uv.y as f64) as f32,
        )
    }

    pub fn bounds(self) -> [Vec2; 2] {
        let mut min = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut max = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for uv in [
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ] {
            let p = self.point(uv);
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        [min, max]
    }

    pub fn map(self, g: &mut MeshGeometry) {
        for (uv, position) in g.uvs.iter_mut().zip(&mut g.positions) {
            *position = self.point(*uv);
        }
        // Alpha output uses sequential vertex IDs. Keep authored winding positive.
        for face in &mut g.triangles {
            let [a, b, c] = face.map(|i| g.positions[i as usize]);
            if (b.x - a.x) as f64 * (c.y - a.y) as f64 - (b.y - a.y) as f64 * ((c.x - a.x) as f64)
                < 0.
            {
                face.swap(1, 2);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle() -> MeshGeometry {
        MeshGeometry {
            vertex_ids: vec![0, 1, 2, 3],
            positions: vec![
                Vec2::new(10., 20.),
                Vec2::new(110., 20.),
                Vec2::new(110., 220.),
                Vec2::new(10., 220.),
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        }
    }

    #[test]
    fn affine_source_recovers_rotation_reflection_and_rejects_deformation() {
        let mut g = rectangle();
        for p in &mut g.positions {
            *p = Vec2::new(300. - p.y, 180. - p.x);
        }
        let t = TextureTransform::from_geometry(&g).unwrap();
        assert_eq!(t.bounds(), [Vec2::new(80., 70.), Vec2::new(280., 170.)]);
        for (uv, p) in g.uvs.iter().zip(&g.positions) {
            assert_eq!(t.point(*uv), *p);
        }
        g.positions[3].x += 4.;
        assert!(TextureTransform::from_geometry(&g).is_err());
        g.uvs.fill(Vec2::new(0., 0.));
        assert!(TextureTransform::from_geometry(&g).is_err());
    }
}
