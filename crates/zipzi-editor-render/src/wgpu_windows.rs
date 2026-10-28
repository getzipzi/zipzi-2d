//! D3D12 -> shared RGBA8 -> GPUI D3D11. Only this module uses native D3D12.
//! The wgpu source is transitioned through wgpu to COPY_SRC and remains there.
//! Shared destinations are native-only, COMMON between accesses. One worker
//! owns the wgpu queue and all native submissions; the UI owns the D3D11 context.
use anyhow::{Context as _, Result, anyhow, ensure};
use std::{
    mem::ManuallyDrop,
    sync::{Arc, atomic::Ordering},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_ALL, HANDLE},
        Graphics::{
            Direct3D11::*,
            Direct3D12::*,
            Dxgi::{Common::*, IDXGIDevice},
        },
    },
    core::{Interface, PCWSTR},
};
use zipzi_render_wgpu::WgpuContext;

pub fn windows_context(consumer: &ID3D11Device) -> Result<WgpuContext> {
    let dxgi: IDXGIDevice = consumer.cast()?;
    let luid = unsafe { dxgi.GetAdapter()?.GetDesc()?.AdapterLuid };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let mut future = std::pin::pin!(instance.enumerate_adapters(wgpu::Backends::DX12));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let adapters = loop {
        match std::future::Future::poll(future.as_mut(), &mut cx) {
            std::task::Poll::Ready(adapters) => break adapters,
            std::task::Poll::Pending => std::thread::sleep(std::time::Duration::from_millis(1)),
        }
    };
    for adapter in adapters {
        let matches = unsafe {
            adapter
                .as_hal::<wgpu::hal::api::Dx12>()
                .and_then(|a| a.raw_adapter().GetDesc1().ok())
                .is_some_and(|d| d.AdapterLuid == luid)
        };
        if matches {
            return WgpuContext::from_adapter(adapter)
                .map_err(|e| anyhow!("{}: {}", e.code, e.message));
        }
    }
    Err(anyhow!("GPUI adapter has no matching wgpu D3D12 adapter"))
}

struct SharedHandle(HANDLE);
impl Drop for SharedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct CopyJob {
    _destination: ID3D12Resource,
    _source: wgpu::Texture,
    _allocator: ID3D12CommandAllocator,
    _commands: ID3D12GraphicsCommandList,
}

struct Slot {
    image: Arc<gpui::DirectXSurface>,
    resource: ID3D12Resource,
    ready: ID3D12Fence,
    consumed: ID3D12Fence,
    job: Option<CopyJob>,
}
impl Slot {
    fn reusable(&self) -> bool {
        Arc::strong_count(&self.image) == 1 && self.image.is_idle()
    }
    fn bytes(&self) -> u64 {
        u64::from(self.image.extent[0]) * u64::from(self.image.extent[1]) * 4
    }
}

/// Worker-local, bounded pool. A retained scene/candidate pins its immutable slot.
/// A full pool applies backpressure before rendering another model frame.
pub struct WindowsPresenter {
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    consumer: ID3D11Device,
    slots: Vec<Slot>,
    // Keep wgpu's native backend/library alive until every native COM object above
    // has been released, including when the caller drops its renderer first.
    _wgpu_device: wgpu::Device,
    _wgpu_queue: wgpu::Queue,
}
impl WindowsPresenter {
    pub const MAX_SLOTS: usize = 8;
    pub const MAX_BYTES: u64 = 256 * 1024 * 1024;

    pub fn new(context: &WgpuContext, consumer: ID3D11Device) -> Result<Self> {
        let hal = unsafe { context.device().as_hal::<wgpu::hal::api::Dx12>() }
            .context("Windows presentation requires D3D12")?;
        let _: ID3D11Device5 = consumer.cast().context("D3D11 shared fences unavailable")?;
        let dxgi: IDXGIDevice = consumer.cast()?;
        ensure!(
            unsafe { dxgi.GetAdapter()?.GetDesc()?.AdapterLuid }
                == unsafe { hal.raw_device().GetAdapterLuid() },
            "D3D11 and D3D12 adapters differ"
        );
        Ok(Self {
            device: hal.raw_device().clone(),
            queue: hal.raw_queue().clone(),
            consumer,
            slots: Vec::new(),
            _wgpu_device: context.device().clone(),
            _wgpu_queue: context.queue().clone(),
        })
    }

