//! Per-command render state, target traversal, and Metal pass encoding.
use super::context::create_texture;
use super::geometry::MeshBuffers;
use super::mask::{fit_mask_budget, CachedMask, MaskAttachment, MaskCacheKey, MaskViewport};
use super::pipeline::Uniform;
use super::surface::SurfaceLayout;
use super::{MetalOutputMode, MetalRenderStats, MetalRenderer, MetalTextureCatalog};
use kasane_core::{
    evaluation::{DrawableFrame, OffscreenFrame},
    BlendMode, Status, Vec2,
};
use kasane_render::gpu::{compose_affine, inverse_affine};
use kasane_render::{Affine2, TargetId, TargetItem, ViewportConfig};
use metal::{
    CommandBufferRef, MTLClearColor, MTLCullMode, MTLIndexType, MTLLoadAction, MTLOrigin,
    MTLPrimitiveType, MTLSize, MTLStoreAction, MTLTextureUsage, MTLWinding, RenderPassDescriptor,
    RenderPipelineState, Texture, TextureRef,
};
use std::collections::HashMap;

impl MetalRenderer {
    /// Add all Metal render and blit passes to a host-owned command buffer.
    /// Commit that buffer before encoding another frame with this renderer.
    /// Use the same command queue for the lifetime of a renderer: cached mask
    /// writes and later reads are ordered by that queue, without CPU waits.
    pub fn encode(
        &mut self,
        command: &CommandBufferRef,
        output: &TextureRef,
        output_mode: MetalOutputMode,
        textures: &MetalTextureCatalog<'_>,
    ) -> Result<MetalRenderStats, Status> {
        // Encoders and render-pass descriptors are autoreleased by metal-rs.
        // Rust/Python callers need not have an AppKit event-loop pool.
        metal::objc::rc::autoreleasepool(|| {
            self.encode_inner(command, output, output_mode, textures)
        })
    }

