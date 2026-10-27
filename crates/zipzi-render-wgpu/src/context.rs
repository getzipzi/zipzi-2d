use std::future::Future;
use std::sync::mpsc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use zipzi_core::types::Status;
use zipzi_render::texture::premultiply_rgba8;

use crate::modern::validate_target;
use crate::WgpuTargetConfig;

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub struct WgpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    adapter_backend: String,
}

impl WgpuContext {
    pub fn new() -> Result<Self, Status> {
        let instance = wgpu::Instance::default();
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|error| Status::error("GPU_ADAPTER", error.to_string()))?;
        let info = adapter.get_info();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|error| Status::error("GPU_DEVICE", error.to_string()))?;
        Ok(Self {
            device,
            queue,
            adapter_name: info.name,
            adapter_backend: format!("wgpu ({:?})", info.backend),
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub fn adapter_backend(&self) -> &str {
        &self.adapter_backend
    }

    pub fn upload_rgba8(
        &self,
        width: u32,
        height: u32,
        rgba: &[u8],
        mipmaps: &[(u32, u32, Vec<u8>)],
    ) -> Result<wgpu::Texture, Status> {
        upload_rgba8(&self.device, &self.queue, width, height, rgba, mipmaps)
    }

    pub fn output_texture(&self, target: WgpuTargetConfig) -> Result<wgpu::Texture, Status> {
        output_texture(&self.device, target)
    }

    pub fn read_rgba8(&self, texture: &wgpu::Texture) -> Result<Vec<u8>, Status> {
        read_rgba8(&self.device, &self.queue, texture)
    }
}

pub fn upload_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
    mipmaps: &[(u32, u32, Vec<u8>)],
) -> Result<wgpu::Texture, Status> {
    let limit = device.limits().max_texture_dimension_2d;
    if width == 0
        || height == 0
        || width > limit
        || height > limit
        || rgba.len() != width as usize * height as usize * 4
    {
        return Err(Status::error(
            "INVALID_TEXTURE",
            "Invalid RGBA8 texture dimensions or bytes.",
        ));
    }
    let (mut expected_width, mut expected_height) = (width, height);
    for (w, h, bytes) in mipmaps {
        expected_width = (expected_width / 2).max(1);
        expected_height = (expected_height / 2).max(1);
        if (*w, *h) != (expected_width, expected_height)
            || bytes.len() != *w as usize * *h as usize * 4
        {
            return Err(Status::error("INVALID_TEXTURE", "Invalid RGBA8 mip level."));
        }
    }
    let max_mip_levels = 32 - width.max(height).leading_zeros();
    if mipmaps.len() >= max_mip_levels as usize {
        return Err(Status::error(
            "INVALID_TEXTURE",
            "Too many RGBA8 mip levels.",
        ));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("zipzi.source"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1 + mipmaps.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let write_level = |level: u32, w: u32, h: u32, bytes: &[u8]| {
        let premultiplied = premultiply_rgba8(bytes);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &premultiplied,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    };
    write_level(0, width, height, rgba);
    for (index, (w, h, bytes)) in mipmaps.iter().enumerate() {
        write_level(index as u32 + 1, *w, *h, bytes);
    }
    Ok(texture)
}

pub fn output_texture(
    device: &wgpu::Device,
    target: WgpuTargetConfig,
) -> Result<wgpu::Texture, Status> {
    validate_target(device, target)?;
    Ok(device.create_texture(&wgpu::TextureDescriptor {
        label: Some("zipzi.output"),
        size: wgpu::Extent3d {
            width: target.width,
            height: target.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }))
}

pub fn read_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Vec<u8>, Status> {
    let format = texture.format();
    if !matches!(
        format,
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8UnormSrgb
    ) || texture.dimension() != wgpu::TextureDimension::D2
        || texture.depth_or_array_layers() != 1
        || texture.sample_count() != 1
        || !texture.usage().contains(wgpu::TextureUsages::COPY_SRC)
    {
        return Err(Status::error(
            "INVALID_READBACK",
            "Readback requires a single-sample 2D RGBA8/BGRA8 COPY_SRC texture.",
        ));
    }
    let extent = texture.size();
    let width = extent.width;
    let height = extent.height;
    let row_bytes = width * 4;
    let padded_row = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("zipzi.readback"),
        size: u64::from(padded_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("zipzi.readback.encoder"),
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map_err(|error| Status::error("GPU_POLL", error.to_string()))?;
    receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|error| Status::error("GPU_READBACK", error.to_string()))?
        .map_err(|error| Status::error("GPU_READBACK", error.to_string()))?;
    let mapped = buffer.slice(..).get_mapped_range();
    let mut pixels = Vec::with_capacity((row_bytes * height) as usize);
    for row in mapped.chunks_exact(padded_row as usize) {
        pixels.extend_from_slice(&row[..row_bytes as usize]);
    }
    drop(mapped);
    buffer.unmap();
    if matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }
    Ok(pixels)
}