    /// Select a free slot without a GPU/CPU wait. None means the caller should
    /// coalesce pending requests and retry, never publish an error/blank frame.
    pub fn acquire(&mut self, extent: [u32; 2]) -> Result<Option<usize>> {
        ensure!(
            extent.into_iter().all(|v| v > 0 && v <= 16384),
            "Invalid shared preview extent"
        );
        let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 4;
        ensure!(
            extent.into_iter().all(|v| v > 0) && bytes <= Self::MAX_BYTES,
            "Shared preview exceeds presentation budget"
        );
        unsafe {
            self.device.GetDeviceRemovedReason()?;
            self.consumer.GetDeviceRemovedReason()?;
        }
        for slot in &mut self.slots {
            let ready = unsafe { slot.ready.GetCompletedValue() };
            if ready != u64::MAX && ready >= slot.image.ready_value {
                slot.job = None;
            }
        }
        if let Some(index) = self
            .slots
            .iter()
            .position(|s| s.image.extent == extent && s.reusable())
        {
            self.slots[index].job = None;
            return Ok(Some(index));
        }
        // Retire unused old sizes, but never discard command allocators in flight.
        self.slots.retain(|s| !s.reusable());
        if self.slots.len() >= Self::MAX_SLOTS
            || self.slots.iter().map(Slot::bytes).sum::<u64>() + bytes > Self::MAX_BYTES
        {
            return Ok(None);
        }
        let slot = self.create_slot(extent)?;
        self.slots.push(slot);
        Ok(Some(self.slots.len() - 1))
    }

    fn create_slot(&self, extent: [u32; 2]) -> Result<Slot> {
        // Committed resources, single mip/sample and RGBA8 are the conservative
        // D3D11/12 sharing contract. The actual open/SRV creation is the capability probe.
        let mut resource = None;
        unsafe {
            self.device.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_SHARED,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                    Width: extent[0] as u64,
                    Height: extent[1],
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
                    Flags: D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS
                        | D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut resource,
            )?;
        }
        let resource: ID3D12Resource = resource.context("shared texture")?;
        let handle = SharedHandle(unsafe {
            self.device
                .CreateSharedHandle(&resource, None, GENERIC_ALL.0, PCWSTR::null())?
        });
        let device1: ID3D11Device1 = self.consumer.cast()?;
        let texture: ID3D11Texture2D = unsafe { device1.OpenSharedResource1(handle.0)? };
        let mut view = None;
        unsafe {
            self.consumer
                .CreateShaderResourceView(&texture, None, Some(&mut view))?;
        }
        let (ready, ready11) = self.shared_fence()?;
        let (consumed, consumed11) = self.shared_fence()?;
        Ok(Slot {
            image: Arc::new(gpui::DirectXSurface {
                device: self.consumer.clone(),
                view: view.context("shared SRV")?,
                extent,
                ready: ready11,
                ready_value: 0,
                consumed: consumed11,
                consumed_value: 0.into(),
            }),
            resource,
            ready,
            consumed,
            job: None,
        })
    }

    fn shared_fence(&self) -> Result<(ID3D12Fence, ID3D11Fence)> {
        let fence: ID3D12Fence = unsafe { self.device.CreateFence(0, D3D12_FENCE_FLAG_SHARED)? };
        let handle = SharedHandle(unsafe {
            self.device
                .CreateSharedHandle(&fence, None, GENERIC_ALL.0, PCWSTR::null())?
        });
        let device5: ID3D11Device5 = self.consumer.cast()?;
        let mut opened = None;
        unsafe {
            device5.OpenSharedFence(handle.0, &mut opened)?;
        }
        Ok((fence, opened.context("shared fence")?))
    }

    /// Submit one native GPU copy after the model's wgpu submission. No readback,
    /// conversion or completion wait; the D3D11 renderer waits on the ready fence.
    pub fn publish(
        &mut self,
        context: &WgpuContext,
        index: usize,
        source: wgpu::Texture,
    ) -> Result<Arc<gpui::DirectXSurface>> {
        let device = unsafe { context.device().as_hal::<wgpu::hal::api::Dx12>() }
            .context("Presentation context must use D3D12")?;
        ensure!(
            *device.raw_device() == self.device,
            "Presentation context device changed"
        );
        drop(device);
        let slot = self
            .slots
            .get_mut(index)
            .context("Invalid shared texture slot")?;
        ensure!(slot.reusable(), "Shared texture still in use");
        ensure!(
            source.format() == wgpu::TextureFormat::Rgba8Unorm
                && source.dimension() == wgpu::TextureDimension::D2
                && source.depth_or_array_layers() == 1
                && source.sample_count() == 1
                && source.usage().contains(wgpu::TextureUsages::COPY_SRC)
                && [source.width(), source.height()] == slot.image.extent,
            "Shared preview format/extent mismatch"
        );
        let image = Arc::get_mut(&mut slot.image).context("Shared frame is retained")?;
        let allocator: ID3D12CommandAllocator = unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?
        };
        let commands: ID3D12GraphicsCommandList = unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?
        };
        let raw_source =
            unsafe { source.as_hal::<wgpu::hal::api::Dx12>() }.context("D3D12 source")?;
        let mut source_device: Option<ID3D12Device> = None;
        unsafe {
            raw_source.raw_resource().GetDevice(&mut source_device)?;
        }
        ensure!(
            source_device.as_ref() == Some(&self.device),
            "Source texture belongs to another device"
        );
        // SAFETY: state transitions for the wgpu-owned source go through its tracker.
        let mut encoder = context.device().create_command_encoder(&Default::default());
        encoder.transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &source,
                selector: None,
                state: wgpu::TextureUses::COPY_SRC,
            }),
        );
        unsafe {
            barrier(
                &commands,
                &slot.resource,
                D3D12_RESOURCE_STATE_COMMON,
                D3D12_RESOURCE_STATE_COPY_DEST,
            );
            let mut dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(slot.resource.clone())),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                ..Default::default()
            };
            let mut src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(raw_source.raw_resource().clone())),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                ..Default::default()
            };
            commands.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            ManuallyDrop::drop(&mut dst.pResource);
            ManuallyDrop::drop(&mut src.pResource);
            barrier(
                &commands,
                &slot.resource,
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_COMMON,
            );
            commands.Close()?;
        }
        drop(raw_source);
        let list: ID3D12CommandList = commands.cast()?;
        context.queue().submit([encoder.finish()]);
        image.ready_value += 1;
        // Retain all native command resources before executing, including error paths.
        slot.job = Some(CopyJob {
            _destination: slot.resource.clone(),
            _source: source,
            _allocator: allocator,
            _commands: commands,
        });
        unsafe {
            self.queue
                .Wait(&slot.consumed, image.consumed_value.load(Ordering::Acquire))?;
            self.queue.ExecuteCommandLists(&[Some(list)]);
            self.queue.Signal(&slot.ready, image.ready_value)?;
        }
        Ok(slot.image.clone())
    }
}

