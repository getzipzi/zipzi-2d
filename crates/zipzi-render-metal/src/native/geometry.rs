//! Immutable mesh buffers and packed geometry uploads.
use metal::{Buffer, DeviceRef, MTLResourceOptions};
use zipzi_render::gpu::Vertex;

/// Immutable buffers can be retained by several submitted command buffers.
/// An edit replaces only changed buffers; it never overwrites in-flight data.
#[derive(Clone)]
pub(super) struct MeshBuffers {
    pub(super) vertex: Buffer,
    pub(super) index: Buffer,
    pub(super) vertex_offset: u64,
    pub(super) index_offset: u64,
    pub(super) index_count: u64,
    pub(super) revision: u64,
}

fn upload_buffer<T>(device: &DeviceRef, data: &[T]) -> Buffer {
    if data.is_empty() {
        device.new_buffer(4, MTLResourceOptions::StorageModeShared)
    } else {
        device.new_buffer_with_data(
            data.as_ptr().cast(),
            std::mem::size_of_val(data) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }
}

impl MeshBuffers {
    pub(super) fn new(
        device: &DeviceRef,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        revision: u64,
    ) -> Self {
        Self {
            vertex: upload_buffer(device, &vertices),
            index: upload_buffer(device, &indices),
            vertex_offset: 0,
            index_offset: 0,
            index_count: indices.len() as u64,
            revision,
        }
    }
}

/// Pack changed meshes into immutable slabs instead of allocating a Metal
/// resource per mesh. Limit slab size so a small retained mesh does not pin an
/// arbitrarily large allocation after subsequent partial edits.
pub(super) fn pack_buffers<T: Copy>(
    device: &DeviceRef,
    updates: &[usize],
    length: impl Fn(usize) -> usize,
    mut append: impl FnMut(usize, &mut Vec<T>),
) -> (Vec<(usize, Buffer, u64)>, usize) {
    let limit = device.max_buffer_length().min(4 * 1024 * 1024) as usize;
    let mut data = Vec::<T>::new();
    let mut offsets = Vec::new();
    let mut packed = Vec::new();
    let mut uploads = 0;
    let mut flush = |data: &mut Vec<T>, offsets: &mut Vec<(usize, u64)>| {
        if offsets.is_empty() {
            return;
        }
        let buffer = upload_buffer(device, data);
        uploads += 1;
        for (index, offset) in offsets.drain(..) {
            packed.push((index, buffer.clone(), offset));
        }
        data.clear();
    };
    for &index in updates {
        if (data.len() + length(index)) * std::mem::size_of::<T>() > limit {
            flush(&mut data, &mut offsets);
        }
        offsets.push((index, (data.len() * std::mem::size_of::<T>()) as u64));
        append(index, &mut data);
    }
    flush(&mut data, &mut offsets);
    (packed, uploads)
}
