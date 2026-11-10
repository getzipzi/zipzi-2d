//! Platform dependency boundary for the optional observation GPU feature.
//! Re-exports do not create a device or allocate GPU resources.

#[cfg(target_os = "windows")]
pub use zipzi_render_d3d11 as d3d11;
#[cfg(target_os = "macos")]
pub use zipzi_render_metal as metal;
#[cfg(any(
    not(any(target_os = "macos", target_os = "windows")),
    feature = "wgpu-backend"
))]
pub use zipzi_render_wgpu as wgpu;
