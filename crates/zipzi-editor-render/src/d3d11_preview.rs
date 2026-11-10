//! Model preview on GPUI's device. The worker records, and the UI submits.
use anyhow::{Context, Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use windows::{Win32::Graphics::Direct3D11::*, core::Interface};
use zipzi_core::{DrawableFrame, Vec2};
use zipzi_editor_core::{SceneSnapshot, ViewportTransform};
use zipzi_render::{Affine2, ViewportConfig, texture::straight_rgba_mipmaps};
use zipzi_render_d3d11::{
    D3d11Renderer, D3d11Target, D3d11Texture, D3d11TextureCatalog, upload_rgba8,
};

pub struct RecordedD3d11Preview {
    pub surface: Arc<gpui::DirectXSurface>,
    pub commands: ID3D11CommandList,
}

struct Slot {
    image: Arc<gpui::DirectXSurface>,
    target: D3d11Target,
}

impl Slot {
    fn reusable(&self) -> bool {
        Arc::strong_count(&self.image) == 1 && self.image.is_idle()
    }
}

pub struct D3d11Preview {
    device: ID3D11Device,
    renderer: D3d11Renderer,
    textures: HashMap<String, D3d11Texture>,
    synced_scene: Option<(SceneSnapshot, Arc<DrawableFrame>)>,
    slots: Vec<Slot>,
    draws: u64,
    uploads: u64,
}

impl D3d11Preview {
    pub fn new(device: ID3D11Device) -> Result<Self> {
        Ok(Self {
            renderer: D3d11Renderer::new(device.clone())?,
            device,
            textures: HashMap::new(),
            synced_scene: None,
            slots: Vec::new(),
            draws: 0,
            uploads: 0,
        })
    }

    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.draws,
            self.uploads,
            self.textures.values().map(D3d11Texture::bytes).sum(),
        )
    }

    pub fn retain_cached_textures(&mut self, live: &HashSet<String>) {
        self.textures.retain(|key, _| live.contains(key));
        self.slots.retain(|slot| !slot.reusable());
        self.renderer.clear_model();
        self.synced_scene = None;
    }

    pub fn record(
        &mut self,
        extent: [u32; 2],
        scene: &SceneSnapshot,
        transform: ViewportTransform,
        mut boundary: impl FnMut(&'static str),
    ) -> Result<Option<RecordedD3d11Preview>> {
        let Some(slot) = self.acquire(extent)? else {
            return Ok(None);
        };
        boundary("dequeued");
        boundary("worker_begin");
        let keys: Vec<_> = scene
            .textures
            .iter()
            .map(crate::scene_texture_key)
            .collect();
        for (source, key) in scene.textures.iter().zip(&keys) {
            if !self.textures.contains_key(key) {
                ensure!(
                    (1..=16384).contains(&source.width)
                        && (1..=16384).contains(&source.height)
                        && source.rgba.len() as u64
                            == u64::from(source.width) * u64::from(source.height) * 4,
                    "Invalid D3D11 preview source texture"
                );
                let mipmaps = straight_rgba_mipmaps(source.width, source.height, &source.rgba);
                let texture = upload_rgba8(
                    &self.device,
                    source.width,
                    source.height,
                    &source.rgba,
                    &mipmaps,
                )?;
                self.textures.insert(key.clone(), texture);
                self.uploads += 1;
            }
        }
        if self.stats().2 > 128 * 1024 * 1024 {
            let active: HashSet<_> = keys.iter().collect();
            self.textures.retain(|key, _| active.contains(key));
        }
        let catalog = D3d11TextureCatalog::new(
            scene
                .textures
                .iter()
                .zip(&keys)
                .map(|(source, key)| (source.asset_id.as_str(), &self.textures[key])),
        );
        boundary("textures_ready");
        if self.synced_scene.as_ref().is_none_or(|(previous, _)| {
            !Arc::ptr_eq(&previous.viewport_frame, &scene.viewport_frame)
                || previous.model_opacity != scene.model_opacity
        }) {
            self.synced_scene = Some((scene.clone(), scene.render_frame()?));
        }
        let frame = self.synced_scene.as_ref().unwrap().1.clone();
        let dpi = transform.metrics.scale_factor;
        let scale = transform.scale * dpi;
        let view = ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(scale, 0.),
                b: Vec2::new(0., -scale),
                origin: Vec2::new(transform.offset.x * dpi, transform.offset.y * dpi),
            },
            target_extent: Vec2::new(extent[0] as f32, extent[1] as f32),
            mask_scale: f64::from(scale),
        };
        let slot = &self.slots[slot];
        let commands = self
            .renderer
            .record(&slot.target, frame, view, &catalog, &mut boundary)?;
        boundary("native_d3d11_recorded");
        self.draws += 1;
        Ok(Some(RecordedD3d11Preview {
            surface: slot.image.clone(),
            commands,
        }))
    }

    fn acquire(&mut self, extent: [u32; 2]) -> Result<Option<usize>> {
        ensure!(
            extent.into_iter().all(|v| (1..=16384).contains(&v))
                && u64::from(extent[0]) * u64::from(extent[1]) * 4
                    <= crate::windows_preview::MAX_FRAME_BYTES,
            "Invalid D3D11 preview extent"
        );
        unsafe { self.device.GetDeviceRemovedReason()? };
        if let Some(slot) = self
            .slots
            .iter()
            .position(|s| s.image.extent == extent && s.reusable())
        {
            return Ok(Some(slot));
        }
        self.slots.retain(|slot| !slot.reusable());
        if self.slots.len() >= crate::windows_preview::MAX_SLOTS {
            return Ok(None);
        }
        let target = D3d11Target::new(&self.device, extent)?;
        let device: ID3D11Device5 = self.device.cast()?;
        let mut ready = None;
        let mut consumed = None;
        unsafe {
            device.CreateFence::<ID3D11Fence>(0, D3D11_FENCE_FLAG_NONE, &mut ready)?;
            device.CreateFence::<ID3D11Fence>(0, D3D11_FENCE_FLAG_NONE, &mut consumed)?;
        }
        // Production and sampling are ordered on the same immediate context.
        let image = Arc::new(gpui::DirectXSurface {
            device: self.device.clone(),
            view: target.view().clone(),
            extent,
            ready: ready.context("D3D11 preview ready fence")?,
            ready_value: 0,
            consumed: consumed.context("D3D11 preview consumed fence")?,
            consumed_value: 0.into(),
        });
        self.slots.push(Slot { image, target });
        Ok(Some(self.slots.len() - 1))
    }
}
