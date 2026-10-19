//! Mask viewport policy, attachment budgets, atlases, and coverage caching.
use super::context::create_texture;
use super::encoder::FrameEncoder;
use super::pipeline::Uniform;
use super::MetalRenderer;
use kasane_core::{evaluation::DrawableFrame, Status, Vec2};
use kasane_render::gpu::{inverse_affine, mask_layout_from_bounds, MaskLayout};
use kasane_render::{Affine2, Bounds2, MaskId, ViewportConfig};
use metal::{MTLPixelFormat, Texture, TextureRef};

impl MetalRenderer {
    pub(super) fn mask_layout(
        &self,
        mask: MaskId,
        scale: f64,
        view: MaskViewport,
    ) -> Result<Option<MaskLayout>, Status> {
        let bounds = self.scene.masks()[mask.0].bounds;
        view.layout(bounds, scale)
    }

    pub(super) fn validate_attachment_budget(
        &self,
        frame: &DrawableFrame,
        viewport: ViewportConfig,
        color_bytes: u64,
        mask_view: MaskViewport,
    ) -> Result<u64, Status> {
        let bytes = |size: (u32, u32)| u64::from(size.0) * u64::from(size.1) * 4;
        // The color dry run follows the same acquire/release order and exact
        // extent matching as FrameEncoder, including mixed-size pool storage.
        let mut total = color_bytes.saturating_add(bytes((self.target.width, self.target.height)));
        let mut masks = std::collections::HashSet::new();
        let mut count_mask = |sources: &[String], mask: MaskId, scale: f64| -> Result<(), Status> {
            let layout = self.mask_layout(mask, scale, mask_view)?;
            if masks.insert(MaskCacheKey::new(sources, layout)) {
                let layout = layout.unwrap_or(EMPTY_MASK_LAYOUT);
                total = total.saturating_add(bytes((layout.width, layout.height)));
            }
            Ok(())
        };
        for (index, drawable) in frame.drawables.iter().enumerate() {
            if !drawable.masks.is_empty()
                && drawable.visible
                && drawable.opacity > 0.
                && !drawable.indices.is_empty()
                && self.scene.targets()[self.scene.meshes()[index].target.0].active
            {
                count_mask(
                    &drawable.masks,
                    self.scene.meshes()[index].mask.unwrap(),
                    viewport.mask_scale,
                )?;
            }
        }
        for (index, offscreen) in frame.offscreens.iter().enumerate() {
            if self.scene.targets()[index + 1].active && !offscreen.masks.is_empty() {
                count_mask(
                    &offscreen.masks,
                    self.scene.targets()[index + 1].mask.unwrap(),
                    viewport.mask_scale.max(1.0),
                )?;
            }
        }
        if total > kasane_render::OFFSCREEN_BUDGET_BYTES as u64 {
            return Err(Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "Metal attachments exceed the configured budget.",
            ));
        }
        Ok(total)
    }
}

#[derive(Clone)]
pub(super) struct MaskAttachment {
    pub(super) texture: Texture,
    pub(super) layout: MaskLayout,
    pub(super) region: [f32; 4],
}

