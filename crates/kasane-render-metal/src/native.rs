//! Native Metal API and implementation modules.
mod context;
mod encoder;
mod geometry;
mod mask;
mod pipeline;
mod renderer;
mod surface;
mod textures;

use metal::MTLPixelFormat;

pub use context::{read_rgba8, MetalContext, MetalInitError};
pub use renderer::MetalRenderer;
pub use textures::{MetalTexture, MetalTextureCatalog};

/// Only four byte per pixel color formats are supported by the render path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetalTargetConfig {
    pub width: u32,
    pub height: u32,
    pub format: MTLPixelFormat,
}

impl Default for MetalTargetConfig {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            format: MTLPixelFormat::RGBA8Unorm,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetalOutputMode {
    #[default]
    Replace,
    Composite,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetalRenderStats {
    pub draw_calls: usize,
    pub render_passes: usize,
    pub masks: usize,
    pub mask_cache_hits: usize,
    pub buffer_uploads: usize,
    pub active_surfaces: usize,
    pub destination_copies: usize,
    /// Per-command color/snapshot storage, excluding the host output and masks.
    pub color_attachment_bytes: u64,
    /// Conservative per-command attachment reservation, including host output,
    /// mask storage and atlas packing overhead. Never exceeds the shared budget.
    pub attachment_budget_bytes: u64,
    /// Longest mask edge allowed after budget-driven resolution reduction.
    pub mask_resolution_limit: u32,
}