    fn encode_inner(
        &mut self,
        command: &CommandBufferRef,
        output: &TextureRef,
        output_mode: MetalOutputMode,
        textures: &MetalTextureCatalog<'_>,
    ) -> Result<MetalRenderStats, Status> {
        let frame = self
            .frame
            .as_ref()
            .ok_or_else(|| Status::error("MISSING_MODEL", "Submit a model before encoding."))?;
        let viewport = self
            .viewport
            .ok_or_else(|| Status::error("MISSING_VIEW", "Update the view before encoding."))?;
        if output.width() != self.target.width as u64
            || output.height() != self.target.height as u64
            || output.pixel_format() != self.target.format
            || output.texture_type() != metal::MTLTextureType::D2
            || output.sample_count() != 1
            || !output.usage().contains(MTLTextureUsage::RenderTarget)
        {
            return Err(Status::error(
                "INVALID_TARGET",
                "Metal output texture extent, format, or usage differs from target.",
            ));
        }
        for drawable in &frame.drawables {
            let texture = textures
                .get(&drawable.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
            if texture.view.width() != texture.width as u64
                || texture.view.height() != texture.height as u64
                || texture.view.texture_type() != metal::MTLTextureType::D2
                || texture.view.sample_count() != 1
                || !texture.view.usage().contains(MTLTextureUsage::ShaderRead)
                || !std::ptr::eq(texture.view.device(), self.device.as_ref())
            {
                return Err(Status::error("INVALID_TEXTURE", &drawable.texture_asset_id));
            }
        }
        if !std::ptr::eq(output.device(), self.device.as_ref()) {
            return Err(Status::error(
                "DEVICE_MISMATCH",
                "Output texture belongs to another Metal device.",
            ));
        }
        let color_layout = self.color_layout(viewport, output_mode)?;
        let (viewport, mask_view, attachment_bytes) =
            fit_mask_budget(viewport, |view, mask_view| {
                self.validate_attachment_budget(frame, view, color_layout.bytes, mask_view)
            })?;
        // Replace can render directly to the host texture. Composite requires
        // an intermediate image to preserve the host's existing background.
        let main_color = if output_mode == MetalOutputMode::Replace {
            output.to_owned()
        } else {
            create_texture(
                &self.device,
                self.target.width,
                self.target.height,
                self.target.format,
            )
        };
        // Preflight failures must preserve cached coverage and upload accounting.
        // Transfer ownership only once the command is ready to encode passes.
        let mask_cache = std::mem::take(&mut self.mask_cache);
        let atlas_pages = std::mem::take(&mut self.mask_atlas_pages);
        let buffer_uploads = std::mem::take(&mut self.pending_uploads);
        let mut state = FrameEncoder {
            renderer: self,
            frame,
            viewport,
            mask_view,
            textures,
            command,
            layouts: color_layout.surfaces,
            surfaces: HashMap::new(),
            color_pool: Vec::new(),
            attachment_bytes,
            masks: HashMap::new(),
            pass: None,
            mask_cache,
            used_masks: Default::default(),
            atlas_pages,
            atlas_slots: HashMap::new(),
            atlas_cleared: Default::default(),
            stats: MetalRenderStats {
                buffer_uploads,
                attachment_budget_bytes: attachment_bytes,
                mask_resolution_limit: mask_view.max_dimension,
                color_attachment_bytes: if output_mode == MetalOutputMode::Composite {
                    u64::from(self.target.width) * u64::from(self.target.height) * 4
                } else {
                    0
                },
                ..Default::default()
            },
        };
        state.prepare_mask_atlas()?;
        state.stats.attachment_budget_bytes += state
            .atlas_pages
            .iter()
            .map(|texture| texture.width() * texture.height() * 4)
            .sum::<u64>();
        state.encode_masks()?;
        state.finish_pass();
        state.encode_target(TargetId::MAIN, &main_color)?;
        if output_mode == MetalOutputMode::Composite {
            let mut uniform = Uniform::new(
                (self.target.width, self.target.height),
                [1.0; 4],
                [0.0, 0.0, 0.0, 1.0],
                1.0,
            );
            uniform.view(Affine2 {
                a: Vec2::new(self.target.width as f32, 0.),
                b: Vec2::new(0., self.target.height as f32),
                origin: Vec2::default(),
            });
            state.draw(
                output,
                &self.pipelines.present_composite,
                &main_color,
                None,
                None,
                &self.quad,
                &uniform,
                false,
                true,
                false,
            );
        }
        state.finish_pass();
        state
            .mask_cache
            .retain(|key, _| state.used_masks.contains(key));
        let mask_cache = std::mem::take(&mut state.mask_cache);
        let atlas_pages = std::mem::take(&mut state.atlas_pages);
        let stats = state.stats;
        debug_assert_eq!(stats.color_attachment_bytes, color_layout.bytes);
        drop(state);
        self.mask_cache = mask_cache;
        self.mask_atlas_pages = atlas_pages;
        Ok(stats)
    }

    pub fn render(
        &mut self,
        output: &TextureRef,
        frame: &DrawableFrame,
        textures: &MetalTextureCatalog<'_>,
        viewport: ViewportConfig,
    ) -> Result<MetalRenderStats, Status> {
        self.sync_model(frame, textures)?;
        self.update_view(viewport)?;
        metal::objc::rc::autoreleasepool(|| {
            let queue = self.queue.clone();
            let command = queue.new_command_buffer();
            let stats = self.encode(command, output, MetalOutputMode::Replace, textures)?;
            command.commit();
            Ok(stats)
        })
    }
}
pub(super) struct FrameEncoder<'a, 'tex> {
    pub(super) renderer: &'a MetalRenderer,
    pub(super) frame: &'a DrawableFrame,
    pub(super) viewport: ViewportConfig,
    pub(super) mask_view: MaskViewport,
    pub(super) textures: &'a MetalTextureCatalog<'tex>,
    pub(super) command: &'a CommandBufferRef,
    pub(super) layouts: Vec<SurfaceLayout>,
    pub(super) surfaces: HashMap<TargetId, Texture>,
    // Local to this command: submitted frames never share writable attachments.
    pub(super) color_pool: Vec<Texture>,
    pub(super) attachment_bytes: u64,
    pub(super) masks: HashMap<String, MaskAttachment>,
    pub(super) pass: Option<(Texture, metal::RenderCommandEncoder)>,
    pub(super) mask_cache: HashMap<MaskCacheKey, CachedMask>,
    pub(super) used_masks: std::collections::HashSet<MaskCacheKey>,
    pub(super) atlas_pages: Vec<Texture>,
    pub(super) atlas_slots: HashMap<MaskCacheKey, (usize, u32, u32)>,
    pub(super) atlas_cleared: std::collections::HashSet<usize>,
    pub(super) stats: MetalRenderStats,
}

impl FrameEncoder<'_, '_> {
    fn encode_target(&mut self, id: TargetId, main: &TextureRef) -> Result<(), Status> {
        let target = &self.renderer.scene.targets()[id.0];
        if !target.active {
            return Ok(());
        }
        let items = target.items.clone();
        let target_texture = if id == TargetId::MAIN {
            main.to_owned()
        } else {
            let texture = self.acquire_color(self.layouts[id.0].size);
            self.surfaces.insert(id, texture.clone());
            self.stats.active_surfaces += 1;
            texture
        };
        let size = self.layouts[id.0].size;
        let mut clear = true;
        for item in items {
            if let TargetItem::Composite(child) = item {
                if self.renderer.scene.targets()[child.0].active {
                    self.finish_pass();
                    self.encode_target(child, main)?;
                }
            }
            let destination_read = self.renderer.item_reads_destination(item);
            let snapshot = if destination_read {
                if clear {
                    self.clear(&target_texture);
                    clear = false;
                }
                Some(self.snapshot(&target_texture, size))
            } else {
                None
            };
            let drawn = match item {
                TargetItem::Draw(mesh) => self.encode_mesh(
                    mesh.0,
                    id,
                    &target_texture,
                    size,
                    snapshot.as_deref(),
                    clear,
                )?,
                TargetItem::Composite(child) => self.encode_composite(
                    child,
                    id,
                    &target_texture,
                    size,
                    snapshot.as_deref(),
                    clear,
                )?,
            };
            if drawn {
                clear = false;
            }
            if let Some(snapshot) = snapshot {
                self.color_pool.push(snapshot);
            }
            if let TargetItem::Composite(child) = item {
                if let Some(texture) = self.surfaces.remove(&child) {
                    self.color_pool.push(texture);
                }
            }
        }
        if clear {
            self.clear(&target_texture);
        }
        Ok(())
    }

