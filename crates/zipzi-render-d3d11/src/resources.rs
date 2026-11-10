use anyhow::{ensure, Context, Result};
use std::{collections::HashMap, ffi::CString};
use windows::{
    core::PCSTR,
    Win32::Graphics::{Direct3D::Fxc::*, Direct3D11::*, Dxgi::Common::*},
};
use zipzi_render::{texture::premultiply_rgba8, TextureCatalog, TextureInfo};

pub const MAX_TEXTURE_DIMENSION: u32 = D3D11_REQ_TEXTURE2D_U_OR_V_DIMENSION;

pub struct D3d11Texture {
    pub(crate) view: ID3D11ShaderResourceView,
    device: ID3D11Device,
    info: TextureInfo,
    bytes: u64,
}

impl D3d11Texture {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

pub struct D3d11TextureCatalog<'a>(HashMap<&'a str, &'a D3d11Texture>);

impl<'a> D3d11TextureCatalog<'a> {
    pub fn new(textures: impl IntoIterator<Item = (&'a str, &'a D3d11Texture)>) -> Self {
        Self(textures.into_iter().collect())
    }

    pub(crate) fn view(&self, id: &str) -> Result<&ID3D11ShaderResourceView> {
        self.0
            .get(id)
            .map(|t| &t.view)
            .with_context(|| format!("Missing D3D11 texture {id}"))
    }

    pub(crate) fn validate_device(&self, device: &ID3D11Device) -> Result<()> {
        ensure!(
            self.0.values().all(|texture| &texture.device == device),
            "D3D11 source texture belongs to a different device"
        );
        Ok(())
    }
}

impl TextureCatalog for D3d11TextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<TextureInfo> {
        self.0.get(id).map(|texture| texture.info)
    }
}

/// An RGBA8 render target that can also be sampled or copied.
pub struct D3d11Target {
    pub(crate) extent: [u32; 2],
    pub(crate) texture: ID3D11Texture2D,
    pub(crate) view: ID3D11ShaderResourceView,
    pub(crate) rtv: ID3D11RenderTargetView,
}

impl D3d11Target {
    pub fn new(device: &ID3D11Device, extent: [u32; 2]) -> Result<Self> {
        ensure!(
            valid_extent(extent) && bytes(extent) <= zipzi_render::OFFSCREEN_BUDGET_BYTES as u64,
            "Invalid D3D11 render target extent"
        );
        let mut desc = texture_desc(extent);
        desc.BindFlags |= D3D11_BIND_RENDER_TARGET.0 as u32;
        let mut texture = None;
        let mut view = None;
        let mut rtv = None;
        unsafe {
            device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.context("D3D11 target texture")?;
            device.CreateShaderResourceView(&texture, None, Some(&mut view))?;
            device.CreateRenderTargetView(&texture, None, Some(&mut rtv))?;
            Ok(Self {
                extent,
                texture,
                view: view.context("D3D11 target SRV")?,
                rtv: rtv.context("D3D11 target RTV")?,
            })
        }
    }

    pub fn view(&self) -> &ID3D11ShaderResourceView {
        &self.view
    }
}

pub(crate) fn valid_extent(extent: [u32; 2]) -> bool {
    extent
        .into_iter()
        .all(|value| (1..=MAX_TEXTURE_DIMENSION).contains(&value))
}

pub(crate) fn bytes([width, height]: [u32; 2]) -> u64 {
    u64::from(width) * u64::from(height) * 4
}