// Native command allocators must survive submission, even if a dialog closes
// on the UI thread before its candidate finishes. Keep backend/library ownership
// last in declaration order so native COM objects are released first.
struct Retirement {
    slots: Vec<Slot>,
    _queue: ID3D12CommandQueue,
    _device: wgpu::Device,
    _wgpu_queue: wgpu::Queue,
}
impl Drop for WindowsPresenter {
    fn drop(&mut self) {
        let complete = |slot: &Slot| {
            let value = unsafe { slot.ready.GetCompletedValue() };
            slot.job.is_none() || value == u64::MAX || value >= slot.image.ready_value
        };
        if self.slots.iter().all(complete) {
            return;
        }
        let retirement = Arc::new(Retirement {
            slots: std::mem::take(&mut self.slots),
            _queue: self.queue.clone(),
            _device: self._wgpu_device.clone(),
            _wgpu_queue: self._wgpu_queue.clone(),
        });
        let worker = retirement.clone();
        let spawned = std::thread::Builder::new()
            .name("zipzi-gpu-retirement".into())
            .spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while !worker.slots.iter().all(complete) {
                    if std::time::Instant::now() >= deadline {
                        // Failed signals/device hangs cannot justify freeing GPU-visible
                        // memory. Retain this bounded pool rather than cause a use-after-free.
                        eprintln!(
                            "GPU retirement timed out; retaining in-flight preview resources"
                        );
                        std::mem::forget(worker);
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });
        if spawned.is_err() {
            eprintln!("GPU retirement thread unavailable; retaining in-flight preview resources");
            std::mem::forget(retirement);
        }
    }
}

unsafe fn barrier(
    commands: &ID3D12GraphicsCommandList,
    resource: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) {
    let mut barrier = D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(resource.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
        ..Default::default()
    };
    unsafe {
        commands.ResourceBarrier(std::slice::from_ref(&barrier));
        ManuallyDrop::drop(&mut (*barrier.Anonymous.Transition).pResource);
    }
}
