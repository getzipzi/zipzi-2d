//! ScenePlan-driven cross-platform GPU renderer. Source textures store premultiplied RGBA.
mod api;
mod context;
mod encoding;
mod geometry;
mod mask;
mod modern;
mod pipeline;
mod surface;
mod textures;
pub use api::*;
pub use context::{output_texture, read_rgba8, upload_rgba8, WgpuContext};
pub use modern::{WgpuEncodeTarget, WgpuOutputMode, WgpuRenderStats, WgpuRenderer};