/// Upload straight RGBA and straight mip levels as premultiplied RGBA8.
pub fn upload_rgba8(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    rgba: &[u8],
    mipmaps: &[(u32, u32, Vec<u8>)],
) -> Result<D3d11Texture> {
    ensure!(
        valid_extent([width, height]) && rgba.len() as u64 == bytes([width, height]),
        "Invalid D3D11 source texture"
    );
    ensure!(
        mipmaps.len() < (32 - width.max(height).leading_zeros()) as usize,
        "Invalid D3D11 source mip count"
    );
    for (i, (w, h, pixels)) in mipmaps.iter().enumerate() {
        ensure!(
            [*w, *h] == [(width >> (i + 1)).max(1), (height >> (i + 1)).max(1)]
                && pixels.len() as u64 == bytes([*w, *h]),
            "Invalid D3D11 source mip dimensions"
        );
    }
    let pixels: Vec<_> = std::iter::once(premultiply_rgba8(rgba))
        .chain(mipmaps.iter().map(|(_, _, bytes)| premultiply_rgba8(bytes)))
        .collect();
    let levels: Vec<_> = pixels
        .iter()
        .enumerate()
        .map(|(level, data)| D3D11_SUBRESOURCE_DATA {
            pSysMem: data.as_ptr().cast(),
            SysMemPitch: (width >> level).max(1) * 4,
            ..Default::default()
        })
        .collect();
    let mut desc = texture_desc([width, height]);
    desc.Usage = D3D11_USAGE_IMMUTABLE;
    desc.MipLevels = levels.len() as u32;
    let mut texture = None;
    let mut view = None;
    unsafe {
        device.CreateTexture2D(&desc, Some(levels.as_ptr()), Some(&mut texture))?;
        device.CreateShaderResourceView(
            &texture.context("D3D11 source texture")?,
            None,
            Some(&mut view),
        )?;
    }
    Ok(D3d11Texture {
        view: view.context("D3D11 source SRV")?,
        device: device.clone(),
        info: TextureInfo { width, height },
        bytes: pixels.iter().map(|bytes| bytes.len() as u64).sum(),
    })
}

pub(crate) fn buffer<T>(
    device: &ID3D11Device,
    data: &[T],
    bind: D3D11_BIND_FLAG,
) -> Result<ID3D11Buffer> {
    let size = std::mem::size_of_val(data);
    ensure!(
        (1..=128 * 1024 * 1024).contains(&size),
        "D3D11 preview buffer size"
    );
    let mut buffer = None;
    // Callers supply initialized packed arrays of f32 or u32. CreateBuffer copies them.
    unsafe {
        device.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: size as u32,
                Usage: D3D11_USAGE_IMMUTABLE,
                BindFlags: bind.0 as u32,
                ..Default::default()
            },
            Some(&D3D11_SUBRESOURCE_DATA {
                pSysMem: data.as_ptr().cast(),
                ..Default::default()
            }),
            Some(&mut buffer),
        )?;
    }
    buffer.context("D3D11 preview buffer")
}

pub(crate) fn dynamic_buffer(
    device: &ID3D11Device,
    size: usize,
    bind: D3D11_BIND_FLAG,
) -> Result<ID3D11Buffer> {
    ensure!(
        (1..=128 * 1024 * 1024).contains(&size),
        "D3D11 dynamic buffer size"
    );
    let mut buffer = None;
    unsafe {
        device.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: size as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: bind.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            },
            None,
            Some(&mut buffer),
        )?;
    }
    buffer.context("D3D11 dynamic buffer")
}

/// The complete upload belongs to this command list, even if older recordings
/// using the same buffer are dropped, submitted later, or replayed.
/// Callers supply initialized packed arrays fitting the buffer's allocation.
pub(crate) fn write_discard<T: Copy>(
    context: &ID3D11DeviceContext,
    buffer: &ID3D11Buffer,
    data: &[T],
) -> Result<()> {
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        context.Map(buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
        std::ptr::copy_nonoverlapping(
            data.as_ptr().cast::<u8>(),
            mapped.pData.cast(),
            std::mem::size_of_val(data),
        );
        context.Unmap(buffer, 0);
    }
    Ok(())
}

pub(crate) fn texture_desc(extent: [u32; 2]) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: extent[0],
        Height: extent[1],
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    }
}

pub fn compile_shader(source: &[u8], entry: &str, target: &str) -> Result<Vec<u8>> {
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
        anyhow::bail!("D3D11 shader {entry:?}: {e}: {diagnostic}");
    }
    let code = code.context("D3D11 shader bytecode")?;
    Ok(unsafe {
        std::slice::from_raw_parts(code.GetBufferPointer().cast(), code.GetBufferSize()).to_vec()
    })
}
