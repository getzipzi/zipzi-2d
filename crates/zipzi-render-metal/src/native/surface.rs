//! Pixel-aligned group crops and a dry run of command-local color allocations.
use super::{MetalOutputMode, MetalRenderer};
use zipzi_core::{Status, Vec2};
use zipzi_render::{surface_layout_bounds, Affine2, TargetId, TargetItem, ViewportConfig};

#[derive(Clone, Copy)]
pub(super) struct SurfaceLayout {
    pub size: (u32, u32),
    pub transform: Affine2,
    pub screen_origin: Vec2,
}

pub(super) struct ColorLayout {
    pub surfaces: Vec<SurfaceLayout>,
    /// Allocated color/snapshot bytes, including Composite presentation storage.
    pub bytes: u64,
}

impl MetalRenderer {
    pub(super) fn color_layout(
        &self,
        view: ViewportConfig,
        mode: MetalOutputMode,
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
        // Many different extents can fragment an exact-size texture pool. Keep
        // the previous common-surface strategy when it needs less total storage.
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

    pub(super) fn item_reads_destination(&self, item: TargetItem) -> bool {
        let frame = self.frame.as_ref().expect("model submitted");
        match item {
            TargetItem::Draw(id) => {
                let d = &frame.drawables[id.0];
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

    fn color_bytes(&self, layouts: &[SurfaceLayout], mode: MetalOutputMode) -> u64 {
        fn acquire(pool: &mut Vec<(u32, u32)>, bytes: &mut u64, size: (u32, u32)) {
            if let Some(i) = pool.iter().position(|s| *s == size) {
                pool.swap_remove(i);
            } else {
                *bytes = bytes.saturating_add(u64::from(size.0) * u64::from(size.1) * 4);
            }
        }
        fn visit(
            renderer: &MetalRenderer,
            layouts: &[SurfaceLayout],
            id: TargetId,
            pool: &mut Vec<(u32, u32)>,
            bytes: &mut u64,
        ) {
            if id != TargetId::MAIN {
                acquire(pool, bytes, layouts[id.0].size);
            }
            for &item in &renderer.scene.targets()[id.0].items {
                if let TargetItem::Composite(child) = item {
                    if renderer.scene.targets()[child.0].active {
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
        let mut bytes = if mode == MetalOutputMode::Composite {
            u64::from(self.target.width) * u64::from(self.target.height) * 4
        } else {
            0
        };
        visit(self, layouts, TargetId::MAIN, &mut Vec::new(), &mut bytes);
        bytes
    }
}