    fn encode_mesh(
        &mut self,
        index: usize,
        target_id: TargetId,
        target: &TextureRef,
        size: (u32, u32),
        destination: Option<&TextureRef>,
        clear: bool,
    ) -> Result<bool, Status> {
        let drawable = &self.frame.drawables[index];
        if !drawable.visible || drawable.opacity <= 0.0 || drawable.indices.is_empty() {
            return Ok(false);
        }
        let texture = self
            .textures
            .get(&drawable.texture_asset_id)
            .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
        let mut uniform = Uniform::new(
            size,
            drawable.multiply_color,
            drawable.screen_color,
            drawable.opacity,
        );
        uniform.view(if target_id == TargetId::MAIN {
            self.viewport.transform
        } else {
            self.layouts[target_id.0].transform
        });
        let mask = self.masks.get(&drawable.id).cloned();
        if let Some(mask) = &mask {
            uniform.mask(mask, drawable.inverted_mask);
        }
        let pipeline = if let Some(mode) = drawable.raw_blend_mode {
            uniform.raw_blend(mode);
            &self.renderer.pipelines.extended_draw
        } else {
            match drawable.blend_mode {
                BlendMode::Normal => &self.renderer.pipelines.draw,
                BlendMode::Additive => {
                    uniform.flags[2] = 1;
                    &self.renderer.pipelines.additive
                }
                BlendMode::Multiplicative => {
                    uniform.flags[2] = 2;
                    &self.renderer.pipelines.multiplicative
                }
            }
        };
        self.draw(
            target,
            pipeline,
            texture.view,
            mask.as_ref().map(|m| m.texture.as_ref()),
            destination,
            &self.renderer.meshes[index],
            &uniform,
            self.textures.repeats(&drawable.texture_asset_id),
            drawable.double_sided,
            clear,
        );
        Ok(true)
    }

