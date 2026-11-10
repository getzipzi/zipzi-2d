//! Native Windows rendering of backend-neutral Zipzi scenes.
#![cfg(target_os = "windows")]

mod context;
mod encoding;
mod renderer;
mod resources;

pub use context::D3d11Context;
pub use renderer::D3d11Renderer;
pub use resources::{compile_shader, upload_rgba8, D3d11Target, D3d11Texture, D3d11TextureCatalog};

/// A backend-neutral scene or attachment validation error.
#[derive(Debug)]
pub struct D3d11ValidationError(pub zipzi_core::Status);

impl std::fmt::Display for D3d11ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0.code, self.0.message)
    }
}

impl std::error::Error for D3d11ValidationError {}
