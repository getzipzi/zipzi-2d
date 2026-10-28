//! D3D11 presentation contract. All context operations belong to the UI renderer.
use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::Graphics::Direct3D11::*;

/// Immutable until all scene references and both GPU timelines have retired.
#[derive(Debug)]
pub struct DirectXSurface {
    /// Device identity prevents stale frames being used after device recovery.
    pub device: ID3D11Device,
    /// Sampled view of the shared RGBA8 texture.
    pub view: ID3D11ShaderResourceView,
    /// Texture dimensions in physical pixels.
    pub extent: [u32; 2],
    /// Producer completion timeline.
    pub ready: ID3D11Fence,
    /// Producer submission corresponding to this immutable frame.
    pub ready_value: u64,
    /// Consumer completion timeline; signaled after every draw, including redraws.
    pub consumed: ID3D11Fence,
    /// Last reserved consumer submission; a failed signal must prevent reuse.
    pub consumed_value: AtomicU64,
}

impl DirectXSurface {
    /// Reserve a value before issuing consumer commands. Only the UI thread submits them.
    pub fn next_consumed_value(&self) -> u64 {
        self.consumed_value.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Nonblocking completion check. Device removal is not successful completion.
    pub fn is_idle(&self) -> bool {
        let ready = unsafe { self.ready.GetCompletedValue() };
        let consumed = unsafe { self.consumed.GetCompletedValue() };
        ready != u64::MAX
            && consumed != u64::MAX
            && ready >= self.ready_value
            && consumed >= self.consumed_value.load(Ordering::Acquire)
    }
}
