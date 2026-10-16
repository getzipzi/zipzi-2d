//! Persistent scene, view, and geometry state for the Metal renderer.
use super::context::{validate_target, MAX_TEXTURE_DIMENSION};
use super::geometry::{pack_buffers, MeshBuffers};
use super::mask::{CachedMask, MaskCacheKey};
use super::pipeline::{sampler, Pipelines};
use super::{MetalContext, MetalTargetConfig, MetalTextureCatalog};
use kasane_core::{evaluation::DrawableFrame, Status, Vec2};
use kasane_render::gpu::{append_vertices_for, quad_indices, quad_vertices, Vertex};
use kasane_render::{surface_layout, Affine2, ScenePlan, ViewportConfig};
use metal::{CommandQueue, Device, SamplerState, Texture};
use std::{collections::HashMap, sync::Arc};

/// ScenePlan-driven native Metal renderer. It owns the Metal pipelines but
/// source textures and the presentation texture remain host owned.
pub struct MetalRenderer {
    pub(super) device: Device,
    pub(super) queue: CommandQueue,
    pub(super) target: MetalTargetConfig,
    pub(super) pipelines: Pipelines,
    pub(super) clamp_sampler: SamplerState,
    pub(super) repeat_sampler: SamplerState,
    pub(super) scene: ScenePlan,
    pub(super) frame: Option<Arc<DrawableFrame>>,
    pub(super) viewport: Option<ViewportConfig>,
    pub(super) meshes: Vec<MeshBuffers>,
    pub(super) quad: MeshBuffers,
    pub(super) next_geometry_revision: u64,
    pub(super) pending_uploads: usize,
    pub(super) mask_cache: HashMap<MaskCacheKey, CachedMask>,
    pub(super) mask_atlas_pages: Vec<Texture>,
}

impl MetalRenderer {
    pub fn new(context: &MetalContext, target: MetalTargetConfig) -> Result<Self, Status> {
        validate_target(target)?;
        let clamp_sampler = sampler(context.device(), false);
        let repeat_sampler = sampler(context.device(), true);
        Ok(Self {
            device: context.device().to_owned(),
            queue: context.queue().to_owned(),
            target,
            pipelines: Pipelines::new(context.device(), target.format)?,
            clamp_sampler,
            repeat_sampler,
            scene: ScenePlan::default(),
            frame: None,
            viewport: None,
            meshes: Vec::new(),
            quad: MeshBuffers::new(
                context.device(),
                quad_vertices((1, 1), Affine2::IDENTITY, Affine2::IDENTITY),
                quad_indices(),
                0,
            ),
            next_geometry_revision: 1,
            pending_uploads: 0,
            mask_cache: HashMap::new(),
            mask_atlas_pages: Vec::new(),
        })
    }

    pub fn target(&self) -> MetalTargetConfig {
        self.target
    }

    pub fn resize(&mut self, target: MetalTargetConfig) -> Result<(), Status> {
        validate_target(target)?;
        if self.target.format != target.format {
            self.pipelines = Pipelines::new(&self.device, target.format)?;
        }
        if self.target != target {
            self.viewport = None;
        }
        self.target = target;
        Ok(())
    }

    pub fn sync_model(
        &mut self,
        frame: &DrawableFrame,
        textures: &MetalTextureCatalog<'_>,
    ) -> Result<(), Status> {
        self.sync_model_shared(Arc::new(frame.clone()), textures)
    }

