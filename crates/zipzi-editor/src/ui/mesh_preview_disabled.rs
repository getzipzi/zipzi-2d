//! Keep CPU mesh generation available without requesting Metal resources.
use gpui_kit::*;
use std::sync::{Arc, Mutex};
use zipzi_editor_core::SceneSnapshot;

pub(super) type MeshPreviewCache = Arc<Mutex<()>>;
pub(super) struct MeshPreview;
impl MeshPreview {
    pub fn prepare(
        _: SceneSnapshot,
        source_size: [f32; 2],
        _: Option<crate::platform::GpuContext>,
        _: MeshPreviewCache,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            source_size.into_iter().all(|v| v.is_finite() && v > 0.),
            "Invalid preview extent"
        );
        Ok(Self)
    }
    pub fn paint(
        &mut self,
        _: Bounds<Pixels>,
        _: Bounds<Pixels>,
        _: &mut Window,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
