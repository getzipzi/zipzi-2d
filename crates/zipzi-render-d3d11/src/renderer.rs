use anyhow::{ensure, Context, Result};
use std::sync::Arc;
use windows::{
    core::s,
    Win32::Graphics::{Direct3D11::*, Dxgi::Common::*},
};
use zipzi_core::DrawableFrame;
use zipzi_render::{
    gpu::{fit_mask_budget, inverse_affine},
    ScenePlan, ViewportConfig,
};

use crate::{
    encoding::{ColorPlan, ConstantBuffers, MaskPlan},
    resources::{buffer, bytes, dynamic_buffer, MAX_TEXTURE_DIMENSION},
    D3d11Target, D3d11TextureCatalog, D3d11ValidationError,
};

/// Records on a private deferred context. All lists from this renderer must
/// execute on the immediate context of its device.
pub struct D3d11Renderer {
    pub(crate) device: ID3D11Device,
    pub(crate) deferred: ID3D11DeviceContext,
    pub(crate) input: ID3D11InputLayout,
    pub(crate) vertex: ID3D11VertexShader,
    pub(crate) pixel: ID3D11PixelShader,
    pub(crate) mask_pixel: ID3D11PixelShader,
    pub(crate) raw_pixel: ID3D11PixelShader,
    pub(crate) blend: [ID3D11BlendState; 3],
    pub(crate) raster: [ID3D11RasterizerState; 3],
    pub(crate) samplers: [Option<ID3D11SamplerState>; 2],
    pub(crate) frame: Option<Arc<DrawableFrame>>,
    pub(crate) plan: ScenePlan,
    pub(crate) geometry: Option<Geometry>,
    constants: ConstantBuffers,
    pub(crate) masks: Vec<D3d11Target>,
    pub(crate) colors: Vec<D3d11Target>,
}

#[derive(Clone, Copy)]
pub(crate) struct DrawRange {
    pub(crate) count: u32,
    pub(crate) first_index: u32,
    pub(crate) base_vertex: i32,
}

pub(crate) struct Geometry {
    pub(crate) vertex: ID3D11Buffer,
    pub(crate) index: ID3D11Buffer,
    pub(crate) ranges: Vec<DrawRange>,
    pub(crate) vertices: Vec<[f32; 4]>,
    topology: Vec<Option<(usize, Arc<[u32]>)>>,
}

impl D3d11Renderer {
    pub fn clear_model(&mut self) {
        self.frame = None;
        self.geometry = None;
        self.constants.clear();
        self.plan = ScenePlan::default();
        self.masks.clear();
        self.colors.clear();
    }

    pub fn record(
        &mut self,
        output: &D3d11Target,
        frame: Arc<DrawableFrame>,
        view: ViewportConfig,
        textures: &D3d11TextureCatalog<'_>,
        mut boundary: impl FnMut(&'static str),
    ) -> Result<ID3D11CommandList> {
        ensure!(
            unsafe { output.texture.GetDevice()? } == self.device,
            "D3D11 output belongs to a different device"
        );
        ensure!(
            output.extent.map(|v| v as f32) == [view.target_extent.x, view.target_extent.y],
            "Viewport and D3D11 output extent differ"
        );
        unsafe { self.device.GetDeviceRemovedReason()? };
        textures.validate_device(&self.device)?;
        inverse_affine(view.transform).map_err(status)?;
        if self
            .frame
            .as_ref()
            .is_none_or(|previous| !Arc::ptr_eq(previous, &frame))
        {
            self.frame = None;
            self.plan.update(&frame, textures).map_err(status)?;
            boundary("native_d3d11_plan_ready");
            match self.geometry.as_mut() {
                Some(geometry) if geometry.matches(&frame) => geometry.update_vertices(&frame),
                _ => self.geometry = Some(Geometry::new(&self.device, &frame)?),
            }
            self.frame = Some(frame.clone());
        }
        boundary("model_synced");
        boundary("encode_begin");
        let color = ColorPlan::new(&self.plan, &frame, view, output.extent).map_err(status)?;
        let color_bytes =
            bytes(output.extent) + color.sizes.iter().copied().map(bytes).sum::<u64>();
        if color_bytes > zipzi_render::OFFSCREEN_BUDGET_BYTES as u64 {
            return Err(status(zipzi_core::Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "D3D11 color attachments exceed the budget",
            )));
        }
        let mut masks = MaskPlan::default();
        fit_mask_budget(view, MAX_TEXTURE_DIMENSION, |view, visible| {
            masks = MaskPlan::new(&self.plan, &frame, view.mask_scale, visible)?;
            let bytes = color_bytes + masks.extents().map(bytes).sum::<u64>();
            if bytes > zipzi_render::OFFSCREEN_BUDGET_BYTES as u64 {
                return Err(zipzi_core::Status::error(
                    "OFFSCREEN_BUDGET_EXCEEDED",
                    "D3D11 color and mask attachments exceed the budget",
                ));
            }
            Ok(bytes)
        })
        .map_err(status)?;
        boundary("native_d3d11_layout_ready");
        prepare_targets(&self.device, &mut self.colors, color.sizes.iter().copied())?;
        prepare_targets(&self.device, &mut self.masks, masks.extents())?;
        boundary("native_d3d11_targets_ready");

        let mut constants = std::mem::take(&mut self.constants);
        let encoded =
            crate::encoding::encode(self, output, textures, &color, &masks, &mut constants);
        boundary("native_d3d11_encoded");
        self.constants = constants;
        // Discard even a partial recording on error before accepting another frame.
        let mut commands = None;
        let finished = unsafe { self.deferred.FinishCommandList(false, Some(&mut commands)) };
        encoded?;
        finished?;
        commands.context("D3D11 model command list")
    }
}

