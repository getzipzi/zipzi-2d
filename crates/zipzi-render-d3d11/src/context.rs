use anyhow::{bail, Context, Result};
use windows::Win32::{
    Foundation::HMODULE,
    Graphics::{
        Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0},
        Direct3D11::*,
        Dxgi::*,
    },
};

use crate::{
    resources::{texture_desc, MAX_TEXTURE_DIMENSION},
    D3d11Target,
};

/// Hardware device and immediate context for a standalone capture worker.
pub struct D3d11Context {
    device: ID3D11Device,
    immediate: ID3D11DeviceContext,
    staging: Option<([u32; 2], ID3D11Texture2D)>,
    name: String,
}

impl D3d11Context {
    pub fn new() -> Result<Self> {
        let factory: IDXGIFactory6 = unsafe { CreateDXGIFactory1()? };
        for index in 0.. {
            let adapter: IDXGIAdapter1 = match unsafe {
                factory.EnumAdapterByGpuPreference(index, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)
            } {
                Ok(adapter) => adapter,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(e.into()),
            };
            let desc = unsafe { adapter.GetDesc1()? };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let mut device = None;
            let mut immediate = None;
            if unsafe {
                D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut immediate),
                )
            }
            .is_ok()
            {
                let name = String::from_utf16_lossy(&desc.Description);
                return Ok(Self {
                    device: device.context("D3D11 device")?,
                    immediate: immediate.context("D3D11 immediate context")?,
                    staging: None,
                    name: name.trim_end_matches('\0').to_owned(),
                });
            }
        }
        bail!("No Direct3D 11 hardware adapter with feature level 11.0")
    }

    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn max_texture_dimension(&self) -> u32 {
        MAX_TEXTURE_DIMENSION
    }

    pub fn submit(&mut self, commands: &ID3D11CommandList) -> Result<()> {
        let device = unsafe { commands.GetDevice()? };
        anyhow::ensure!(
            device == self.device,
            "D3D11 command list belongs to a different device"
        );
        unsafe {
            self.immediate.ExecuteCommandList(commands, false);
            self.device.GetDeviceRemovedReason()?;
        }
        Ok(())
    }

    pub fn read_rgba8(&mut self, target: &D3d11Target) -> Result<Vec<u8>> {
        anyhow::ensure!(
            unsafe { target.texture.GetDevice()? } == self.device,
            "D3D11 target belongs to a different device"
        );
        if self
            .staging
            .as_ref()
            .is_none_or(|(extent, _)| *extent != target.extent)
        {
            let mut desc = texture_desc(target.extent);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            unsafe {
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut staging))?
            };
            self.staging = Some((target.extent, staging.context("D3D11 staging texture")?));
        }
        let staging = &self.staging.as_ref().unwrap().1;
        let [width, height] = target.extent;
        let pitch = width as usize * 4;
        let mut pixels = vec![0; pitch * height as usize];
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.immediate.CopyResource(staging, &target.texture);
            self.immediate
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            for (row, output) in pixels.chunks_exact_mut(pitch).enumerate() {
                std::ptr::copy_nonoverlapping(
                    mapped
                        .pData
                        .cast::<u8>()
                        .add(row * mapped.RowPitch as usize),
                    output.as_mut_ptr(),
                    pitch,
                );
            }
            self.immediate.Unmap(staging, 0);
        }
        Ok(pixels)
    }
}
