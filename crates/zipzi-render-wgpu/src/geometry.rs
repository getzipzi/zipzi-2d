use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;
use zipzi_core::evaluation::DrawableFrame;
use zipzi_core::types::{Status, Vec2};

use crate::api::WgpuTextureCatalog;
use crate::modern::WgpuRenderer;

const SLAB_UPLOAD_CAP: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct MeshBuffers {
    pub positions: wgpu::Buffer,
    pub position_offset: u64,
    pub position_bytes: u64,
    pub uvs: wgpu::Buffer,
    pub uv_offset: u64,
    pub uv_bytes: u64,
    pub indices: wgpu::Buffer,
    pub index_offset: u64,
    pub index_bytes: u64,
    pub index_count: u32,
    pub revision: u64,
}

impl MeshBuffers {
    pub fn quad(device: &wgpu::Device) -> Self {
        let positions: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let uvs: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];
        let pos_bytes = bytemuck::cast_slice(&positions);
        let uv_bytes = bytemuck::cast_slice(&uvs);
        let idx_bytes = bytemuck::cast_slice(&indices);
        Self {
            positions: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("zipzi.quad.positions"),
                contents: pos_bytes,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            position_offset: 0,
            position_bytes: pos_bytes.len() as u64,
            uvs: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("zipzi.quad.uvs"),
                contents: uv_bytes,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            uv_offset: 0,
            uv_bytes: uv_bytes.len() as u64,
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("zipzi.quad.indices"),
                contents: idx_bytes,
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_offset: 0,
            index_bytes: idx_bytes.len() as u64,
            index_count: indices.len() as u32,
            revision: 0,
        }
    }
}

struct GeometrySlab {
    buffer: wgpu::Buffer,
    offsets: Vec<(usize, u64, u64)>,
}

#[derive(Clone, Copy)]
enum GeometryData<'a> {
    Points(&'a [Vec2]),
    Indices(&'a [u32]),
}
impl GeometryData<'_> {
    fn len(self) -> usize {
        match self {
            Self::Points(v) => v.len() * 8,
            Self::Indices(v) => v.len() * 4,
        }
    }
    fn append(self, output: &mut Vec<u8>) {
        match self {
            Self::Points(values) => {
                // Vec2 has no stable repr/Pod contract; the GPU expects XY float32.
                for value in values {
                    output.extend_from_slice(&value.x.to_ne_bytes());
                    output.extend_from_slice(&value.y.to_ne_bytes());
                }
            }
            Self::Indices(values) => output.extend_from_slice(bytemuck::cast_slice(values)),
        }
    }
}

