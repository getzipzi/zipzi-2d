//! Immutable mesh overlays on GPUI's D3D11 device and immediate queue.
//! A deferred context isolates pipeline state; execution restores GPUI's state.
use crate::{OverlayPass, OverlayStyle, OverlayViewport, overlay_data::Key};
use anyhow::{Context, Result, ensure};
use std::{ffi::CString, sync::Arc};
use windows::{
    Win32::Graphics::{Direct3D::Fxc::*, Direct3D::*, Direct3D11::*, Dxgi::Common::*},
    core::PCSTR,
};
use zipzi_editor_core::OverlaySnapshot;

pub struct WindowsOverlayRenderer {
    device: ID3D11Device,
    immediate: ID3D11DeviceContext,
    deferred: ID3D11DeviceContext,
    edges: ID3D11VertexShader,
    markers: ID3D11VertexShader,
    solid: ID3D11PixelShader,
    marker_fill: ID3D11PixelShader,
    blend: ID3D11BlendState,
    raster: ID3D11RasterizerState,
    depth: ID3D11DepthStencilState,
    cached: [Option<(Key, Arc<gpui::DirectXSurface>)>; 3],
    // The immediate context is shared with GPUI; only the owning UI thread submits.
    _ui_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl WindowsOverlayRenderer {
    pub fn new(device: ID3D11Device) -> Result<Self> {
        let immediate = unsafe { device.GetImmediateContext()? };
        let mut deferred = None;
        unsafe {
            device.CreateDeferredContext(0, Some(&mut deferred))?;
        }
        let vertex = |entry| -> Result<ID3D11VertexShader> {
            let bytes = compile(entry, "vs_5_0")?;
            let mut shader = None;
            unsafe {
                device.CreateVertexShader(&bytes, None, Some(&mut shader))?;
            }
            shader.context("overlay vertex shader")
        };
        let pixel = |entry| -> Result<ID3D11PixelShader> {
            let bytes = compile(entry, "ps_5_0")?;
            let mut shader = None;
            unsafe {
                device.CreatePixelShader(&bytes, None, Some(&mut shader))?;
            }
            shader.context("overlay pixel shader")
        };
        let mut blend = None;
        let mut raster = None;
        let mut depth = None;
        unsafe {
            let mut desc = D3D11_BLEND_DESC::default();
            desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            device.CreateBlendState(&desc, Some(&mut blend))?;
            device.CreateRasterizerState(
                &D3D11_RASTERIZER_DESC {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: D3D11_CULL_NONE,
                    DepthClipEnable: true.into(),
                    MultisampleEnable: true.into(),
                    ..Default::default()
                },
                Some(&mut raster),
            )?;
            device.CreateDepthStencilState(
                &D3D11_DEPTH_STENCIL_DESC {
                    DepthEnable: false.into(),
                    DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
                    DepthFunc: D3D11_COMPARISON_ALWAYS,
                    ..Default::default()
                },
                Some(&mut depth),
            )?;
        }
        Ok(Self {
            edges: vertex("edge_vertex")?,
            markers: vertex("marker_vertex")?,
            solid: pixel("solid_fragment")?,
            marker_fill: pixel("marker_fragment")?,
            immediate,
            deferred: deferred.context("overlay deferred context")?,
            blend: blend.context("overlay blend")?,
            raster: raster.context("overlay rasterizer")?,
            depth: depth.context("overlay depth state")?,
            device,
            cached: [None, None, None],
            _ui_thread: Default::default(),
        })
    }
    pub fn uses_device(&self, device: &ID3D11Device) -> bool {
        self.device == *device
    }
    pub fn draw_pass(
        &mut self,
        viewport: OverlayViewport,
        overlay: &OverlaySnapshot,
        style: OverlayStyle,
        pass: OverlayPass,
    ) -> Result<Option<Arc<gpui::DirectXSurface>>> {
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("overlay.d3d11_total");
        let key = Key::new(viewport, overlay, style, pass)?;
        let slot = match pass {
            OverlayPass::All => 0,
            OverlayPass::Edges => 1,
            OverlayPass::Vertices => 2,
        };
        if key.edges.is_empty() && key.markers.is_empty() {
            self.cached[slot] = None;
            return Ok(None);
        }
        if let Some((old, image)) = &self.cached[slot] {
            if old == &key {
                return Ok(Some(image.clone()));
            }
        }
        // Never overwrite textures held by a retained GPUI scene, even after GPU completion.
        let (texture, rtv) = self.target(viewport.extent, 1)?;
        let msaa = if key.edges.is_empty() {
            None
        } else {
            Some(self.target(viewport.extent, 4)?)
        };
        let uniforms = key.uniforms();
        let constants = self.buffer(
            std::slice::from_ref(&uniforms),
            D3D11_BIND_CONSTANT_BUFFER,
            0,
        )?;
        let c = &self.deferred;
        unsafe {
            c.ClearState();
            c.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: viewport.extent[0] as f32,
                Height: viewport.extent[1] as f32,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            c.RSSetState(&self.raster);
            c.OMSetBlendState(&self.blend, None, u32::MAX);
            c.OMSetDepthStencilState(&self.depth, 0);
            c.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            c.VSSetConstantBuffers(0, Some(&[Some(constants.clone())]));
            c.PSSetConstantBuffers(0, Some(&[Some(constants)]));
            if let Some((msaa_texture, msaa_view)) = &msaa {
                c.ClearRenderTargetView(msaa_view, &[0.; 4]);
                c.OMSetRenderTargets(Some(&[Some(msaa_view.clone())]), None);
                self.encode(&key.edges, &self.edges, &self.solid)?;
                c.OMSetRenderTargets(None, None);
                c.ResolveSubresource(&texture, 0, msaa_texture, 0, DXGI_FORMAT_B8G8R8A8_UNORM);
            } else {
                c.ClearRenderTargetView(&rtv, &[0.; 4]);
            }
            if !key.markers.is_empty() {
                c.OMSetRenderTargets(Some(&[Some(rtv)]), None);
                self.encode(&key.markers, &self.markers, &self.marker_fill)?;
            }
            c.OMSetRenderTargets(None, None);
            c.VSSetShaderResources(0, Some(&[None]));
        }
        let mut commands = None;
        unsafe {
            c.FinishCommandList(false, Some(&mut commands))?;
        }
        let mut view = None;
        let device5: ID3D11Device5 = self.device.cast()?;
        let mut ready = None;
        let mut consumed = None;
        unsafe {
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut view))?;
            device5.CreateFence::<ID3D11Fence>(0, D3D11_FENCE_FLAG_NONE, &mut ready)?;
            device5.CreateFence::<ID3D11Fence>(0, D3D11_FENCE_FLAG_NONE, &mut consumed)?;
        }
        let ready = ready.context("overlay ready fence")?;
        let image = Arc::new(gpui::DirectXSurface {
            device: self.device.clone(),
            view: view.context("overlay SRV")?,
            extent: viewport.extent,
            ready: ready.clone(),
            ready_value: 1,
            consumed: consumed.context("overlay consumed fence")?,
            consumed_value: 0.into(),
        });
        let commands = commands.context("overlay command list")?;
        let context4: ID3D11DeviceContext4 = self.immediate.cast()?;
        unsafe {
            self.immediate.ExecuteCommandList(&commands, true);
            context4.Signal(&ready, 1)?;
        }
        self.cached[slot] = Some((key, image.clone()));
        Ok(Some(image))
    }
    fn target(
        &self,
        extent: [u32; 2],
        samples: u32,
    ) -> Result<(ID3D11Texture2D, ID3D11RenderTargetView)> {
        let mut texture = None;
        let mut view = None;
        unsafe {
            self.device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: extent[0],
                    Height: extent[1],
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: samples,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: if samples == 1 {
                        (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32
                    } else {
                        D3D11_BIND_RENDER_TARGET.0 as u32
                    },
                    ..Default::default()
                },
                None,
                Some(&mut texture),
            )?;
            let texture = texture.context("overlay render texture")?;
            self.device
                .CreateRenderTargetView(&texture, None, Some(&mut view))?;
            Ok((texture, view.context("overlay RTV")?))
        }
    }
    // Only fully initialized float instance arrays and repr(C) Uniforms (no padding)
    // enter this private helper. D3D11 copies immutable initial data before returning.
    fn buffer<T>(&self, data: &[T], bind: D3D11_BIND_FLAG, stride: u32) -> Result<ID3D11Buffer> {
        let bytes = std::mem::size_of_val(data);
        ensure!(
            bytes > 0 && bytes <= 128 * 1024 * 1024,
            "Overlay buffer exceeds budget"
        );
        let mut buffer = None;
        unsafe {
            self.device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: bytes as u32,
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: bind.0 as u32,
                    MiscFlags: if stride == 0 {
                        0
                    } else {
                        D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32
                    },
                    StructureByteStride: stride,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: data.as_ptr().cast(),
                    ..Default::default()
                }),
                Some(&mut buffer),
            )?;
        }
        buffer.context("overlay buffer")
    }
    fn encode(
        &self,
        data: &[[f32; 4]],
        vertex: &ID3D11VertexShader,
        pixel: &ID3D11PixelShader,
    ) -> Result<()> {
        let buffer = self.buffer(data, D3D11_BIND_SHADER_RESOURCE, 16)?;
        let mut view = None;
        unsafe {
            self.device.CreateShaderResourceView(
                &buffer,
                Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_UNKNOWN,
                    ViewDimension: D3D_SRV_DIMENSION_BUFFER,
                    Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                        Buffer: D3D11_BUFFER_SRV {
                            Anonymous1: D3D11_BUFFER_SRV_0 { FirstElement: 0 },
                            Anonymous2: D3D11_BUFFER_SRV_1 {
                                NumElements: data.len() as u32,
                            },
                        },
                    },
                }),
                Some(&mut view),
            )?;
            self.deferred.VSSetShader(vertex, None);
            self.deferred.PSSetShader(pixel, None);
            self.deferred.VSSetShaderResources(0, Some(&[view]));
            self.deferred.DrawInstanced(6, data.len() as u32, 0, 0);
        }
        Ok(())
    }
}
use windows::core::Interface;
fn compile(entry: &str, target: &str) -> Result<Vec<u8>> {
    let source = include_bytes!("overlay.hlsl");
    let entry = CString::new(entry)?;
    let target = CString::new(target)?;
    let mut code = None;
    let mut errors = None;
    let result = unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            PCSTR::null(),
            None,
            None,
            PCSTR(entry.as_ptr().cast()),
            PCSTR(target.as_ptr().cast()),
            D3DCOMPILE_ENABLE_STRICTNESS | D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(e) = result {
        let diagnostic = errors
            .map(|b| unsafe {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    b.GetBufferPointer().cast(),
                    b.GetBufferSize(),
                ))
                .into_owned()
            })
            .unwrap_or_default();
        anyhow::bail!("Overlay shader: {e}: {diagnostic}");
    }
    let code = code.context("overlay shader bytecode")?;
    Ok(unsafe {
        std::slice::from_raw_parts(code.GetBufferPointer().cast(), code.GetBufferSize()).to_vec()
    })
}
