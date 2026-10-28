//! Command-local targets and batched render passes. No queued uniform overwrites.
use crate::{
    geometry::MeshBuffers,
    mask::{fit_mask_budget, CachedMask, MaskAttachment, MaskJob},
    pipeline::{FaceMode, Uniform, UniformKey},
    surface::{PooledTexture, SurfaceLayout},
    WgpuEncodeTarget, WgpuOutputMode, WgpuRenderStats, WgpuRenderer, WgpuTextureCatalog,
};
use std::collections::{HashMap, HashSet};
use wgpu::util::DeviceExt;
use zipzi_core::{BlendMode, Status, Vec2};
use zipzi_render::gpu::{compose_affine, inverse_affine};
use zipzi_render::{Affine2, TargetId, TargetItem};

/// Only immutable bindings to source textures and masks survive an encoding.
/// Transient color/snapshot views never enter this cache or retain their storage.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct BindingKey {
    uniform: wgpu::Buffer,
    source: wgpu::TextureView,
    mask: Option<wgpu::TextureView>,
    repeat: bool,
}
pub(crate) struct CachedBinding {
    group: wgpu::BindGroup,
    used: bool,
}
struct DrawCall {
    mesh: MeshBuffers,
    bindings: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    scissor: Option<(u32, u32, u32, u32)>,
}
struct FrameEncoder<'a, 'b> {
    renderer: &'a WgpuRenderer,
    encoder: &'a mut wgpu::CommandEncoder,
    textures: &'a WgpuTextureCatalog<'b>,
    layouts: Vec<SurfaceLayout>,
    pool: Vec<PooledTexture>,
    masks: HashMap<String, MaskAttachment>,
    stats: WgpuRenderStats,
    uniforms: HashMap<UniformKey, wgpu::Buffer>,
    bindings: HashMap<BindingKey, CachedBinding>,
    used_uniforms: HashSet<UniformKey>,
}
impl WgpuRenderer {
    // wgpu handles hash/compare by stable resource identity, not mutable contents.
    #[allow(clippy::mutable_key_type)]
    pub fn encode(
        &mut self,
        target: WgpuEncodeTarget<'_>,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<WgpuRenderStats, Status> {
        self.ensure_device(target.device)?;
        if self.scene.targets().is_empty() {
            return Err(Status::error("MISSING_MODEL", "Submit a model first"));
        }
        let view = self.viewport.ok_or_else(|| {
            Status::error("MISSING_VIEW", "Submit model and view before encoding")
        })?;
        self.validate_frame_textures(&self.frame, textures)?;
        let output = target.output.texture();
        if output.width() != self.target.width
            || output.height() != self.target.height
            || output.format() != self.target.format
            || output.dimension() != wgpu::TextureDimension::D2
            || output.depth_or_array_layers() != 1
            || output.sample_count() != 1
            || !output
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(Status::error(
                "INVALID_TARGET",
                "Output extent, format or usage differs from target",
            ));
        }
        // A view-only host does not promise COPY_SRC. Direct Replace is safe
        // when the main target never needs a destination snapshot or reads itself.
        let direct_main = target.output_mode == WgpuOutputMode::Replace
            && !self.scene.targets()[0]
                .items
                .iter()
                .any(|&item| self.item_reads_destination(item))
            && !self
                .frame
                .drawables
                .iter()
                .filter(|d| d.renderable)
                .any(|d| {
                    textures
                        .get(&d.texture_asset_id)
                        .is_some_and(|t| t.view.texture() == output)
                });
        let color = self.color_layout(view, direct_main)?;
        self.validate_layouts(&color)?;
        let mut jobs = Vec::new();
        let mut consumers = HashMap::new();
        let (_, mask_view, budget) = fit_mask_budget(
            view,
            self.device.limits().max_texture_dimension_2d,
            |v, mv| {
                let (candidate, mapping) = self.mask_jobs(v, mv, textures)?;
                let bytes = color.bytes
                    + u64::from(self.target.width) * u64::from(self.target.height) * 4
                    + candidate
                        .iter()
                        .map(|j| u64::from(j.layout.width) * u64::from(j.layout.height) * 4)
                        .sum::<u64>();
                if bytes > zipzi_render::OFFSCREEN_BUDGET_BYTES as u64 {
                    return Err(Status::error(
                        "OFFSCREEN_BUDGET_EXCEEDED",
                        "Attachments exceed budget",
                    ));
                }
                jobs = candidate;
                consumers = mapping;
                Ok(bytes)
            },
        )?;
        let mut cache = std::mem::take(&mut self.mask_cache);
        let mut pages = std::mem::take(&mut self.atlas_pages);
        let mut stats = WgpuRenderStats {
            vertex_upload_bytes: std::mem::take(&mut self.pending_vertex_bytes),
            index_upload_bytes: std::mem::take(&mut self.pending_index_bytes),
            geometry_buffer_creations: std::mem::take(&mut self.pending_uploads),
            active_surfaces: self.scene.active_target_count(),
            masks: jobs.len(),
            attachment_bytes: budget,
            mask_resolution_limit: mask_view.max_dimension,
            ..Default::default()
        };
        let slots = prepare_atlas(self, &jobs, &mut cache, &mut pages, budget);
        stats.attachment_bytes += pages
            .iter()
            .map(|t| u64::from(t.size.0) * u64::from(t.size.1) * 4)
            .sum::<u64>();
        let uniforms = std::mem::take(&mut self.uniforms);
        let mut bindings = std::mem::take(&mut self.bindings);
        for binding in bindings.values_mut() {
            binding.used = false;
        }
        let mut state = FrameEncoder {
            bindings,
            uniforms,
            used_uniforms: HashSet::new(),
            renderer: self,
            encoder: target.encoder,
            textures,
            layouts: color.surfaces,
            pool: Vec::new(),
            masks: HashMap::new(),
            stats,
        };
        let mut attachments = Vec::new();
        let mut cleared_pages = HashSet::new();
        let mut atlas_draws: Vec<Vec<DrawCall>> = (0..pages.len()).map(|_| Vec::new()).collect();
        for (index, job) in jobs.iter().enumerate() {
            if let Some(cached) = cache.get(&job.key).filter(|c| {
                c.sources == job.stamps && job.stamps.iter().all(|s| s.revision.is_some())
            }) {
                attachments.push(cached.attachment.clone());
                state.stats.mask_cache_hits += 1;
                continue;
            }
            let atlas_page = slots.get(&index).copied();
            let (texture, x, y) = if let Some((page, x, y)) = atlas_page {
                cleared_pages.insert(page);
                (pages[page].clone(), x, y)
            } else {
                let texture = cache
                    .get(&job.key)
                    .filter(|c| {
                        c.attachment.region == [0., 0., 1., 1.]
                            && c.attachment.texture.size == (job.layout.width, job.layout.height)
                    })
                    .map(|c| c.attachment.texture.clone())
                    .unwrap_or_else(|| {
                        PooledTexture::new(
                            &self.device,
                            (job.layout.width, job.layout.height),
                            wgpu::TextureFormat::Rgba8Unorm,
                        )
                    });
                (texture, 0, 0)
            };
            let l = job.layout;
            let transform = Affine2 {
                a: Vec2::new(l.scale, 0.),
                b: Vec2::new(0., l.scale),
                origin: Vec2::new(
                    -l.origin.x * l.scale + x as f32,
                    -l.origin.y * l.scale + y as f32,
                ),
            };
            let mut local_draws = Vec::new();
            let draws = if let Some((page, _, _)) = atlas_page {
                &mut atlas_draws[page]
            } else {
                &mut local_draws
            };
            if job.key.layout.is_some() {
                for &i in &job.sources {
                    let Some(mesh) = &self.geometry[i] else {
                        continue;
                    };
                    let d = &self.frame.drawables[i];
                    let uniform = Uniform::new(texture.size, transform, [1.; 4], [0.; 4], 1.);
                    let t = textures
                        .get(&d.texture_asset_id)
                        .expect("preflight textures");
                    let mut call = state.draw_call(
                        mesh,
                        &uniform,
                        t.view,
                        None,
                        None,
                        textures.repeats(&d.texture_asset_id),
                        self.pipelines
                            .mask
                            .get(FaceMode::for_draw(d.double_sided, &uniform)),
                    );
                    call.scissor = Some((x, y, l.width, l.height));
                    draws.push(call);
                }
            }
            if atlas_page.is_none() {
                let mut clear = true;
                state.flush(&texture.view, &mut local_draws, &mut clear);
            }
            let attachment = MaskAttachment {
                region: [
                    x as f32 / texture.size.0 as f32,
                    y as f32 / texture.size.1 as f32,
                    l.width as f32 / texture.size.0 as f32,
                    l.height as f32 / texture.size.1 as f32,
                ],
                texture,
                layout: l,
            };
            cache.insert(
                job.key.clone(),
                CachedMask {
                    attachment: attachment.clone(),
                    sources: job.stamps.clone(),
                },
            );
            attachments.push(attachment);
            state.stats.mask_redraws += 1;
        }
        for (page, mut draws) in atlas_draws.into_iter().enumerate() {
            if cleared_pages.contains(&page) || !draws.is_empty() {
                let mut clear = cleared_pages.contains(&page);
                state.flush(&pages[page].view, &mut draws, &mut clear);
            }
        }
        for (id, i) in consumers {
            state.masks.insert(id, attachments[i].clone());
        }
        let main = if direct_main {
            PooledTexture {
                texture: output.clone(),
                view: target.output.clone(),
                size: (self.target.width, self.target.height),
            }
        } else {
            state.acquire((self.target.width, self.target.height))
        };
        state.encode_target(TargetId::MAIN, &main)?;
        if !direct_main {
            let transform = Affine2 {
                a: Vec2::new(self.target.width as f32, 0.),
                b: Vec2::new(0., self.target.height as f32),
                origin: Vec2::default(),
            };
            let uniform = Uniform::new(main.size, transform, [1.; 4], [0.; 4], 1.);
            let pipeline = if target.output_mode == WgpuOutputMode::Replace {
                &self.pipelines.present_replace
            } else {
                &self.pipelines.present
            };
            let call = state.draw_call(
                &self.quad, &uniform, &main.view, None, None, false, pipeline,
            );
            let mut present_clear = target.output_mode == WgpuOutputMode::Replace;
            state.flush(target.output, &mut vec![call], &mut present_clear);
        }
        let stats = state.stats;
        state
            .uniforms
            .retain(|k, _| state.used_uniforms.contains(k));
        state.bindings.retain(|_, binding| binding.used);
        let bindings = state.bindings;
        let uniforms = state.uniforms;
        self.bindings = bindings;
        self.uniforms = uniforms;
        debug_assert_eq!(stats.color_attachment_bytes, color.bytes);
        cache.retain(|k, _| jobs.iter().any(|j| j.key == *k));
        self.mask_cache = cache;
        self.atlas_pages = pages;
        Ok(stats)
    }
}

