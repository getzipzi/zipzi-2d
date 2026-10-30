//! Platform dependency boundary for the optional observation GPU feature.
//! Re-exports do not create a device or allocate GPU resources.

#[cfg(target_os = "macos")]
pub use zipzi_render_metal as metal;
#[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
pub use zipzi_render_wgpu as wgpu;
