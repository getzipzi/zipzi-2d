//! Worker-local GPU conversion and reusable staging for GPUI image presentation.
use anyhow::{Result, anyhow, ensure};
use std::sync::mpsc;
use std::time::Duration;
use zipzi_render_wgpu::WgpuContext;

pub struct WgpuImageReadback {
    pipeline: wgpu::ComputePipeline,
    buffers: Option<Buffers>,
}

struct Buffers {
    converted: wgpu::Buffer,
    staging: wgpu::Buffer,
    size: u64,
}

impl WgpuImageReadback {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zipzi.gpui_bgra"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_readback.wgsl").into()),
        });
        Self {
            pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("zipzi.gpui_bgra"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            }),
            buffers: None,
        }
    }

    /// Read linear RGBA8/BGRA8 into tightly packed, straight BGRA bytes.
    ///
    /// Conversion runs before readback on the GPU. Only one buffer pair is kept,
    /// replaced on size changes; returned CPU images own independent allocations.
    /// Calls must be serial on this context's worker. sRGB is deliberately rejected
    /// because textureLoad would decode it before the integer conversion.
    pub fn read(&mut self, context: &WgpuContext, texture: &wgpu::Texture) -> Result<Vec<u8>> {
        ensure!(
            matches!(
                texture.format(),
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
            ) && texture.dimension() == wgpu::TextureDimension::D2
                && texture.depth_or_array_layers() == 1
                && texture.sample_count() == 1
                && texture
                    .usage()
                    .contains(wgpu::TextureUsages::TEXTURE_BINDING),
            "GPUI readback requires a single-sample linear RGBA8/BGRA8 sampled texture"
        );
        let device = context.device();
        let size = u64::from(texture.width()) * u64::from(texture.height()) * 4;
        ensure!(
            size <= u64::from(device.limits().max_storage_buffer_binding_size)
                && size <= device.limits().max_buffer_size,
            "GPUI readback exceeds the device buffer limit"
        );
        if !self
            .buffers
            .as_ref()
            .is_some_and(|buffers| buffers.size == size)
        {
            self.buffers = Some(Buffers {
                converted: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("zipzi.gpui_bgra.converted"),
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                staging: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("zipzi.gpui_bgra.staging"),
                    size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                size,
            });
        }
        let buffers = self.buffers.as_ref().unwrap();
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            mip_level_count: Some(1),
            ..Default::default()
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zipzi.gpui_bgra"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.converted.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zipzi.gpui_bgra"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(texture.width().div_ceil(8), texture.height().div_ceil(8), 1);
        }
        // Packed storage avoids texture row padding and a second CPU swizzle pass.
        encoder.copy_buffer_to_buffer(&buffers.converted, 0, &buffers.staging, 0, size);
        let submission = context.queue().submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        buffers
            .staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        let completion = (|| -> Result<()> {
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(Duration::from_secs(30)),
                })
                .map_err(|error| anyhow!("GPU readback poll: {error}"))?;
            receiver.recv_timeout(Duration::from_secs(2))??;
            Ok(())
        })();
        if let Err(error) = completion {
            // Cancel a pending map too, so a failed read cannot poison reuse.
            buffers.staging.unmap();
            return Err(error);
        }
        let mapped = buffers.staging.slice(..).get_mapped_range();
        let pixels = mapped.to_vec();
        drop(mapped);
        buffers.staging.unmap();
        Ok(pixels)
    }
}
