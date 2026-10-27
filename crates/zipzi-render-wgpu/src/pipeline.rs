use bytemuck::{Pod, Zeroable};
use zipzi_render::Affine2;

use crate::mask::MaskAttachment;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub(crate) struct Uniform {
    pub view_a: [f32; 4],
    pub view_b: [f32; 4],
    pub mask_a: [f32; 4],
    pub mask_b: [f32; 4],
    pub mask_region: [f32; 4],
    pub base_color: [f32; 4],
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
    pub opacity: [f32; 4],
    pub flags: [u32; 4],
    pub blend_mode: [u32; 4],
    pub alpha_mode: [u32; 4],
}

pub(crate) type UniformKey = [u8; std::mem::size_of::<Uniform>()];

impl Uniform {
    pub fn new(
        size: (u32, u32),
        view: Affine2,
        multiply_color: [f32; 4],
        screen_color: [f32; 4],
        opacity: f32,
    ) -> Self {
        let w = size.0.max(1) as f32;
        let h = size.1.max(1) as f32;
        Self {
            view_a: [
                2.0 * view.a.x / w,
                2.0 * view.b.x / w,
                0.0,
                2.0 * view.origin.x / w - 1.0,
            ],
            view_b: [
                -2.0 * view.a.y / h,
                -2.0 * view.b.y / h,
                0.0,
                1.0 - 2.0 * view.origin.y / h,
            ],
            mask_a: [1.0, 0.0, 0.0, 0.0],
            mask_b: [0.0, 1.0, 0.0, 0.0],
            mask_region: [0.0, 0.0, 1.0, 1.0],
            base_color: [size.0 as f32, size.1 as f32, 0.0, 0.0],
            multiply_color,
            screen_color,
            opacity: [opacity, 0.0, 0.0, 0.0],
            flags: [0; 4],
            blend_mode: [0; 4],
            alpha_mode: [0; 4],
        }
    }

    pub fn mask(&mut self, attachment: &MaskAttachment, mask_view: Affine2, inverted: bool) {
        let layout = attachment.layout;
        let lw = layout.logical_size.x.max(f32::EPSILON);
        let lh = layout.logical_size.y.max(f32::EPSILON);
        self.mask_a = [
            mask_view.a.x / lw,
            mask_view.b.x / lw,
            0.0,
            (mask_view.origin.x - layout.origin.x) / lw,
        ];
        self.mask_b = [
            mask_view.a.y / lh,
            mask_view.b.y / lh,
            0.0,
            (mask_view.origin.y - layout.origin.y) / lh,
        ];
        self.mask_region = attachment.region;
        self.flags[0] = 1;
        self.flags[2] = u32::from(inverted);
    }

    pub fn raw_blend(&mut self, raw: u32) {
        self.blend_mode[0] = raw & 0xff;
        self.alpha_mode[0] = (raw >> 8) & 0xff;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FaceMode {
    DoubleSided = 0,
    CullBackCcw = 1,
    CullBackCw = 2,
}

impl FaceMode {
    pub fn for_draw(double_sided: bool, uniform: &Uniform) -> Self {
        if double_sided {
            Self::DoubleSided
        } else {
            let determinant =
                uniform.view_a[0] * uniform.view_b[1] - uniform.view_a[1] * uniform.view_b[0];
            if determinant < 0.0 {
                Self::CullBackCw
            } else {
                Self::CullBackCcw
            }
        }
    }
}

pub(crate) struct PipelineSet([wgpu::RenderPipeline; 3]);

impl PipelineSet {
    pub fn get(&self, mode: FaceMode) -> &wgpu::RenderPipeline {
        &self.0[mode as usize]
    }
}

pub(crate) struct Pipelines {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub draw: PipelineSet,
    pub additive: PipelineSet,
    pub multiplicative: PipelineSet,
    pub extended_draw: PipelineSet,
    pub mask: PipelineSet,
    pub present: wgpu::RenderPipeline,
    pub present_replace: wgpu::RenderPipeline,
    pub linear_clamp: wgpu::Sampler,
    pub linear_repeat: wgpu::Sampler,
}

impl Pipelines {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zipzi.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("zipzi.wgsl").into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zipzi.bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(
                            std::mem::size_of::<Uniform>() as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("zipzi.pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let vertex_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: 8,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            },
            wgpu::VertexBufferLayout {
                array_stride: 8,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 1,
                }],
            },
        ];
        let build_one = |label: &str,
                         fragment_entry: &str,
                         format: wgpu::TextureFormat,
                         blend: Option<wgpu::BlendState>,
                         cull_mode: Option<wgpu::Face>,
                         front_face: wgpu::FrontFace| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_draw"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &vertex_buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face,
                    cull_mode,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let build_set = |label: &str,
                         fragment_entry: &str,
                         format: wgpu::TextureFormat,
                         blend: Option<wgpu::BlendState>| {
            PipelineSet([
                build_one(
                    label,
                    fragment_entry,
                    format,
                    blend,
                    None,
                    wgpu::FrontFace::Ccw,
                ),
                build_one(
                    label,
                    fragment_entry,
                    format,
                    blend,
                    Some(wgpu::Face::Back),
                    wgpu::FrontFace::Ccw,
                ),
                build_one(
                    label,
                    fragment_entry,
                    format,
                    blend,
                    Some(wgpu::Face::Back),
                    wgpu::FrontFace::Cw,
                ),
            ])
        };

        let normal_blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let additive_blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let multiplicative_blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });

        let draw = build_set("zipzi.draw", "fs_draw", color_format, normal_blend);
        let additive = build_set("zipzi.additive", "fs_draw", color_format, additive_blend);
        let multiplicative = build_set(
            "zipzi.multiplicative",
            "fs_draw",
            color_format,
            multiplicative_blend,
        );
        let extended_draw = build_set(
            "zipzi.extended_draw",
            "fs_extended_draw",
            color_format,
            None,
        );
        let mask = build_set(
            "zipzi.mask",
            "fs_mask",
            wgpu::TextureFormat::Rgba8Unorm,
            normal_blend,
        );
        let present = build_one(
            "zipzi.present",
            "fs_present",
            color_format,
            normal_blend,
            None,
            wgpu::FrontFace::Ccw,
        );
        let present_replace = build_one(
            "zipzi.present_replace",
            "fs_present",
            color_format,
            None,
            None,
            wgpu::FrontFace::Ccw,
        );

        let linear_clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zipzi.sampler.clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let linear_repeat = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zipzi.sampler.repeat"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        Self {
            bind_group_layout,
            draw,
            additive,
            multiplicative,
            extended_draw,
            mask,
            present,
            present_replace,
            linear_clamp,
            linear_repeat,
        }
    }
}
