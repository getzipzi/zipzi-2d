//! Viewport cropping and density policy follows the native Metal renderer.
use crate::{surface::PooledTexture, WgpuRenderer, WgpuTextureCatalog};
use std::collections::HashMap;
use zipzi_core::Status;
pub(super) use zipzi_render::gpu::fit_mask_budget;
use zipzi_render::gpu::{MaskLayout, MaskViewport, EMPTY_MASK_LAYOUT};
use zipzi_render::{MaskId, ViewportConfig};
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
