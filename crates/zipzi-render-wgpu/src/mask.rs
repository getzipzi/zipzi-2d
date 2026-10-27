//! Viewport cropping and density policy follows the native Metal renderer.
use crate::{surface::PooledTexture, WgpuRenderer, WgpuTextureCatalog};
use std::collections::HashMap;
use zipzi_core::{Status, Vec2};
use zipzi_render::gpu::{inverse_affine, mask_layout_from_bounds, MaskLayout};
use zipzi_render::{Bounds2, MaskId, ViewportConfig};
/// Keep the best available density before sacrificing resolution. Color-only
/// exhaustion remains an error, even after every mask has reached one texel.
pub(super) fn fit_mask_budget(
    viewport: ViewportConfig,
    device_limit: u32,
    mut budget: impl FnMut(ViewportConfig, MaskViewport) -> Result<u64, Status>,
) -> Result<(ViewportConfig, MaskViewport, u64), Status> {
    let mut mask_view = MaskViewport::new(viewport)?;
    mask_view.max_dimension = mask_view.max_dimension.min(device_limit);
    let rounded_scale = viewport.mask_scale.log2().ceil().exp2();
    let rounded_view = ViewportConfig {
        mask_scale: rounded_scale,
        ..viewport
    };
    if rounded_scale.is_finite() && rounded_scale > 0. {
        match budget(rounded_view, mask_view) {
            Ok(bytes) => return Ok((rounded_view, mask_view, bytes)),
            Err(error) if error.code == "OFFSCREEN_BUDGET_EXCEEDED" => {}
            Err(error) => return Err(error),
        }
    }
    loop {
        match budget(viewport, mask_view) {
            Ok(bytes) => return Ok((viewport, mask_view, bytes)),
            Err(error)
                if error.code == "OFFSCREEN_BUDGET_EXCEEDED" && mask_view.max_dimension > 1 =>
            {
                mask_view.max_dimension /= 2;
            }
            Err(error) => return Err(error),
        }
    }
}

// A fully offscreen mask has zero coverage, including for inverted consumers.
// Its 1x1 attachment is cleared without drawing the source geometry.
pub(super) const EMPTY_MASK_LAYOUT: MaskLayout = MaskLayout {
    width: 1,
    height: 1,
    origin: Vec2 { x: 0., y: 0. },
    logical_size: Vec2 { x: 1., y: 1. },
    scale: 1.,
};

#[derive(Clone, Copy)]
pub(super) struct MaskViewport {
    pub(super) bounds: Bounds2,
    pub(super) max_dimension: u32,
}

