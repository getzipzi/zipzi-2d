//! Platform adapters for immutable Editor scene snapshots.
#[cfg(target_os = "macos")]
mod metal;
#[cfg(target_os = "macos")]
pub use metal::*;
#[cfg(target_os = "macos")]
mod overlay;

#[cfg(feature = "wgpu-backend")]
mod wgpu;
#[cfg(feature = "wgpu-backend")]
pub use wgpu::{WgpuSceneRenderer, WgpuSubmittedFrame, WgpuTargetConfig};
#[cfg(feature = "wgpu-backend")]
mod wgpu_readback;
#[cfg(feature = "wgpu-backend")]
pub use wgpu_readback::WgpuImageReadback;
#[cfg(all(feature = "wgpu-backend", target_os = "macos"))]
mod wgpu_macos;
#[cfg(all(feature = "wgpu-backend", target_os = "macos"))]
pub use wgpu_macos::WgpuMetalSceneRenderer;

/// Shared screen-space radius for filled and hollow editing markers.
pub const OVERLAY_MARKER_RADIUS: f32 = 3.;

/// Separate textures let the canvas insert control lines beneath all markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayPass {
    All,
    Edges,
    Vertices,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayStyle {
    /// Straight RGBA; shaders produce premultiplied output.
    pub edge: [f32; 4],
    pub vertex: [f32; 4],
    pub selected: [f32; 4],
    pub restricted: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayViewport {
    pub extent: [u32; 2],
    pub scale: f32,
    /// Logical window origin; the texture starts at floor(origin * scale).
    pub origin: [f32; 2],
}