    fn encode_composite(
        &mut self,
        child: TargetId,
        parent: TargetId,
        target: &TextureRef,
        size: (u32, u32),
        destination: Option<&TextureRef>,
        clear: bool,
    ) -> Result<bool, Status> {
        let group = &self.renderer.scene.targets()[child.0];
        if !group.active {
            return Ok(false);
        }
        let source = self
            .surfaces
            .get(&child)
            .ok_or_else(|| Status::error("MISSING_SURFACE", &group.id))?
            .clone();
        let offscreen: &OffscreenFrame = &self.frame.offscreens[child.0 - 1];
        let mut uniform = Uniform::new(
            size,
            offscreen.multiply_color,
            offscreen.screen_color,
            offscreen.opacity,
        );
        let layout = self.layouts[child.0];
        let source_scale = Affine2 {
            a: Vec2::new(layout.size.0 as f32, 0.0),
            b: Vec2::new(0.0, layout.size.1 as f32),
            origin: Vec2::default(),
        };
        // Crops share the viewport's pixel grid. Composite with an integer
        // translation, including between differently cropped nested targets.
        let parent_origin = self.layouts[parent.0].screen_origin;
        let composite_transform = Affine2 {
            origin: Vec2::new(
                layout.screen_origin.x - parent_origin.x,
                layout.screen_origin.y - parent_origin.y,
            ),
            ..source_scale
        };
        uniform.view(composite_transform);
        uniform.mask_view(compose_affine(
            inverse_affine(layout.transform)?,
            source_scale,
        ));
        let mask = self.masks.get(&offscreen.id).cloned();
        if let Some(mask) = &mask {
            uniform.mask(mask, offscreen.flags & 8 != 0);
        }
        let pipeline = if offscreen.blend_mode != 0 {
            uniform.raw_blend(offscreen.blend_mode);
            &self.renderer.pipelines.extended_composite
        } else {
            &self.renderer.pipelines.composite
        };
        self.draw(
            target,
            pipeline,
            &source,
            mask.as_ref().map(|m| m.texture.as_ref()),
            destination,
            &self.renderer.quad,
            &uniform,
            false,
            true,
            clear,
        );
        Ok(true)
    }

    fn acquire_color(&mut self, size: (u32, u32)) -> Texture {
        // A pooled texture may still be sampled by the current pass. End that
        // encoder before writing it again; Metal orders the tracked resource.
        self.finish_pass();
        if let Some(index) = self.color_pool.iter().position(|texture| {
            texture.width() == u64::from(size.0) && texture.height() == u64::from(size.1)
        }) {
            return self.color_pool.swap_remove(index);
        }
        self.stats.color_attachment_bytes += u64::from(size.0) * u64::from(size.1) * 4;
        create_texture(
            &self.renderer.device,
            size.0,
            size.1,
            self.renderer.target.format,
        )
    }

    fn snapshot(&mut self, source: &TextureRef, size: (u32, u32)) -> Texture {
        let destination = self.acquire_color(size);
        let blit = self.command.new_blit_command_encoder();
        blit.copy_from_texture(
            source,
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
            MTLSize::new(size.0 as u64, size.1 as u64, 1),
            &destination,
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
        );
        blit.end_encoding();
        self.stats.destination_copies += 1;
        destination
    }