impl MaskViewport {
    pub(crate) fn new(viewport: ViewportConfig) -> Result<Self, Status> {
        let inverse = inverse_affine(viewport.transform)?;
        // A small screen-space apron and snapped bounds make small pans share
        // coverage. The layout helper retains its four canvas-pixel border.
        let margin = 64.;
        let min = Vec2::new(-margin, -margin);
        let max = Vec2::new(
            viewport.target_extent.x + margin,
            viewport.target_extent.y + margin,
        );
        let mut bounds = Bounds2::from_point(inverse.transform_point(min));
        for point in [Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)] {
            bounds = bounds.include(inverse.transform_point(point));
        }
        if !bounds.position.x.is_finite()
            || !bounds.position.y.is_finite()
            || !bounds.size.x.is_finite()
            || !bounds.size.y.is_finite()
        {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Visible mask bounds overflowed.",
            ));
        }
        let tile = Vec2::new(
            margin * (inverse.a.x.abs() + inverse.b.x.abs()),
            margin * (inverse.a.y.abs() + inverse.b.y.abs()),
        );
        if !tile.x.is_finite() || !tile.y.is_finite() || tile.x <= 0. || tile.y <= 0. {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Visible mask grid overflowed.",
            ));
        }
        let end = bounds.end();
        let origin = Vec2::new(
            (bounds.position.x / tile.x).floor() * tile.x,
            (bounds.position.y / tile.y).floor() * tile.y,
        );
        bounds = Bounds2 {
            position: origin,
            size: Vec2::new(
                (end.x / tile.x).ceil() * tile.x - origin.x,
                (end.y / tile.y).ceil() * tile.y - origin.y,
            ),
        };
        Ok(Self {
            bounds,
            max_dimension: 4096,
        })
    }

    pub(crate) fn layout(self, bounds: Bounds2, scale: f64) -> Result<Option<MaskLayout>, Status> {
        let min = Vec2::new(
            bounds.position.x.max(self.bounds.position.x),
            bounds.position.y.max(self.bounds.position.y),
        );
        let max = Vec2::new(
            bounds.end().x.min(self.bounds.end().x),
            bounds.end().y.min(self.bounds.end().y),
        );
        if max.x <= min.x || max.y <= min.y {
            return Ok(None);
        }
        // Cropping must not change the sampling phase. The screen-space crop
        // grid is generally fractional in mask texels (e.g. zoom 20 with a
        // rounded mask density of 32), which otherwise makes edges shimmer
        // whenever panning crosses a crop boundary. Align to the uncropped
        // source's grid before adding the filtering border.
        let layout = mask_layout_from_bounds(min, max, scale, self.max_dimension)?;
        if layout.scale != scale as f32 || !layout.scale.is_finite() || layout.scale <= 0. {
            // Resolution-limited layouts already change density with their
            // extent. Keep their dimensions within the selected budget cap.
            return Ok(Some(layout));
        }
        let align = |value: f32, origin: f32| {
            let density = f64::from(layout.scale);
            (f64::from(origin)
                + ((f64::from(value) - f64::from(origin)) * density).floor() / density)
                as f32
        };
        let min = Vec2::new(
            align(min.x, bounds.position.x),
            align(min.y, bounds.position.y),
        );
        mask_layout_from_bounds(min, max, scale, self.max_dimension).map(Some)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct MaskCacheKey {
    pub(super) sources: Vec<String>,
    // Cropped origin and extent must participate in cache identity, even when
    // density and source geometry are unchanged by a pan or resize.
    pub(super) layout: Option<[u32; 7]>,
}

impl MaskCacheKey {
    pub(crate) fn new(sources: &[String], layout: Option<MaskLayout>) -> Self {
        let mut unique = Vec::new();
        for source in sources {
            if !unique.contains(source) {
                unique.push(source.clone());
            }
        }
        Self {
            sources: unique,
            layout: layout.map(|layout| {
                [
                    layout.width,
                    layout.height,
                    layout.origin.x.to_bits(),
                    layout.origin.y.to_bits(),
                    layout.logical_size.x.to_bits(),
                    layout.logical_size.y.to_bits(),
                    layout.scale.to_bits(),
                ]
            }),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MaskSourceStamp {
    pub view: wgpu::TextureView,
    pub revision: Option<u64>,
    pub geometry: u64,
    pub repeat: bool,
    pub double_sided: bool,
}
#[derive(Clone)]
pub(crate) struct MaskAttachment {
    pub texture: PooledTexture,
    pub layout: MaskLayout,
    pub region: [f32; 4],
}
pub(crate) struct CachedMask {
    pub attachment: MaskAttachment,
    pub sources: Vec<MaskSourceStamp>,
}
pub(crate) struct MaskJob {
    pub key: MaskCacheKey,
    pub layout: MaskLayout,
    pub sources: Vec<usize>,
    pub stamps: Vec<MaskSourceStamp>,
}
impl WgpuRenderer {
    pub(crate) fn mask_jobs(
        &self,
        view: ViewportConfig,
        mask_view: MaskViewport,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<(Vec<MaskJob>, HashMap<String, usize>), Status> {
        let mut jobs: Vec<MaskJob> = Vec::new();
        let mut consumers = HashMap::new();
        let mut keys = HashMap::new();
        let mut add =
            |id: &str, mask: MaskId, sources: &[String], scale: f64| -> Result<(), Status> {
                let layout = mask_view.layout(self.scene.masks()[mask.0].bounds, scale)?;
                let key = MaskCacheKey::new(sources, layout);
                if let Some(&i) = keys.get(&key) {
                    consumers.insert(id.to_owned(), i);
                    return Ok(());
                }
                let indices: Vec<_> = key
                    .sources
                    .iter()
                    .map(|s| self.scene.mesh_id(s).unwrap().0)
                    .collect();
                let mut stamps = Vec::new();
                for &i in &indices {
                    let d = &self.frame.drawables[i];
                    let t = textures
                        .get(&d.texture_asset_id)
                        .ok_or_else(|| Status::error("MISSING_TEXTURE", &d.texture_asset_id))?;
                    stamps.push(MaskSourceStamp {
                        view: t.view.clone(),
                        revision: textures.revision(&d.texture_asset_id),
                        geometry: self.geometry[i].as_ref().map_or(0, |g| g.revision),
                        repeat: textures.repeats(&d.texture_asset_id),
                        double_sided: d.double_sided,
                    });
                }
                consumers.insert(id.to_owned(), jobs.len());
                keys.insert(key.clone(), jobs.len());
                jobs.push(MaskJob {
                    key,
                    layout: layout.unwrap_or(EMPTY_MASK_LAYOUT),
                    sources: indices,
                    stamps,
                });
                Ok(())
            };
        for (i, d) in self.frame.drawables.iter().enumerate() {
            let mesh = &self.scene.meshes()[i];
            if d.renderable
                && d.visible
                && d.opacity > 0.
                && !d.indices.is_empty()
                && self.scene.targets()[mesh.target.0].active
            {
                if let Some(mask) = mesh.mask {
                    add(&d.id, mask, &d.masks, view.mask_scale)?;
                }
            }
        }
        for (i, d) in self.frame.offscreens.iter().enumerate() {
            let target = &self.scene.targets()[i + 1];
            if target.active {
                if let Some(mask) = target.mask {
                    add(&d.id, mask, &d.masks, view.mask_scale.max(1.))?;
                }
            }
        }
        Ok((jobs, consumers))
    }
}