fn prepare_atlas(
    renderer: &WgpuRenderer,
    jobs: &[MaskJob],
    cache: &mut HashMap<crate::mask::MaskCacheKey, CachedMask>,
    pages: &mut Vec<PooledTexture>,
    budget: u64,
) -> HashMap<usize, (usize, u32, u32)> {
    let mut slots = HashMap::new();
    if renderer.scene.masks().len() < 32 || jobs.is_empty() {
        if !pages.is_empty() {
            cache.clear();
            pages.clear();
        }
        return slots;
    }
    let old_bytes: u64 = pages
        .iter()
        .map(|t| u64::from(t.size.0) * u64::from(t.size.1) * 4)
        .sum();
    if !pages.is_empty()
        && budget + old_bytes <= zipzi_render::OFFSCREEN_BUDGET_BYTES as u64
        && jobs.iter().all(|j| {
            cache.get(&j.key).is_some_and(|c| {
                c.sources == j.stamps && j.stamps.iter().all(|s| s.revision.is_some())
            })
        })
    {
        return slots;
    }
    cache.clear();
    let side = 1024u32.min(renderer.device.limits().max_texture_dimension_2d);
    let mut sizes: Vec<(u32, u32)> = Vec::new();
    let (mut x, mut y, mut row) = (0, 0, 0);
    for (i, j) in jobs.iter().enumerate() {
        let (w, h) = (j.layout.width, j.layout.height);
        if sizes.is_empty()
            || w > side
            || h > side
            || sizes.last().is_some_and(|&(w, h)| w > side || h > side)
        {
            sizes.push((0, 0));
            (x, y, row) = (0, 0, 0);
        }
        if x + w > side && x != 0 {
            y += row;
            x = 0;
            row = 0;
        }
        if y + h > side && y != 0 {
            sizes.push((0, 0));
            (x, y, row) = (0, 0, 0);
        }
        let page = sizes.len() - 1;
        slots.insert(i, (page, x, y));
        sizes[page].0 = sizes[page].0.max(x + w);
        sizes[page].1 = sizes[page].1.max(y + h);
        x += w;
        row = row.max(h);
    }
    let bytes: u64 = sizes
        .iter()
        .map(|&(w, h)| u64::from(w) * u64::from(h) * 4)
        .sum();
    if budget + bytes > zipzi_render::OFFSCREEN_BUDGET_BYTES as u64 {
        pages.clear();
        return HashMap::new();
    }
    let old = std::mem::take(pages);
    *pages = sizes
        .into_iter()
        .enumerate()
        .map(|(i, size)| {
            old.get(i)
                .filter(|p| p.size == size)
                .cloned()
                .unwrap_or_else(|| {
                    PooledTexture::new(&renderer.device, size, wgpu::TextureFormat::Rgba8Unorm)
                })
        })
        .collect();
    slots
}