fn pack_buffers<'a>(
    device: &wgpu::Device,
    usage: wgpu::BufferUsages,
    label: &str,
    items: impl IntoIterator<Item = (usize, GeometryData<'a>)>,
) -> Vec<GeometrySlab> {
    let limit = (device.limits().max_buffer_size as usize).min(SLAB_UPLOAD_CAP);
    let mut slabs = Vec::new();
    let mut batch: Vec<(usize, GeometryData<'a>)> = Vec::new();
    let mut total = 0usize;
    let flush = |batch: &mut Vec<(usize, GeometryData<'a>)>,
                 total: &mut usize,
                 slabs: &mut Vec<GeometrySlab>| {
        if batch.is_empty() {
            return;
        }
        let buffer_size = (*total).max(4);
        let mut contents = Vec::with_capacity(buffer_size);
        let mut offsets = Vec::with_capacity(batch.len());
        for (index, bytes) in batch.drain(..) {
            let offset = contents.len() as u64;
            bytes.append(&mut contents);
            offsets.push((index, offset, bytes.len() as u64));
        }
        if contents.len() < 4 {
            contents.resize(4, 0);
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: &contents,
            usage,
        });
        *total = 0;
        slabs.push(GeometrySlab { buffer, offsets });
    };
    for (index, bytes) in items {
        if !batch.is_empty() && total + bytes.len() > limit {
            flush(&mut batch, &mut total, &mut slabs);
        }
        total += bytes.len();
        batch.push((index, bytes));
    }
    flush(&mut batch, &mut total, &mut slabs);
    slabs
}

impl WgpuRenderer {
    pub fn sync_model(
        &mut self,
        device: &wgpu::Device,
        frame: &DrawableFrame,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<(), Status> {
        self.sync_model_shared(device, Arc::new(frame.clone()), textures)
    }

    pub fn sync_model_shared(
        &mut self,
        device: &wgpu::Device,
        frame: Arc<DrawableFrame>,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<(), Status> {
        self.ensure_device(device)?;
        self.validate_frame_textures(&frame, textures)?;
        let max_buffer = device.limits().max_buffer_size;
        for drawable in &frame.drawables {
            let vertex_bytes = (drawable.positions.len() as u64).saturating_mul(8);
            let index_bytes =
                (drawable.indices.len() as u64).saturating_mul(std::mem::size_of::<u32>() as u64);
            if vertex_bytes > max_buffer
                || index_bytes > max_buffer
                || drawable.indices.len() > u32::MAX as usize
            {
                return Err(Status::error(
                    "GPU_BUFFER_LIMIT",
                    format!("{} exceeds the wgpu mesh buffer limit.", drawable.id),
                ));
            }
        }
        self.scene.update(&frame, textures)?;
        let mut previous_map = self
            .frame
            .drawables
            .iter()
            .zip(std::mem::take(&mut self.geometry))
            .filter_map(|(drawable, buffers)| Some((drawable.id.as_str(), (drawable, buffers?))))
            .collect::<HashMap<_, _>>();
        let mut synced = Vec::with_capacity(frame.drawables.len());
        let mut vertex_bytes_uploaded = 0u64;
        let mut index_bytes_uploaded = 0u64;
        let mut position_uploads = Vec::new();
        let mut uv_uploads = Vec::new();
        let mut index_uploads = Vec::new();
        let mut pending = Vec::new();
        for (slot, drawable) in frame.drawables.iter().enumerate() {
            if drawable.indices.is_empty() || drawable.positions.is_empty() {
                synced.push(None);
                continue;
            }
            let previous = previous_map.remove(drawable.id.as_str());
            let positions_same = previous
                .as_ref()
                .is_some_and(|(prev, _)| prev.positions == drawable.positions);
            let uvs_same = previous
                .as_ref()
                .is_some_and(|(prev, _)| prev.uvs == drawable.uvs);
            let indices_same = previous
                .as_ref()
                .is_some_and(|(prev, _)| prev.indices == drawable.indices);
            if positions_same && uvs_same && indices_same {
                let mut buffers = previous.unwrap().1;
                buffers.index_count = drawable.indices.len() as u32;
                synced.push(Some(buffers));
                continue;
            }
            let revision = self.next_geometry_revision;
            self.next_geometry_revision += 1;
            if !positions_same {
                let bytes = GeometryData::Points(&drawable.positions);
                vertex_bytes_uploaded += bytes.len() as u64;
                position_uploads.push((slot, bytes));
            }
            if !uvs_same {
                let bytes = GeometryData::Points(&drawable.uvs);
                vertex_bytes_uploaded += bytes.len() as u64;
                uv_uploads.push((slot, bytes));
            }
            if !indices_same {
                let bytes = GeometryData::Indices(&drawable.indices);
                index_bytes_uploaded += bytes.len() as u64;
                index_uploads.push((slot, bytes));
            }
            pending.push((
                slot,
                previous.map(|(_, buffers)| buffers),
                drawable.indices.len() as u32,
                revision,
            ));
            synced.push(None);
        }
        let pos_slabs = pack_buffers(
            device,
            wgpu::BufferUsages::VERTEX,
            "zipzi.positions",
            position_uploads,
        );
        let uv_slabs = pack_buffers(device, wgpu::BufferUsages::VERTEX, "zipzi.uvs", uv_uploads);
        let idx_slabs = pack_buffers(
            device,
            wgpu::BufferUsages::INDEX,
            "zipzi.indices",
            index_uploads,
        );
        let creations = (pos_slabs.len() + uv_slabs.len() + idx_slabs.len()) as u64;
        let mut positions = HashMap::new();
        for slab in pos_slabs {
            for (slot, offset, bytes) in slab.offsets {
                positions.insert(slot, (slab.buffer.clone(), offset, bytes));
            }
        }
        let mut uvs = HashMap::new();
        for slab in uv_slabs {
            for (slot, offset, bytes) in slab.offsets {
                uvs.insert(slot, (slab.buffer.clone(), offset, bytes));
            }
        }
        let mut indices = HashMap::new();
        for slab in idx_slabs {
            for (slot, offset, bytes) in slab.offsets {
                indices.insert(slot, (slab.buffer.clone(), offset, bytes));
            }
        }
        for (slot, previous, index_count, revision) in pending {
            let (pos_buf, position_offset, position_bytes) =
                positions.remove(&slot).unwrap_or_else(|| {
                    let prev = previous.as_ref().unwrap();
                    (
                        prev.positions.clone(),
                        prev.position_offset,
                        prev.position_bytes,
                    )
                });
            let (uv_buf, uv_offset, uv_bytes) = uvs.remove(&slot).unwrap_or_else(|| {
                let prev = previous.as_ref().unwrap();
                (prev.uvs.clone(), prev.uv_offset, prev.uv_bytes)
            });
            let (idx_buf, index_offset, index_bytes) = indices.remove(&slot).unwrap_or_else(|| {
                let prev = previous.as_ref().unwrap();
                (prev.indices.clone(), prev.index_offset, prev.index_bytes)
            });
            synced[slot] = Some(MeshBuffers {
                positions: pos_buf,
                position_offset,
                position_bytes,
                uvs: uv_buf,
                uv_offset,
                uv_bytes,
                indices: idx_buf,
                index_offset,
                index_bytes,
                index_count,
                revision,
            });
        }
        self.geometry = synced;
        self.frame = frame;
        self.pending_uploads += creations;
        self.pending_vertex_bytes += vertex_bytes_uploaded;
        self.pending_index_bytes += index_bytes_uploaded;
        Ok(())
    }
}
