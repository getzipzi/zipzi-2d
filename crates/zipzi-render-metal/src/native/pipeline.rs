//! Metal shader ABI, pipeline compilation, and sampler state.
use super::mask::MaskAttachment;
use bytemuck::{Pod, Zeroable};
use metal::{
    DeviceRef, MTLBlendFactor, MTLPixelFormat, MTLSamplerAddressMode, MTLSamplerMinMagFilter,
    MTLSamplerMipFilter, RenderPipelineDescriptor, RenderPipelineState, SamplerDescriptor,
    SamplerState,
};
use zipzi_core::Status;
use zipzi_render::Affine2;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Uniform {
    pub(super) target_size: [f32; 4],
    pub(super) multiply_color: [f32; 4],
    pub(super) screen_color: [f32; 4],
    pub(super) mask_bounds: [f32; 4],
    pub(super) opacity: [f32; 4],
    pub(super) flags: [u32; 4],
    pub(super) modes: [u32; 4],
    pub(super) view_a: [f32; 4],
    pub(super) view_b: [f32; 4],
    pub(super) view_origin: [f32; 4],
    pub(super) mask_a: [f32; 4],
    pub(super) mask_b: [f32; 4],
    pub(super) mask_origin: [f32; 4],
    pub(super) mask_region: [f32; 4],
}

impl Uniform {
    pub(super) fn new(
        size: (u32, u32),
        multiply_color: [f32; 4],
        screen_color: [f32; 4],
        opacity: f32,
    ) -> Self {
        Self {
            target_size: [size.0 as f32, size.1 as f32, 0.0, 0.0],
            multiply_color,
            screen_color,
            mask_bounds: [0.0; 4],
            opacity: [opacity, 0.0, 0.0, 0.0],
            flags: [0; 4],
            modes: [0; 4],
            view_a: [1.0, 0.0, 0.0, 0.0],
            view_b: [0.0, 1.0, 0.0, 0.0],
            view_origin: [0.0; 4],
            mask_a: [1.0, 0.0, 0.0, 0.0],
            mask_b: [0.0, 1.0, 0.0, 0.0],
            mask_origin: [0.0; 4],
            mask_region: [0.0, 0.0, 1.0, 1.0],
        }
    }

    pub(super) fn view(&mut self, value: Affine2) {
        self.view_a = [value.a.x, value.a.y, 0.0, 0.0];
        self.view_b = [value.b.x, value.b.y, 0.0, 0.0];
        self.view_origin = [value.origin.x, value.origin.y, 0.0, 0.0];
    }

    pub(super) fn mask_view(&mut self, value: Affine2) {
        self.mask_a = [value.a.x, value.a.y, 0.0, 0.0];
        self.mask_b = [value.b.x, value.b.y, 0.0, 0.0];
        self.mask_origin = [value.origin.x, value.origin.y, 0.0, 0.0];
    }

    pub(super) fn mask(&mut self, attachment: &MaskAttachment, inverted: bool) {
        let layout = attachment.layout;
        self.mask_region = attachment.region;
        self.mask_bounds = [
            layout.origin.x,
            layout.origin.y,
            layout.logical_size.x,
            layout.logical_size.y,
        ];
        self.flags[0] = 1;
        self.flags[1] = u32::from(inverted);
    }

    pub(super) fn raw_blend(&mut self, mode: u32) {
        self.modes = [mode & 0xff, (mode >> 8) & 0xff, 0, 0];
    }
}

#[derive(Clone, Copy)]
enum Blend {
    Replace,
    Normal,
    Additive,
    Multiplicative,
}

pub(super) struct Pipelines {
    pub(super) draw: RenderPipelineState,
    pub(super) additive: RenderPipelineState,
    pub(super) multiplicative: RenderPipelineState,
    pub(super) composite: RenderPipelineState,
    pub(super) mask: RenderPipelineState,
    pub(super) extended_draw: RenderPipelineState,
    pub(super) extended_composite: RenderPipelineState,
    pub(super) present_composite: RenderPipelineState,
}

impl Pipelines {
    pub(super) fn new(device: &DeviceRef, format: MTLPixelFormat) -> Result<Self, Status> {
        let library = device
            .new_library_with_source(
                include_str!("../zipzi.metal"),
                &metal::CompileOptions::new(),
            )
            .map_err(|message| Status::error("METAL_SHADER", message))?;
        let build = |fragment: &str, target_format, blend| {
            pipeline(device, &library, fragment, target_format, blend)
        };
        Ok(Self {
            draw: build("fs_draw", format, Blend::Normal)?,
            additive: build("fs_draw", format, Blend::Additive)?,
            multiplicative: build("fs_draw", format, Blend::Multiplicative)?,
            composite: build("fs_composite", format, Blend::Normal)?,
            mask: build("fs_mask", MTLPixelFormat::RGBA8Unorm, Blend::Normal)?,
            extended_draw: build("fs_extended_draw", format, Blend::Replace)?,
            extended_composite: build("fs_extended_composite", format, Blend::Replace)?,
            present_composite: build("fs_present", format, Blend::Normal)?,
        })
    }
}

fn pipeline(
    device: &DeviceRef,
    library: &metal::LibraryRef,
    fragment: &str,
    format: MTLPixelFormat,
    blend: Blend,
) -> Result<RenderPipelineState, Status> {
    let vertex = library
        .get_function("vs_main", None)
        .map_err(|message| Status::error("METAL_SHADER", message))?;
    let fragment = library
        .get_function(fragment, None)
        .map_err(|message| Status::error("METAL_SHADER", message))?;
    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_vertex_function(Some(&vertex));
    descriptor.set_fragment_function(Some(&fragment));
    let color = descriptor.color_attachments().object_at(0).unwrap();
    color.set_pixel_format(format);
    if !matches!(blend, Blend::Replace) {
        color.set_blending_enabled(true);
        let (src_rgb, dst_rgb, src_alpha, dst_alpha) = match blend {
            Blend::Normal => (
                MTLBlendFactor::One,
                MTLBlendFactor::OneMinusSourceAlpha,
                MTLBlendFactor::One,
                MTLBlendFactor::OneMinusSourceAlpha,
            ),
            Blend::Additive => (
                MTLBlendFactor::One,
                MTLBlendFactor::One,
                MTLBlendFactor::Zero,
                MTLBlendFactor::One,
            ),
            Blend::Multiplicative => (
                MTLBlendFactor::Zero,
                MTLBlendFactor::SourceColor,
                MTLBlendFactor::Zero,
                MTLBlendFactor::One,
            ),
            Blend::Replace => unreachable!(),
        };
        color.set_source_rgb_blend_factor(src_rgb);
        color.set_destination_rgb_blend_factor(dst_rgb);
        color.set_source_alpha_blend_factor(src_alpha);
        color.set_destination_alpha_blend_factor(dst_alpha);
    }
    device
        .new_render_pipeline_state(&descriptor)
        .map_err(|message| Status::error("METAL_PIPELINE", message))
}

pub(super) fn sampler(device: &DeviceRef, repeat: bool) -> SamplerState {
    let descriptor = SamplerDescriptor::new();
    descriptor.set_min_filter(MTLSamplerMinMagFilter::Linear);
    descriptor.set_mag_filter(MTLSamplerMinMagFilter::Linear);
    descriptor.set_mip_filter(MTLSamplerMipFilter::Linear);
    let address = if repeat {
        MTLSamplerAddressMode::Repeat
    } else {
        MTLSamplerAddressMode::ClampToEdge
    };
    descriptor.set_address_mode_s(address);
    descriptor.set_address_mode_t(address);
    device.new_sampler(&descriptor)
}