impl FrameEncoder<'_, '_> {
    fn acquire(&mut self, size: (u32, u32)) -> PooledTexture {
        if let Some(i) = self.pool.iter().position(|t| t.size == size) {
            return self.pool.swap_remove(i);
        }
        self.stats.color_attachment_bytes += u64::from(size.0) * u64::from(size.1) * 4;
        PooledTexture::new(&self.renderer.device, size, self.renderer.target.format)
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_call(
        &mut self,
        mesh: &MeshBuffers,
        uniform: &Uniform,
        source: &wgpu::TextureView,
        mask: Option<&wgpu::TextureView>,
        destination: Option<&wgpu::TextureView>,
        repeat: bool,
        pipeline: &wgpu::RenderPipeline,
    ) -> DrawCall {
        let r = self.renderer;
        let key: UniformKey = bytemuck::bytes_of(uniform).try_into().unwrap();
        self.used_uniforms.insert(key);
        let buffer = self.uniforms.entry(key).or_insert_with(|| {
            self.stats.uniform_buffer_creations += 1;
            r.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("zipzi.uniform"),
                    contents: &key,
                    usage: wgpu::BufferUsages::UNIFORM,
                })
        });
        let cache_key = (destination.is_none()
            && uniform.flags[1] == 0
            && pipeline != &r.pipelines.present
            && pipeline != &r.pipelines.present_replace)
            .then(|| BindingKey {
                uniform: buffer.clone(),
                source: source.clone(),
                mask: mask.cloned(),
                repeat,
            });
        if let Some(key) = &cache_key {
            if let Some(binding) = self.bindings.get_mut(key) {
                binding.used = true;
                return DrawCall {
                    mesh: mesh.clone(),
                    bindings: binding.group.clone(),
                    pipeline: pipeline.clone(),
                    scissor: None,
                };
            }
        }
        let sampler = if repeat {
            &r.pipelines.linear_repeat
        } else {
            &r.pipelines.linear_clamp
        };
        let bindings = r.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zipzi.draw"),
            layout: &r.pipelines.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(mask.unwrap_or(&r.fallback.view)),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&r.pipelines.linear_clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        destination.unwrap_or(&r.fallback.view),
                    ),
                },
            ],
        });
        if let Some(key) = cache_key {
            self.bindings.insert(
                key,
                CachedBinding {
                    group: bindings.clone(),
                    used: true,
                },
            );
        }
        self.stats.bind_group_creations += 1;
        DrawCall {
            mesh: mesh.clone(),
            bindings,
            pipeline: pipeline.clone(),
            scissor: None,
        }
    }
    fn flush(&mut self, target: &wgpu::TextureView, draws: &mut Vec<DrawCall>, clear: &mut bool) {
        if draws.is_empty() && !*clear {
            return;
        }
        let colors = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: if std::mem::replace(clear, false) {
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("zipzi.pass"),
            color_attachments: &colors,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.stats.render_passes += 1;
        for call in draws.iter() {
            let m = &call.mesh;
            pass.set_pipeline(&call.pipeline);
            pass.set_bind_group(0, &call.bindings, &[]);
            pass.set_vertex_buffer(
                0,
                m.positions
                    .slice(m.position_offset..m.position_offset + m.position_bytes),
            );
            pass.set_vertex_buffer(1, m.uvs.slice(m.uv_offset..m.uv_offset + m.uv_bytes));
            pass.set_index_buffer(
                m.indices
                    .slice(m.index_offset..m.index_offset + m.index_bytes),
                wgpu::IndexFormat::Uint32,
            );
            if let Some((x, y, w, h)) = call.scissor {
                pass.set_scissor_rect(x, y, w, h);
            }
            pass.draw_indexed(0..m.index_count, 0, 0..1);
            self.stats.draw_calls += 1;
        }
        drop(pass);
        draws.clear();
    }
    fn encode_target(&mut self, id: TargetId, target: &PooledTexture) -> Result<(), Status> {
        let r = self.renderer;
        let mut draws = Vec::new();
        let mut clear = true;
        let mut has_destination = false;
        for &item in &r.scene.targets()[id.0].items {
            let child = if let TargetItem::Composite(child) = item {
                if !r.scene.targets()[child.0].active {
                    continue;
                }
                self.flush(&target.view, &mut draws, &mut clear);
                let texture = self.acquire(self.layouts[child.0].size);
                self.encode_target(child, &texture)?;
                Some(texture)
            } else {
                None
            };
            let destination = if r.item_reads_destination(item) {
                self.flush(&target.view, &mut draws, &mut clear);
                let snapshot = self.acquire(target.size);
                self.encoder.copy_texture_to_texture(
                    target.texture.as_image_copy(),
                    snapshot.texture.as_image_copy(),
                    target.texture.size(),
                );
                self.stats.destination_copies += 1;
                has_destination = true;
                Some(snapshot)
            } else {
                None
            };
            let (mesh, source, mut uniform, repeat, double_sided, blend, raw) = match item {
                TargetItem::Draw(i) => {
                    let d = &r.frame.drawables[i.0];
                    if !d.visible || d.opacity <= 0. || d.indices.is_empty() {
                        continue;
                    }
                    let Some(mesh) = &r.geometry[i.0] else {
                        continue;
                    };
                    let mut u = Uniform::new(
                        target.size,
                        self.layouts[id.0].transform,
                        d.multiply_color,
                        d.screen_color,
                        d.opacity,
                    );
                    if let Some(mask) = self.masks.get(&d.id) {
                        u.mask(mask, Affine2::IDENTITY, d.inverted_mask);
                    }
                    (
                        mesh,
                        self.textures
                            .get(&d.texture_asset_id)
                            .expect("validated textures")
                            .view,
                        u,
                        self.textures.repeats(&d.texture_asset_id),
                        d.double_sided,
                        d.blend_mode,
                        d.raw_blend_mode,
                    )
                }
                TargetItem::Composite(c) => {
                    let d = &r.frame.offscreens[c.0 - 1];
                    let layout = self.layouts[c.0];
                    let parent = self.layouts[id.0];
                    let scale = Affine2 {
                        a: Vec2::new(layout.size.0 as f32, 0.),
                        b: Vec2::new(0., layout.size.1 as f32),
                        origin: Vec2::default(),
                    };
                    let transform = Affine2 {
                        origin: Vec2::new(
                            layout.screen_origin.x - parent.screen_origin.x,
                            layout.screen_origin.y - parent.screen_origin.y,
                        ),
                        ..scale
                    };
                    let mut u = Uniform::new(
                        target.size,
                        transform,
                        d.multiply_color,
                        d.screen_color,
                        d.opacity,
                    );
                    u.flags[1] = 1;
                    if let Some(mask) = self.masks.get(&d.id) {
                        u.mask(
                            mask,
                            compose_affine(inverse_affine(layout.transform)?, scale),
                            d.flags & 8 != 0,
                        );
                    }
                    (
                        &r.quad,
                        &child.as_ref().unwrap().view,
                        u,
                        false,
                        true,
                        BlendMode::Normal,
                        if d.blend_mode != 0 {
                            Some(d.blend_mode)
                        } else {
                            None
                        },
                    )
                }
            };
            let mode = FaceMode::for_draw(double_sided, &uniform);
            let pipeline = if let Some(raw) = raw {
                uniform.raw_blend(raw);
                r.pipelines.extended_draw.get(mode)
            } else {
                match blend {
                    BlendMode::Normal => r.pipelines.draw.get(mode),
                    BlendMode::Additive => {
                        uniform.blend_mode[0] = 1;
                        r.pipelines.additive.get(mode)
                    }
                    BlendMode::Multiplicative => {
                        uniform.blend_mode[0] = 2;
                        r.pipelines.multiplicative.get(mode)
                    }
                }
            };
            let mask_id = match item {
                TargetItem::Draw(i) => &r.frame.drawables[i.0].id,
                TargetItem::Composite(i) => &r.frame.offscreens[i.0 - 1].id,
            };
            let mask = self.masks.get(mask_id).map(|m| m.texture.view.clone());
            draws.push(self.draw_call(
                mesh,
                &uniform,
                source,
                mask.as_ref(),
                destination.as_ref().map(|t| &t.view),
                repeat,
                pipeline,
            ));
            // Any subsequent acquire flushes `draws` before writing to a pooled texture.
            if let Some(t) = destination {
                self.pool.push(t);
            }
            if let Some(t) = child {
                self.pool.push(t);
            }
        }
        self.flush(&target.view, &mut draws, &mut clear);
        if has_destination {
            self.stats.destination_targets += 1;
        }
        Ok(())
    }
}
