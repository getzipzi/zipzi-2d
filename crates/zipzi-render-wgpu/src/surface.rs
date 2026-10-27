//! Pixel-aligned group crops and a dry run of command-local color allocations.
use zipzi_core::{Status, Vec2};
use zipzi_render::{surface_layout_bounds, Affine2, TargetId, TargetItem, ViewportConfig};

use crate::modern::{WgpuOutputMode, WgpuRenderer};

#[derive(Clone, Copy)]
pub(crate) struct SurfaceLayout {
    pub size: (u32, u32),
    pub transform: Affine2,
    pub screen_origin: Vec2,
}

pub(crate) struct ColorLayout {
    pub surfaces: Vec<SurfaceLayout>,
    pub bytes: u64,
}

#[derive(Clone)]
pub(crate) struct PooledTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub size: (u32, u32),
}

impl PooledTexture {
    pub fn new(device: &wgpu::Device, size: (u32, u32), format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zipzi.surface"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            size,
        }
    }
}

impl WgpuRenderer {
    pub(crate) fn color_layout(
        &self,
        view: ViewportConfig,
        mode: WgpuOutputMode,
    ) -> Result<ColorLayout, Status> {
        let layout = |bounds| -> Result<SurfaceLayout, Status> {
            let (size, transform) =
                surface_layout_bounds(bounds, view.transform, view.target_extent)?;
            Ok(SurfaceLayout {
                size: (size.width as u32, size.height as u32),
                transform,
                screen_origin: Vec2::new(
                    view.transform.origin.x - transform.origin.x,
                    view.transform.origin.y - transform.origin.y,
                ),
            })
        };
        let main = SurfaceLayout {
            size: (self.target.width, self.target.height),
            transform: view.transform,
            screen_origin: Vec2::default(),
        };
        let mut surfaces = vec![main];
        for target in self.scene.targets().iter().skip(1) {
            surfaces.push(layout(target.bounds)?);
        }
        let bytes = self.color_bytes(&surfaces, mode);
        let common = layout(self.scene.effective_bounds())?;
        let mut shared = vec![common; surfaces.len()];
        shared[0] = main;
        let shared_bytes = self.color_bytes(&shared, mode);
        Ok(
            if !cfg!(feature = "benchmark-global-surfaces") && bytes <= shared_bytes {
                ColorLayout { surfaces, bytes }
            } else {
                ColorLayout {
                    surfaces: shared,
                    bytes: shared_bytes,
                }
            },
        )
    }

    pub(crate) fn item_reads_destination(&self, item: TargetItem) -> bool {
        match item {
            TargetItem::Draw(id) => {
                let d = &self.frame.drawables[id.0];
                self.scene.meshes()[id.0].reads_destination
                    && d.visible
                    && d.opacity > 0.
                    && !d.indices.is_empty()
            }
            TargetItem::Composite(id) => {
                let target = &self.scene.targets()[id.0];
                target.active && target.reads_destination
            }
        }
    }

    fn color_bytes(&self, layouts: &[SurfaceLayout], _mode: WgpuOutputMode) -> u64 {
        fn acquire(pool: &mut Vec<(u32, u32)>, bytes: &mut u64, size: (u32, u32)) {
            if let Some(i) = pool.iter().position(|s| *s == size) {
                pool.swap_remove(i);
            } else {
                *bytes = bytes.saturating_add(u64::from(size.0) * u64::from(size.1) * 4);
            }
        }
        fn visit(
            renderer: &WgpuRenderer,
            layouts: &[SurfaceLayout],
            id: TargetId,
            pool: &mut Vec<(u32, u32)>,
            bytes: &mut u64,
        ) {
            for &item in &renderer.scene.targets()[id.0].items {
                if let TargetItem::Composite(child) = item {
                    if renderer.scene.targets()[child.0].active {
                        acquire(pool, bytes, layouts[child.0].size);
                        visit(renderer, layouts, child, pool, bytes);
                    }
                }
                if renderer.item_reads_destination(item) {
                    acquire(pool, bytes, layouts[id.0].size);
                    pool.push(layouts[id.0].size);
                }
                if let TargetItem::Composite(child) = item {
                    if renderer.scene.targets()[child.0].active {
                        pool.push(layouts[child.0].size);
                    }
                }
            }
        }
        let mut pool = Vec::new();
        let mut bytes = 0;
        acquire(&mut pool, &mut bytes, layouts[TargetId::MAIN.0].size);
        visit(self, layouts, TargetId::MAIN, &mut pool, &mut bytes);
        bytes
    }
}
