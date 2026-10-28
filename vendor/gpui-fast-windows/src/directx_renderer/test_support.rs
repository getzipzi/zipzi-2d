//! Offscreen acceptance harness using the production Windows Fast pipelines.
use super::*;
use crate::fast::frame::{FrameState, Target};

pub struct SurfaceTestRenderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    atlas: DirectXAtlas,
    globals: DirectXGlobalElements,
    pipelines: DirectXRenderPipelines,
    frame: FrameState,
}
impl SurfaceTestRenderer {
    pub fn new(device: ID3D11Device) -> Result<Self> {
        let context = unsafe { device.GetImmediateContext()? };
        let globals = DirectXGlobalElements::new(&device)?;
        let pipelines = DirectXRenderPipelines::new(&device)?;
        let atlas = DirectXAtlas::new(&device, &context);
        Ok(Self {
            device,
            context,
            atlas,
            globals,
            pipelines,
            frame: FrameState::default(),
        })
    }

    pub fn render(
        &mut self,
        scene: &Scene,
        extent: [u32; 2],
        clear: [f32; 4],
    ) -> Result<image::RgbaImage> {
        let [width, height] = extent;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: RENDER_TARGET_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        unsafe {
            let mut texture = None;
            self.device
                .CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.context("offscreen target")?;
            let mut rtv = None;
            self.device
                .CreateRenderTargetView(&texture, None, Some(&mut rtv))?;
            let mut rasterizer = None;
            self.device.CreateRasterizerState(
                &D3D11_RASTERIZER_DESC {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: D3D11_CULL_NONE,
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                Some(&mut rasterizer),
            )?;
            self.context.RSSetState(rasterizer.as_ref());
            self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: width as f32,
                Height: height as f32,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            self.context
                .OMSetRenderTargets(Some(slice::from_ref(&rtv)), None);
            self.context
                .ClearRenderTargetView(rtv.as_ref().unwrap(), &clear);
            update_buffer(
                &self.context,
                self.globals.global_params_buffer.as_ref().unwrap(),
                &[GlobalParams {
                    viewport_size: [width as f32, height as f32],
                    ..Default::default()
                }],
            )?;
            self.context
                .VSSetConstantBuffers(0, Some(slice::from_ref(&self.globals.global_params_buffer)));
            self.context
                .PSSetConstantBuffers(0, Some(slice::from_ref(&self.globals.global_params_buffer)));
            self.context
                .VSSetConstantBuffers(1, Some(slice::from_ref(&self.globals.batch_params_buffer)));
            let target = Target {
                device: &self.device,
                device_context: &self.context,
                atlas: &self.atlas,
                globals: &self.globals,
                paths: None,
            };
            crate::fast::frame::upload(&target, &mut self.pipelines, scene)?;
            crate::fast::frame::draw_scene(&target, &mut self.pipelines, &mut self.frame, scene)?;
            self.context.OMSetRenderTargets(None, None);
            let mut staging = None;
            self.device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    ..desc
                },
                None,
                Some(&mut staging),
            )?;
            let staging = staging.context("offscreen readback")?;
            self.context.CopyResource(&staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            for row in 0..height as usize {
                std::ptr::copy_nonoverlapping(
                    (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                    pixels.as_mut_ptr().add(row * width as usize * 4),
                    width as usize * 4,
                );
            }
            self.context.Unmap(&staging, 0);
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            image::RgbaImage::from_raw(width, height, pixels).context("offscreen pixels")
        }
    }
}