impl Geometry {
    fn matches(&self, frame: &DrawableFrame) -> bool {
        self.topology.len() == frame.drawables.len()
            && self
                .topology
                .iter()
                .zip(&frame.drawables)
                .all(|(previous, mesh)| match previous {
                    None => !mesh.renderable,
                    Some((vertices, indices)) => {
                        mesh.renderable
                            && *vertices == mesh.positions.len()
                            && Arc::ptr_eq(indices, &mesh.indices)
                    }
                })
    }

    fn update_vertices(&mut self, frame: &DrawableFrame) {
        self.vertices.truncate(4);
        for mesh in frame.drawables.iter().filter(|mesh| mesh.renderable) {
            self.vertices.extend(
                mesh.positions
                    .iter()
                    .zip(mesh.uvs.iter())
                    .map(|(p, uv)| [p.x, p.y, uv.x, 1. - uv.y]),
            );
        }
    }

    fn new(device: &ID3D11Device, frame: &DrawableFrame) -> Result<Self> {
        // The first quad composites cropped offscreen textures in pixel space.
        let mut vertices = vec![
            [0., 0., 0., 0.],
            [1., 0., 1., 0.],
            [1., 1., 1., 1.],
            [0., 1., 0., 1.],
        ];
        let mut indices = vec![0_u32, 1, 2, 0, 2, 3];
        let mut ranges = Vec::with_capacity(frame.drawables.len());
        for mesh in &frame.drawables {
            let base_vertex = i32::try_from(vertices.len())?;
            let first_index = u32::try_from(indices.len())?;
            if mesh.renderable {
                vertices.extend(
                    mesh.positions
                        .iter()
                        .zip(mesh.uvs.iter())
                        .map(|(p, uv)| [p.x, p.y, uv.x, 1. - uv.y]),
                );
                indices.extend_from_slice(&mesh.indices);
            }
            ranges.push(DrawRange {
                count: u32::try_from(indices.len())? - first_index,
                first_index,
                base_vertex,
            });
        }
        Ok(Self {
            vertex: dynamic_buffer(
                device,
                std::mem::size_of_val(vertices.as_slice()),
                D3D11_BIND_VERTEX_BUFFER,
            )?,
            index: buffer(device, &indices, D3D11_BIND_INDEX_BUFFER)?,
            ranges,
            vertices,
            topology: frame
                .drawables
                .iter()
                .map(|mesh| {
                    mesh.renderable
                        .then(|| (mesh.positions.len(), mesh.indices.clone()))
                })
                .collect(),
        })
    }
}

fn prepare_targets(
    device: &ID3D11Device,
    targets: &mut Vec<D3d11Target>,
    sizes: impl Iterator<Item = [u32; 2]>,
) -> Result<()> {
    let mut available = std::mem::take(targets);
    for extent in sizes {
        let target = match available.iter().position(|target| target.extent == extent) {
            Some(i) => available.swap_remove(i),
            None => D3d11Target::new(device, extent)?,
        };
        targets.push(target);
    }
    Ok(())
}

fn status(error: zipzi_core::Status) -> anyhow::Error {
    D3d11ValidationError(error).into()
}

