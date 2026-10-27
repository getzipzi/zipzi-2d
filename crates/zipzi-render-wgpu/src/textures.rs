use zipzi_core::evaluation::DrawableFrame;
use zipzi_core::types::Status;

use crate::api::WgpuTextureCatalog;
use crate::modern::WgpuRenderer;

impl WgpuRenderer {
    pub(crate) fn validate_frame_textures(
        &self,
        frame: &DrawableFrame,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<(), Status> {
        let limit = self.device.limits().max_texture_dimension_2d;
        for drawable in frame.drawables.iter().filter(|d| d.renderable) {
            let texture = textures
                .get(&drawable.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
            let raw = texture.view.texture();
            if texture.width == 0
                || texture.height == 0
                || texture.width > limit
                || texture.height > limit
                || raw.width() != texture.width
                || raw.height() != texture.height
                || raw.dimension() != wgpu::TextureDimension::D2
                || raw.depth_or_array_layers() != 1
                || raw.sample_count() != 1
                || !raw.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
                || !matches!(
                    raw.format(),
                    wgpu::TextureFormat::Rgba8Unorm
                        | wgpu::TextureFormat::Bgra8Unorm
                        | wgpu::TextureFormat::Rgba8UnormSrgb
                        | wgpu::TextureFormat::Bgra8UnormSrgb
                )
            {
                return Err(Status::error("INVALID_TEXTURE", &drawable.texture_asset_id));
            }
        }
        Ok(())
    }
}