    /// Retain the host's immutable snapshot without copying all mesh positions.
    pub fn sync_model_shared(
        &mut self,
        frame: Arc<DrawableFrame>,
        textures: &MetalTextureCatalog<'_>,
    ) -> Result<(), Status> {
        let max_buffer = self.device.max_buffer_length();
        for drawable in &frame.drawables {
            let vertex_bytes = (drawable.positions.len() as u64)
                .saturating_mul(std::mem::size_of::<Vertex>() as u64);
            let index_bytes =
                (drawable.indices.len() as u64).saturating_mul(std::mem::size_of::<u32>() as u64);
            if vertex_bytes > max_buffer
                || index_bytes > max_buffer
                || drawable.indices.len() > u32::MAX as usize
            {
                return Err(Status::error(
                    "GPU_BUFFER_LIMIT",
                    format!("{} exceeds the Metal mesh buffer limit.", drawable.id),
                ));
            }
        }
        // ScenePlan finishes all fallible validation before changing its
        // published scene, so a successful update can reuse its allocations.
        self.scene.update(&frame, textures)?;
        let mut meshes = Vec::with_capacity(frame.drawables.len());
        let mut vertex_updates = Vec::new();
        let mut index_updates = Vec::new();
        let canvas_changed = self
            .frame
            .as_ref()
            .is_none_or(|previous| previous.canvas != frame.canvas);
        for (i, drawable) in frame.drawables.iter().enumerate() {
            let prior = self
                .frame
                .as_ref()
                .and_then(|previous| previous.drawables.get(i));
            if let Some(old) = self.meshes.get(i) {
                let vertex_changed = canvas_changed
                    || prior.is_none_or(|prior| {
                        prior.positions != drawable.positions || prior.uvs != drawable.uvs
                    });
                let index_changed = prior.is_none_or(|prior| prior.indices != drawable.indices);
                if !vertex_changed && !index_changed {
                    meshes.push(old.clone());
                    continue;
                }
                if vertex_changed {
                    vertex_updates.push(i);
                }
                if index_changed {
                    index_updates.push(i);
                }
                meshes.push(MeshBuffers {
                    vertex: if vertex_changed {
                        self.quad.vertex.clone() // Replaced by the packed upload below.
                    } else {
                        old.vertex.clone()
                    },
                    index: if index_changed {
                        self.quad.index.clone()
                    } else {
                        old.index.clone()
                    },
                    vertex_offset: old.vertex_offset,
                    index_offset: old.index_offset,
                    index_count: drawable.indices.len() as u64,
                    revision: self.next_geometry_revision,
                });
            } else {
                vertex_updates.push(i);
                index_updates.push(i);
                meshes.push(MeshBuffers {
                    vertex: self.quad.vertex.clone(),
                    index: self.quad.index.clone(),
                    vertex_offset: 0,
                    index_offset: 0,
                    index_count: drawable.indices.len() as u64,
                    revision: self.next_geometry_revision,
                });
            }
            self.next_geometry_revision += 1;
        }
        let (buffers, uploads) = pack_buffers(
            &self.device,
            &vertex_updates,
            |i| frame.drawables[i].positions.len(),
            |i, data| append_vertices_for(&frame.drawables[i], &frame, data),
        );
        self.pending_uploads += uploads;
        for (i, buffer, offset) in buffers {
            meshes[i].vertex = buffer;
            meshes[i].vertex_offset = offset;
        }
        let (buffers, uploads) = pack_buffers(
            &self.device,
            &index_updates,
            |i| frame.drawables[i].indices.len(),
            |i, data| {
                let (triangles, _) = frame.drawables[i].indices.as_chunks::<3>();
                data.extend(
                    triangles
                        .iter()
                        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]]),
                );
            },
        );
        self.pending_uploads += uploads;
        for (i, buffer, offset) in buffers {
            meshes[i].index = buffer;
            meshes[i].index_offset = offset;
        }
        self.meshes = meshes;
        self.frame = Some(frame);
        Ok(())
    }

    pub fn update_view(&mut self, viewport: ViewportConfig) -> Result<(), Status> {
        if self.frame.is_none() {
            return Err(Status::error(
                "MISSING_MODEL",
                "Submit a model before updating the view.",
            ));
        }
        if viewport.target_extent != Vec2::new(self.target.width as f32, self.target.height as f32)
        {
            return Err(Status::error(
                "INVALID_TARGET",
                "Viewport extent must match the output target.",
            ));
        }
        if !viewport.mask_scale.is_finite() || viewport.mask_scale <= 0.0 {
            return Err(Status::error(
                "INVALID_MASK_SCALE",
                "Mask scale must be positive and finite.",
            ));
        }
        let (size, _) = surface_layout(
            Vec2::new(self.scene.canvas().width, self.scene.canvas().height),
            viewport.transform,
            viewport.target_extent,
        )?;
        if size.width > MAX_TEXTURE_DIMENSION as i32 || size.height > MAX_TEXTURE_DIMENSION as i32 {
            return Err(Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "Offscreen surface exceeds Metal limits.",
            ));
        }
        self.viewport = Some(viewport);
        Ok(())
    }
}