impl D3d11Renderer {
    pub fn new(device: ID3D11Device) -> Result<Self> {
        let shader = include_bytes!("model.hlsl");
        let compile = crate::compile_shader;
        let vertex_code = compile(shader, "vs_draw", "vs_5_0")?;
        let pixel_code = compile(shader, "ps_draw", "ps_5_0")?;
        let mask_code = compile(shader, "ps_mask", "ps_5_0")?;
        let raw_code = compile(shader, "ps_extended_draw", "ps_5_0")?;
        let mut deferred = None;
        let mut input = None;
        let mut vertex = None;
        let mut pixel = None;
        let mut mask_pixel = None;
        let mut raw_pixel = None;
        unsafe {
            device.CreateDeferredContext(0, Some(&mut deferred))?;
            device.CreateInputLayout(
                &[
                    D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: s!("POSITION"),
                        Format: DXGI_FORMAT_R32G32_FLOAT,
                        ..Default::default()
                    },
                    D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: s!("TEXCOORD"),
                        Format: DXGI_FORMAT_R32G32_FLOAT,
                        AlignedByteOffset: 8,
                        ..Default::default()
                    },
                ],
                &vertex_code,
                Some(&mut input),
            )?;
            device.CreateVertexShader(&vertex_code, None, Some(&mut vertex))?;
            device.CreatePixelShader(&pixel_code, None, Some(&mut pixel))?;
            device.CreatePixelShader(&mask_code, None, Some(&mut mask_pixel))?;
            device.CreatePixelShader(&raw_code, None, Some(&mut raw_pixel))?;
        }
        let raster = |cull, ccw: bool| -> Result<ID3D11RasterizerState> {
            let mut state = None;
            unsafe {
                device.CreateRasterizerState(
                    &D3D11_RASTERIZER_DESC {
                        FillMode: D3D11_FILL_SOLID,
                        CullMode: cull,
                        FrontCounterClockwise: ccw.into(),
                        DepthClipEnable: true.into(),
                        ..Default::default()
                    },
                    Some(&mut state),
                )?;
            }
            state.context("D3D11 rasterizer")
        };
        let blend = |src, dst, src_alpha, dst_alpha| -> Result<ID3D11BlendState> {
            let mut desc = D3D11_BLEND_DESC::default();
            desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: src,
                DestBlend: dst,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: src_alpha,
                DestBlendAlpha: dst_alpha,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let mut state = None;
            unsafe { device.CreateBlendState(&desc, Some(&mut state))? };
            state.context("D3D11 blend state")
        };
        let sampler = |mode| -> Result<Option<ID3D11SamplerState>> {
            let mut state = None;
            unsafe {
                device.CreateSamplerState(
                    &D3D11_SAMPLER_DESC {
                        Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                        AddressU: mode,
                        AddressV: mode,
                        AddressW: mode,
                        MaxAnisotropy: 1,
                        ComparisonFunc: D3D11_COMPARISON_NEVER,
                        MaxLOD: f32::MAX,
                        ..Default::default()
                    },
                    Some(&mut state),
                )?;
            }
            Ok(state)
        };
        Ok(Self {
            deferred: deferred.context("D3D11 deferred context")?,
            input: input.context("D3D11 input layout")?,
            vertex: vertex.context("D3D11 vertex shader")?,
            pixel: pixel.context("D3D11 pixel shader")?,
            mask_pixel: mask_pixel.context("D3D11 mask shader")?,
            raw_pixel: raw_pixel.context("D3D11 blend shader")?,
            raster: [
                raster(D3D11_CULL_NONE, true)?,
                raster(D3D11_CULL_BACK, true)?,
                raster(D3D11_CULL_BACK, false)?,
            ],
            blend: [
                blend(
                    D3D11_BLEND_ONE,
                    D3D11_BLEND_INV_SRC_ALPHA,
                    D3D11_BLEND_ONE,
                    D3D11_BLEND_INV_SRC_ALPHA,
                )?,
                blend(
                    D3D11_BLEND_ONE,
                    D3D11_BLEND_ONE,
                    D3D11_BLEND_ZERO,
                    D3D11_BLEND_ONE,
                )?,
                blend(
                    D3D11_BLEND_ZERO,
                    D3D11_BLEND_SRC_COLOR,
                    D3D11_BLEND_ZERO,
                    D3D11_BLEND_ONE,
                )?,
            ],
            samplers: [
                sampler(D3D11_TEXTURE_ADDRESS_WRAP)?,
                sampler(D3D11_TEXTURE_ADDRESS_CLAMP)?,
            ],
            device,
            frame: None,
            plan: ScenePlan::default(),
            geometry: None,
            constants: ConstantBuffers::default(),
            masks: Vec::new(),
            colors: Vec::new(),
        })
    }
}
