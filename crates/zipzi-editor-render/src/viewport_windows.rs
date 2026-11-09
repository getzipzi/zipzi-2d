//! UI-thread submission to an independent, premultiplied DirectComposition swap
//! chain. Model and controls share one buffer; GPUI owns the surrounding chrome.
//! No readback, CPU fence wait, or access to GPUI's immediate context off-thread.
use anyhow::{Context, Result, ensure};
use std::{ffi::CString, sync::Arc};
use windows::{
    Win32::Graphics::{Direct3D::Fxc::*, Direct3D::*, Direct3D11::*, Dxgi::Common::*, Dxgi::*},
    core::{Interface, PCSTR},
};

pub struct WindowsViewportPresenter {
    device: ID3D11Device,
    immediate: ID3D11DeviceContext4,
    deferred: ID3D11DeviceContext,
    chain: IDXGISwapChain1,
    extent: [u32; 2],
    vertex: ID3D11VertexShader,
    fragment: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    blend: ID3D11BlendState,
    raster: ID3D11RasterizerState,
    // A failed nonblocking Present retains the completed back buffer for retry.
    pending: bool,
    _ui_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl WindowsViewportPresenter {
    pub fn new(device: ID3D11Device, extent: [u32; 2]) -> Result<Self> {
        ensure!(
            extent.iter().all(|&v| v > 0 && v <= 16384),
            "Invalid viewport extent"
        );
        ensure!(
            u64::from(extent[0]) * u64::from(extent[1]) * 8 <= 64 * 1024 * 1024,
            "Native viewport exceeds the 64 MiB swap-chain budget"
        );
        let dxgi: IDXGIDevice = device.cast()?;
        let factory: IDXGIFactory2 = unsafe { dxgi.GetAdapter()?.GetParent()? };
        let chain = unsafe {
            factory.CreateSwapChainForComposition(
                &device,
                &DXGI_SWAP_CHAIN_DESC1 {
                    Width: extent[0],
                    Height: extent[1],
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: 2,
                    Scaling: DXGI_SCALING_STRETCH,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                    ..Default::default()
                },
                None,
            )?
        };
        let mut vertex = None;
        let mut fragment = None;
        let mut sampler = None;
        let mut blend = None;
        let mut raster = None;
        let mut deferred = None;
        unsafe {
            device.CreateVertexShader(&compile("vertex", "vs_5_0")?, None, Some(&mut vertex))?;
            device.CreatePixelShader(&compile("fragment", "ps_5_0")?, None, Some(&mut fragment))?;
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    ComparisonFunc: D3D11_COMPARISON_NEVER,
                    MaxAnisotropy: 1,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
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
                    ..Default::default()
                },
                Some(&mut raster),
            )?;
            device.CreateDeferredContext(0, Some(&mut deferred))?;
        }
        Ok(Self {
            immediate: unsafe { device.GetImmediateContext()? }.cast()?,
            device,
            chain,
            extent,
            deferred: deferred.context("viewport deferred context")?,
            vertex: vertex.context("viewport vertex shader")?,
            fragment: fragment.context("viewport fragment shader")?,
            sampler: sampler.context("viewport sampler")?,
            blend: blend.context("viewport blend")?,
            raster: raster.context("viewport rasterizer")?,
            pending: false,
            _ui_thread: Default::default(),
        })
    }

    pub fn swap_chain(&self) -> &IDXGISwapChain1 {
        &self.chain
    }
    pub fn matches(&self, device: &ID3D11Device, extent: [u32; 2]) -> bool {
        self.device == *device && self.extent == extent
    }

    /// A pending buffer is retried, never overwritten. false applies backpressure
    /// to the host, which retains/coalesces the latest accepted canvas snapshot.
    pub fn present(
        &mut self,
        model: &Arc<gpui::DirectXSurface>,
        overlay: Option<&Arc<gpui::DirectXSurface>>,
        model_rect: [f32; 4],
    ) -> Result<bool> {
        self.encode(model, overlay, model_rect)?;
        self.retry()
    }

    fn encode(
        &mut self,
        model: &Arc<gpui::DirectXSurface>,
        overlay: Option<&Arc<gpui::DirectXSurface>>,
        model_rect: [f32; 4],
    ) -> Result<()> {
        ensure!(
            !self.pending,
            "Retry the pending viewport buffer before drawing another"
        );
        ensure!(
            model.device == self.device && overlay.is_none_or(|s| s.device == self.device),
            "Viewport sources belong to another device"
        );
        let texture: ID3D11Texture2D = unsafe { self.chain.GetBuffer(0)? };
        let mut target = None;
        unsafe {
            self.device
                .CreateRenderTargetView(&texture, None, Some(&mut target))?;
        }
        let target = target.context("viewport render target")?;
        let c = &self.deferred;
        unsafe {
            c.ClearState();
            c.RSSetState(&self.raster);
            c.OMSetBlendState(&self.blend, None, u32::MAX);
            c.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            c.VSSetShader(&self.vertex, None);
            c.PSSetShader(&self.fragment, None);
            c.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            c.ClearRenderTargetView(&target, &[0.; 4]);
            c.OMSetRenderTargets(Some(&[Some(target)]), None);
            for (image, rect) in std::iter::once((model, model_rect)).chain(overlay.map(|image| {
                (
                    image,
                    [0., 0., self.extent[0] as f32, self.extent[1] as f32],
                )
            })) {
                c.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    TopLeftX: rect[0],
                    TopLeftY: rect[1],
                    Width: rect[2],
                    Height: rect[3],
                    MaxDepth: 1.,
                    ..Default::default()
                }]));
                c.PSSetShaderResources(0, Some(&[Some(image.view.clone())]));
                c.Draw(3, 0);
            }
            c.PSSetShaderResources(0, Some(&[None]));
            c.OMSetRenderTargets(None, None);
            let mut commands = None;
            c.FinishCommandList(false, Some(&mut commands))?;
            let commands = commands.context("viewport command list")?;
            // All consumers use this same UI-owned immediate context and timeline.
            for image in std::iter::once(model).chain(overlay) {
                self.immediate.Wait(&image.ready, image.ready_value)?;
            }
            let values: Vec<_> = std::iter::once(model)
                .chain(overlay)
                .map(|image| (image, image.next_consumed_value()))
                .collect();
            self.immediate.ExecuteCommandList(&commands, true);
            for (image, value) in values {
                self.immediate.Signal(&image.consumed, value)?;
            }
        }
        self.pending = true;
        Ok(())
    }

    pub fn retry(&mut self) -> Result<bool> {
        if !self.pending {
            return Ok(true);
        }
        let result = unsafe { self.chain.Present(0, DXGI_PRESENT_DO_NOT_WAIT) };
        if result == DXGI_ERROR_WAS_STILL_DRAWING {
            return Ok(false);
        }
        result.ok()?;
        self.pending = false;
        Ok(true)
    }
}