/// Keep the best available density before sacrificing resolution. Color-only
/// exhaustion remains an error, even after every mask has reached one texel.
pub(super) fn fit_mask_budget(
    viewport: ViewportConfig,
    mut budget: impl FnMut(ViewportConfig, MaskViewport) -> Result<u64, Status>,
) -> Result<(ViewportConfig, MaskViewport, u64), Status> {
    let mut mask_view = MaskViewport::new(viewport)?;
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
    fn new(viewport: ViewportConfig) -> Result<Self, Status> {
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

    fn layout(self, bounds: Bounds2, scale: f64) -> Result<Option<MaskLayout>, Status> {
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
    fn new(sources: &[String], layout: Option<MaskLayout>) -> Self {
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
pub(super) struct MaskSourceStamp {
    double_sided: bool,
    pub(super) geometry: u64,
    pub(super) texture: usize,
    pub(super) revision: Option<u64>,
    pub(super) repeat: bool,
}

pub(super) struct CachedMask {
    pub(super) attachment: MaskAttachment,
    pub(super) sources: Vec<MaskSourceStamp>,
    // Keep source identities alive while their raw pointers are in the stamps.
    pub(super) _textures: Vec<Texture>,
}

impl FrameEncoder<'_, '_> {
    /// Batch large sets of raw masks into shelves at their original pixel
    /// density. A dirty atlas is rebuilt as a unit; unchanged atlases still
    /// reuse their rendered coverage. Queue ordering protects in-flight reads.
    pub(super) fn prepare_mask_atlas(&mut self) -> Result<(), Status> {
        const SIDE: u32 = 1024;
        if self.renderer.scene.masks().len() < 32 {
            if !self.atlas_pages.is_empty() {
                // Do not keep large atlas pages alive via a few small masks.
                self.mask_cache.clear();
            }
            self.atlas_pages.clear();
            return Ok(());
        }
        let mut jobs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut add = |sources: &[String], mask: MaskId, scale: f64| -> Result<(), Status> {
            let layout = self.renderer.mask_layout(mask, scale, self.mask_view)?;
            let key = MaskCacheKey::new(sources, layout);
            if seen.insert(key.clone()) {
                jobs.push((key, layout.unwrap_or(EMPTY_MASK_LAYOUT)));
            }
            Ok(())
        };
        for (i, drawable) in self.frame.drawables.iter().enumerate() {
            let mesh = &self.renderer.scene.meshes()[i];
            if drawable.visible
                && drawable.opacity > 0.
                && !drawable.indices.is_empty()
                && self.renderer.scene.targets()[mesh.target.0].active
            {
                if let Some(mask) = mesh.mask {
                    add(&drawable.masks, mask, self.viewport.mask_scale)?;
                }
            }
        }
        for (i, offscreen) in self.frame.offscreens.iter().enumerate() {
            let target = &self.renderer.scene.targets()[i + 1];
            if target.active {
                if let Some(mask) = target.mask {
                    add(&offscreen.masks, mask, self.viewport.mask_scale.max(1.))?;
                }
            }
        }
        let all_valid = jobs.iter().all(|(key, _)| {
            self.mask_cache.get(key).is_some_and(|cached| {
                key.sources.len() == cached.sources.len()
                    && key.sources.iter().zip(&cached.sources).all(|(id, stamp)| {
                        let index = self.renderer.scene.mesh_id(id).unwrap().0;
                        stamp.revision.is_some()
                            && self
                                .mask_source_stamp(index)
                                .is_ok_and(|current| *stamp == current)
                    })
            })
        });
        let logical_bytes = self.attachment_bytes;
        let cached_bytes: u64 = self
            .atlas_pages
            .iter()
            .map(|t| t.width() * t.height() * 4)
            .sum();
        if all_valid
            && logical_bytes.saturating_add(cached_bytes)
                <= kasane_render::OFFSCREEN_BUDGET_BYTES as u64
        {
            if jobs.is_empty() {
                self.atlas_pages.clear();
            }
            return Ok(());
        }
        self.mask_cache.clear();
        let mut sizes: Vec<(u32, u32)> = Vec::new();
        let (mut x, mut y, mut row_height) = (0, 0, 0);
        for (key, layout) in jobs {
            if sizes.is_empty()
                || layout.width > SIDE
                || layout.height > SIDE
                || sizes.last().is_some_and(|&(w, h)| w > SIDE || h > SIDE)
            {
                sizes.push((0, 0));
                (x, y, row_height) = (0, 0, 0);
            }
            if x + layout.width > SIDE && x != 0 {
                y += row_height;
                x = 0;
                row_height = 0;
            }
            if y + layout.height > SIDE && y != 0 {
                sizes.push((0, 0));
                (x, y, row_height) = (0, 0, 0);
            }
            let page = sizes.len() - 1;
            self.atlas_slots.insert(key, (page, x, y));
            sizes[page].0 = sizes[page].0.max(x + layout.width);
            sizes[page].1 = sizes[page].1.max(y + layout.height);
            x += layout.width;
            row_height = row_height.max(layout.height);
        }
        // Include shelf waste in the resource budget, conservatively retaining
        // the logical-mask estimate as well as physical atlas storage.
        let atlas_bytes: u64 = sizes
            .iter()
            .map(|&(w, h)| u64::from(w) * u64::from(h) * 4)
            .sum();
        if logical_bytes.saturating_add(atlas_bytes) > kasane_render::OFFSCREEN_BUDGET_BYTES as u64
        {
            self.atlas_slots.clear();
            self.atlas_pages.clear();
            return Ok(()); // Fall back to tightly sized individual masks.
        }
        let old_pages = std::mem::take(&mut self.atlas_pages);
        self.atlas_pages = sizes
            .into_iter()
            .enumerate()
            .map(|(i, (w, h))| {
                old_pages
                    .get(i)
                    .filter(|t| t.width() == w as u64 && t.height() == h as u64)
                    .cloned()
                    .unwrap_or_else(|| {
                        create_texture(&self.renderer.device, w, h, MTLPixelFormat::RGBA8Unorm)
                    })
            })
            .collect();
        Ok(())
    }

    pub(super) fn encode_masks(&mut self) -> Result<(), Status> {
        for (index, drawable) in self.frame.drawables.iter().enumerate() {
            if !drawable.masks.is_empty()
                && drawable.visible
                && drawable.opacity > 0.
                && !drawable.indices.is_empty()
                && self.renderer.scene.targets()[self.renderer.scene.meshes()[index].target.0]
                    .active
            {
                self.encode_mask(
                    &drawable.id,
                    &drawable.masks,
                    self.renderer.scene.meshes()[index].mask.unwrap(),
                    self.viewport.mask_scale,
                )?;
            }
        }
        for (index, offscreen) in self.frame.offscreens.iter().enumerate() {
            if !offscreen.masks.is_empty() && self.renderer.scene.targets()[index + 1].active {
                self.encode_mask(
                    &offscreen.id,
                    &offscreen.masks,
                    self.renderer.scene.targets()[index + 1].mask.unwrap(),
                    self.viewport.mask_scale.max(1.0),
                )?;
            }
        }
        Ok(())
    }

    /// Atlas validation and individual masks must compare the same source state.
    fn mask_source_stamp(&self, index: usize) -> Result<MaskSourceStamp, Status> {
        let source = &self.frame.drawables[index];
        let texture = self
            .textures
            .get(&source.texture_asset_id)
            .ok_or_else(|| Status::error("MISSING_TEXTURE", &source.texture_asset_id))?;
        Ok(MaskSourceStamp {
            double_sided: source.double_sided,
            geometry: self.renderer.meshes[index].revision,
            texture: texture.view as *const TextureRef as usize,
            revision: self.textures.revision(&source.texture_asset_id),
            repeat: self.textures.repeats(&source.texture_asset_id),
        })
    }

    fn encode_mask(
        &mut self,
        id: &str,
        sources: &[String],
        mask: MaskId,
        scale: f64,
    ) -> Result<(), Status> {
        let layout = self.renderer.mask_layout(mask, scale, self.mask_view)?;
        let empty = layout.is_none();
        let key = MaskCacheKey::new(sources, layout);
        let layout = layout.unwrap_or(EMPTY_MASK_LAYOUT);
        let source_indices: Vec<_> = key
            .sources
            .iter()
            .map(|id| {
                self.renderer
                    .scene
                    .mesh_id(id)
                    .map(|id| id.0)
                    .ok_or_else(|| Status::error("INVALID_MASK", id))
            })
            .collect::<Result<_, _>>()?;
        let stamps: Vec<_> = source_indices
            .iter()
            .map(|&index| self.mask_source_stamp(index))
            .collect::<Result<_, Status>>()?;
        // Unversioned textures may be changed in-place by the host. They can
        // share a mask within this encoding, but never reuse an earlier frame.
        if stamps.iter().all(|s| s.revision.is_some()) || self.used_masks.contains(&key) {
            if let Some(cached) = self.mask_cache.get(&key).filter(|c| c.sources == stamps) {
                self.masks.insert(id.to_owned(), cached.attachment.clone());
                self.used_masks.insert(key);
                self.stats.mask_cache_hits += 1;
                return Ok(());
            }
        }
        // Commands are submitted on one queue, so an earlier frame's reads
        // finish before this frame clears and rewrites the same attachment.
        let atlas_slot = self.atlas_slots.get(&key).copied();
        let texture = if let Some((page, _, _)) = atlas_slot {
            self.atlas_pages[page].clone()
        } else {
            self.mask_cache
                .get(&key)
                .filter(|cached| {
                    cached.attachment.texture.width() == layout.width as u64
                        && cached.attachment.texture.height() == layout.height as u64
                })
                .map(|cached| cached.attachment.texture.clone())
                .unwrap_or_else(|| {
                    create_texture(
                        &self.renderer.device,
                        layout.width,
                        layout.height,
                        MTLPixelFormat::RGBA8Unorm,
                    )
                })
        };
        let (_, x, y) = atlas_slot.unwrap_or((0, 0, 0));
        let size = (texture.width() as u32, texture.height() as u32);
        let transform = Affine2 {
            a: Vec2::new(layout.scale, 0.0),
            b: Vec2::new(0.0, layout.scale),
            origin: Vec2::new(
                -layout.origin.x * layout.scale + x as f32,
                -layout.origin.y * layout.scale + y as f32,
            ),
        };
        let clear = atlas_slot.is_none_or(|(page, _, _)| self.atlas_cleared.insert(page));
        self.begin_pass(&texture, clear);
        self.pass
            .as_ref()
            .unwrap()
            .1
            .set_scissor_rect(metal::MTLScissorRect {
                x: x as u64,
                y: y as u64,
                width: layout.width as u64,
                height: layout.height as u64,
            });
        let retained_textures = source_indices
            .iter()
            .map(|&index| {
                self.textures
                    .get(&self.frame.drawables[index].texture_asset_id)
                    .unwrap()
                    .view
                    .to_owned()
            })
            .collect();
        for index in source_indices.into_iter().filter(|_| !empty) {
            let source = &self.frame.drawables[index];
            if source.indices.is_empty() {
                continue;
            }
            let texture_view = self
                .textures
                .get(&source.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &source.texture_asset_id))?;
            let mut uniform = Uniform::new(size, [1.0; 4], [0.0, 0.0, 0.0, 1.0], 1.0);
            uniform.view(transform);
            self.draw(
                &texture,
                &self.renderer.pipelines.mask,
                texture_view.view,
                None,
                None,
                &self.renderer.meshes[index],
                &uniform,
                self.textures.repeats(&source.texture_asset_id),
                source.double_sided,
                false,
            );
        }
        let attachment = MaskAttachment {
            texture,
            layout,
            region: [
                x as f32 / size.0 as f32,
                y as f32 / size.1 as f32,
                layout.width as f32 / size.0 as f32,
                layout.height as f32 / size.1 as f32,
            ],
        };
        self.masks.insert(id.to_owned(), attachment.clone());
        self.used_masks.insert(key.clone());
        self.mask_cache.insert(
            key,
            CachedMask {
                attachment,
                sources: stamps,
                _textures: retained_textures,
            },
        );
        self.stats.masks += 1;
        Ok(())
    }
}
#[cfg(test)]
mod mask_tests {
    use super::*;

    fn view() -> ViewportConfig {
        ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(4096., 4096.),
            mask_scale: 32.,
        }
    }

    #[test]
    fn visible_mask_budget_reduces_resolution_and_keeps_color_limit() {
        let bounds = Bounds2 {
            position: Vec2::default(),
            size: Vec2::new(8000., 8000.),
        };
        let colors = 4096u64 * 4096 * 4;
        let budget = kasane_render::OFFSCREEN_BUDGET_BYTES as u64;
        let (_, policy, bytes) = fit_mask_budget(view(), |view, policy| {
            let layout = policy.layout(bounds, view.mask_scale)?.unwrap();
            let bytes = colors + 20 * u64::from(layout.width) * u64::from(layout.height) * 4;
            if bytes > budget {
                Err(Status::error("OFFSCREEN_BUDGET_EXCEEDED", "test"))
            } else {
                Ok(bytes)
            }
        })
        .unwrap();
        assert_eq!(policy.max_dimension, 2048);
        assert!(bytes <= budget);
        let error = fit_mask_budget(view(), |_, _| {
            Err(Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "Color storage alone exceeds the budget.",
            ))
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "OFFSCREEN_BUDGET_EXCEEDED");
    }

    #[test]
    fn exact_density_is_tried_before_reducing_resolution() {
        let bounds = Bounds2 {
            position: Vec2::default(),
            size: Vec2::new(64., 64.),
        };
        let (_, policy, bytes) = fit_mask_budget(
            ViewportConfig {
                mask_scale: 1.1,
                ..view()
            },
            |view, policy| {
                let layout = policy.layout(bounds, view.mask_scale)?.unwrap();
                let bytes = u64::from(layout.width) * u64::from(layout.height) * 4;
                if bytes > 32 * 1024 {
                    Err(Status::error("OFFSCREEN_BUDGET_EXCEEDED", "test"))
                } else {
                    Ok(bytes)
                }
            },
        )
        .unwrap();
        assert_eq!(policy.max_dimension, 4096);
        assert!(bytes < 32 * 1024);
    }

    #[test]
    fn rotated_and_sheared_viewport_crop_covers_every_visible_corner() {
        let viewport = ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(24., 8.),
                b: Vec2::new(-16., 32.),
                origin: Vec2::new(-10000., -9000.),
            },
            target_extent: Vec2::new(1280., 800.),
            mask_scale: 32.,
        };
        let policy = MaskViewport::new(viewport).unwrap();
        let layout = policy
            .layout(
                Bounds2 {
                    position: Vec2::new(-8000., -8000.),
                    size: Vec2::new(16000., 16000.),
                },
                32.,
            )
            .unwrap()
            .unwrap();
        assert!(layout.width < 4096 && layout.height < 4096);
        let inverse = inverse_affine(viewport.transform).unwrap();
        for point in [
            Vec2::default(),
            Vec2::new(1280., 0.),
            Vec2::new(0., 800.),
            Vec2::new(1280., 800.),
        ] {
            let model = inverse.transform_point(point);
            assert!(
                model.x >= layout.origin.x && model.x <= layout.origin.x + layout.logical_size.x
            );
            assert!(
                model.y >= layout.origin.y && model.y <= layout.origin.y + layout.logical_size.y
            );
        }
    }
}
