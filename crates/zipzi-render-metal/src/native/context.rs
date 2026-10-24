//! Device setup, texture allocation, target validation, and readback.
use super::MetalTargetConfig;
use metal::{
    CommandQueue, Device, DeviceRef, MTLPixelFormat, MTLRegion, MTLStorageMode, MTLTextureUsage,
    Texture, TextureDescriptor, TextureRef,
};
use zipzi_core::Status;

pub(super) const MAX_TEXTURE_DIMENSION: u32 = 16_384;

#[derive(Debug, thiserror::Error)]
pub enum MetalInitError {
    #[error("no Metal device is available")]
    DeviceUnavailable,
}

/// A native Metal device and command queue. No wgpu adapter or device is made.
pub struct MetalContext {
    device: Device,
    queue: CommandQueue,
}

impl MetalContext {
    pub fn new() -> Result<Self, MetalInitError> {
        let device = Device::system_default().ok_or(MetalInitError::DeviceUnavailable)?;
        Ok(Self::from_device(device))
    }

    /// Use an existing host `MTLDevice`, such as the device assigned to a
    /// `CAMetalLayer`, while creating a dedicated command queue for Zipzi.
    pub fn from_device(device: Device) -> Self {
        let queue = device.new_command_queue();
        Self { device, queue }
    }

    pub fn device(&self) -> &DeviceRef {
        &self.device
    }

    pub fn queue(&self) -> &metal::CommandQueueRef {
        &self.queue
    }

    pub fn name(&self) -> &str {
        self.device.name()
    }

    pub fn max_texture_dimension(&self) -> u32 {
        MAX_TEXTURE_DIMENSION
    }

    /// Upload straight RGBA8 pixels and straight RGBA mip levels. GPU storage is
    /// premultiplied so linear/trilinear filtering cannot bleed invisible RGB.
    pub fn upload_rgba8(
        &self,
        width: u32,
        height: u32,
        rgba: &[u8],
        mipmaps: &[(u32, u32, Vec<u8>)],
    ) -> Result<Texture, Status> {
        if width == 0
            || height == 0
            || width > MAX_TEXTURE_DIMENSION
            || height > MAX_TEXTURE_DIMENSION
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
        let descriptor = texture_descriptor(
            width,
            height,
            MTLPixelFormat::RGBA8Unorm,
            MTLTextureUsage::ShaderRead,
        );
        descriptor.set_mipmap_level_count(1 + mipmaps.len() as u64);
        let texture = self.device.new_texture(&descriptor);
        let premultiplied = zipzi_render::texture::premultiply_rgba8(rgba);
        texture.replace_region(
            MTLRegion::new_2d(0, 0, width as u64, height as u64),
            0,
            premultiplied.as_ptr().cast(),
            (width * 4) as u64,
        );
        for (index, (w, h, bytes)) in mipmaps.iter().enumerate() {
            let premultiplied = zipzi_render::texture::premultiply_rgba8(bytes);
            texture.replace_region(
                MTLRegion::new_2d(0, 0, *w as u64, *h as u64),
                (index + 1) as u64,
                premultiplied.as_ptr().cast(),
                (*w * 4) as u64,
            );
        }
        Ok(texture)
    }

    pub fn output_texture(&self, target: MetalTargetConfig) -> Result<Texture, Status> {
        validate_target(target)?;
        Ok(create_shared_texture(
            &self.device,
            target.width,
            target.height,
            target.format,
        ))
    }
}

pub(super) fn texture_descriptor(
    width: u32,
    height: u32,
    format: MTLPixelFormat,
    usage: MTLTextureUsage,
) -> TextureDescriptor {
    let descriptor = TextureDescriptor::new();
    descriptor.set_texture_type(metal::MTLTextureType::D2);
    descriptor.set_width(width as u64);
    descriptor.set_height(height as u64);
    descriptor.set_pixel_format(format);
    descriptor.set_storage_mode(MTLStorageMode::Shared);
    descriptor.set_usage(usage);
    descriptor
}

pub(super) fn create_texture(
    device: &DeviceRef,
    width: u32,
    height: u32,
    format: MTLPixelFormat,
) -> Texture {
    let descriptor = texture_descriptor(
        width,
        height,
        format,
        MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
    );
    descriptor.set_storage_mode(MTLStorageMode::Private);
    device.new_texture(&descriptor)
}

pub(super) fn create_shared_texture(
    device: &DeviceRef,
    width: u32,
    height: u32,
    format: MTLPixelFormat,
) -> Texture {
    let descriptor = texture_descriptor(
        width,
        height,
        format,
        MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
    );
    device.new_texture(&descriptor)
}

pub(super) fn validate_target(target: MetalTargetConfig) -> Result<(), Status> {
    if target.width == 0
        || target.height == 0
        || target.width > MAX_TEXTURE_DIMENSION
        || target.height > MAX_TEXTURE_DIMENSION
    {
        return Err(Status::error(
            "INVALID_TARGET",
            "Output extent exceeds Metal limits.",
        ));
    }
    if !matches!(
        target.format,
        MTLPixelFormat::RGBA8Unorm
            | MTLPixelFormat::RGBA8Unorm_sRGB
            | MTLPixelFormat::BGRA8Unorm
            | MTLPixelFormat::BGRA8Unorm_sRGB
    ) {
        return Err(Status::error(
            "INVALID_TARGET_FORMAT",
            "Only RGBA8 and BGRA8 color targets are supported.",
        ));
    }
    Ok(())
}

/// Read an RGBA8 shared Metal texture after its command buffer completes.
pub fn read_rgba8(texture: &TextureRef) -> Result<Vec<u8>, Status> {
    if texture.pixel_format() != MTLPixelFormat::RGBA8Unorm
        || texture.storage_mode() != MTLStorageMode::Shared
        || texture.texture_type() != metal::MTLTextureType::D2
        || texture.sample_count() != 1
    {
        return Err(Status::error(
            "INVALID_READBACK",
            "Readback requires a shared, single-sample 2D RGBA8Unorm texture.",
        ));
    }
    let width = texture.width() as usize;
    let height = texture.height() as usize;
    let mut rgba = vec![0u8; width * height * 4];
    texture.get_bytes(
        rgba.as_mut_ptr().cast(),
        (width * 4) as u64,
        MTLRegion::new_2d(0, 0, width as u64, height as u64),
        0,
    );
    Ok(rgba)
}