fn compile(entry: &str, target: &str) -> Result<Vec<u8>> {
    let source = include_bytes!("viewport_windows.hlsl");
    let entry = CString::new(entry)?;
    let target = CString::new(target)?;
    let mut code = None;
    let mut errors = None;
    unsafe {
        let result = D3DCompile(
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
        );
        if let Err(error) = result {
            let diagnostic = errors
                .map(|blob| {
                    String::from_utf8_lossy(std::slice::from_raw_parts(
                        blob.GetBufferPointer().cast(),
                        blob.GetBufferSize(),
                    ))
                    .into_owned()
                })
                .unwrap_or_default();
            anyhow::bail!("Native viewport shader: {error}: {diagnostic}");
        }
        let code = code.context("viewport shader")?;
        Ok(
            std::slice::from_raw_parts(code.GetBufferPointer().cast(), code.GetBufferSize())
                .to_vec(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::HMODULE;

    fn device() -> Result<ID3D11Device> {
        let mut device = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
        }
        device.context("hardware D3D11 device")
    }

    fn solid(device: &ID3D11Device, color: [u8; 4]) -> Result<Arc<gpui::DirectXSurface>> {
        let mut texture = None;
        let mut view = None;
        let mut ready = None;
        let mut consumed = None;
        unsafe {
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: 1,
                    Height: 1,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: color.as_ptr().cast(),
                    SysMemPitch: 4,
                    SysMemSlicePitch: 4,
                }),
                Some(&mut texture),
            )?;
            device.CreateShaderResourceView(&texture.context("source")?, None, Some(&mut view))?;
            let d: ID3D11Device5 = device.cast()?;
            d.CreateFence::<ID3D11Fence>(0, D3D11_FENCE_FLAG_NONE, &mut ready)?;
            d.CreateFence::<ID3D11Fence>(0, D3D11_FENCE_FLAG_NONE, &mut consumed)?;
        }
        Ok(Arc::new(gpui::DirectXSurface {
            device: device.clone(),
            view: view.context("source view")?,
            extent: [1, 1],
            ready: ready.context("ready")?,
            ready_value: 0,
            consumed: consumed.context("consumed")?,
            consumed_value: 0.into(),
        }))
    }

    fn pixels(presenter: &WindowsViewportPresenter) -> Result<Vec<[u8; 4]>> {
        unsafe {
            let source: ID3D11Texture2D = presenter.chain.GetBuffer(0)?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            source.GetDesc(&mut desc);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.MiscFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            presenter
                .device
                .CreateTexture2D(&desc, None, Some(&mut staging))?;
            let staging = staging.context("readback")?;
            presenter.immediate.CopyResource(&staging, &source);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            presenter
                .immediate
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let mut pixels = Vec::new();
            for y in 0..desc.Height as usize {
                let row = std::slice::from_raw_parts(
                    mapped.pData.cast::<u8>().add(y * mapped.RowPitch as usize),
                    desc.Width as usize * 4,
                );
                pixels.extend(row.chunks_exact(4).map(|p| [p[2], p[1], p[0], p[3]]));
            }
            presenter.immediate.Unmap(&staging, 0);
            Ok(pixels)
        }
    }

    #[test]
    fn native_viewport_composes_premultiplied_model_and_controls_together() -> Result<()> {
        let device = device()?;
        let model = solid(&device, [0, 0, 255, 255])?;
        let overlay = solid(&device, [128, 0, 0, 128])?;
        let mut presenter = WindowsViewportPresenter::new(device, [8, 8])?;
        presenter.encode(&model, Some(&overlay), [0., 0., 8., 8.])?;
        for pixel in pixels(&presenter)? {
            assert!(
                pixel
                    .iter()
                    .zip([128u8, 0, 127, 255])
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "{pixel:?}"
            );
        }
        assert!(model.is_idle() && overlay.is_idle());
        // Do not write into a buffer whose nonblocking Present has not retired.
        assert!(presenter.encode(&model, None, [0., 0., 8., 8.]).is_err());
        Ok(())
    }

    #[test]
    fn native_viewport_crops_model_and_preserves_transparent_background() -> Result<()> {
        let device = device()?;
        let model = solid(&device, [255, 0, 0, 255])?;
        let mut presenter = WindowsViewportPresenter::new(device, [8, 8])?;
        presenter.encode(&model, None, [2., 2., 4., 4.])?;
        for (i, pixel) in pixels(&presenter)?.into_iter().enumerate() {
            let inside = (2..6).contains(&(i % 8)) && (2..6).contains(&(i / 8));
            assert_eq!(pixel, if inside { [255, 0, 0, 255] } else { [0; 4] });
        }
        Ok(())
    }
}