    pub(super) fn finish_pass(&mut self) {
        if let Some((_, encoder)) = self.pass.take() {
            encoder.end_encoding();
        }
    }

    pub(super) fn begin_pass(&mut self, target: &TextureRef, clear: bool) {
        if !clear
            && self
                .pass
                .as_ref()
                .is_some_and(|(texture, _)| std::ptr::eq(texture.as_ref(), target))
        {
            return;
        }
        self.finish_pass();
        let descriptor = render_pass(target, clear);
        let encoder = self
            .command
            .new_render_command_encoder(descriptor)
            .to_owned();
        self.pass = Some((target.to_owned(), encoder));
        self.stats.render_passes += 1;
    }

    pub(super) fn clear(&mut self, target: &TextureRef) {
        self.begin_pass(target, true);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(
        &mut self,
        target: &TextureRef,
        pipeline: &RenderPipelineState,
        source: &TextureRef,
        mask: Option<&TextureRef>,
        destination: Option<&TextureRef>,
        mesh: &MeshBuffers,
        uniform: &Uniform,
        repeat: bool,
        double_sided: bool,
        clear: bool,
    ) {
        self.begin_pass(target, clear);
        let encoder = &self.pass.as_ref().unwrap().1;
        encoder.set_render_pipeline_state(pipeline);
        encoder.set_cull_mode(if double_sided {
            MTLCullMode::None
        } else {
            MTLCullMode::Back
        });
        // Front faces are counterclockwise in model space. Viewport and mask
        // targets may reflect Y, so account for their transform independently.
        let determinant =
            uniform.view_a[0] * uniform.view_b[1] - uniform.view_a[1] * uniform.view_b[0];
        encoder.set_front_facing_winding(if determinant < 0. {
            MTLWinding::Clockwise
        } else {
            MTLWinding::CounterClockwise
        });
        encoder.set_vertex_buffer(0, Some(&mesh.vertex), mesh.vertex_offset);
        encoder.set_vertex_bytes(
            1,
            std::mem::size_of::<Uniform>() as u64,
            (uniform as *const Uniform).cast(),
        );
        encoder.set_fragment_bytes(
            1,
            std::mem::size_of::<Uniform>() as u64,
            (uniform as *const Uniform).cast(),
        );
        encoder.set_fragment_texture(0, Some(source));
        if let Some(mask) = mask {
            encoder.set_fragment_texture(1, Some(mask));
        }
        if let Some(destination) = destination {
            encoder.set_fragment_texture(2, Some(destination));
        }
        let sampler = if repeat {
            &self.renderer.repeat_sampler
        } else {
            &self.renderer.clamp_sampler
        };
        encoder.set_fragment_sampler_state(0, Some(sampler));
        encoder.set_fragment_sampler_state(1, Some(&self.renderer.clamp_sampler));
        encoder.set_fragment_sampler_state(2, Some(&self.renderer.clamp_sampler));
        encoder.draw_indexed_primitives(
            MTLPrimitiveType::Triangle,
            mesh.index_count,
            MTLIndexType::UInt32,
            &mesh.index,
            mesh.index_offset,
        );
        self.stats.draw_calls += 1;
    }
}

impl Drop for FrameEncoder<'_, '_> {
    fn drop(&mut self) {
        self.finish_pass();
    }
}

fn render_pass(target: &TextureRef, clear: bool) -> &metal::RenderPassDescriptorRef {
    let descriptor = RenderPassDescriptor::new();
    let color = descriptor.color_attachments().object_at(0).unwrap();
    color.set_texture(Some(target));
    color.set_load_action(if clear {
        MTLLoadAction::Clear
    } else {
        MTLLoadAction::Load
    });
    color.set_store_action(MTLStoreAction::Store);
    color.set_clear_color(MTLClearColor::new(0.0, 0.0, 0.0, 0.0));
    descriptor
}
