use std::sync::Arc;
use zipzi_core::DrawableFrame;

pub const MIN_ZOOM: f32 = 0.04;
pub const MAX_ZOOM: f32 = 32.0;

/// Decoded source pixels owned by the document, shared with render workers.
#[derive(Clone, Debug)]
pub struct SceneTexture {
    pub asset_id: String,
    /// Identifies immutable pixels, including PSD drafts without encoded PNGs.
    pub cache_key: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

/// A frame and its resources remain valid after editing or closing the source document.
#[derive(Clone, Debug)]
pub struct SceneSnapshot {
    pub model_opacity: f32,
    /// Runtime evaluation, unaffected by editor visibility; suitable for export.
    pub runtime_frame: Arc<DrawableFrame>,
    /// Viewport color output. Mask inputs and evaluated deformation stay intact.
    pub viewport_frame: Arc<DrawableFrame>,
    pub textures: Arc<[SceneTexture]>,
}

impl SceneSnapshot {
    pub(crate) fn with_manual_reference(
        mut self,
        target: &crate::ManualMeshTarget,
        preview_result: bool,
    ) -> Self {
        if preview_result {
            return self;
        }
        let mut frame = self.viewport_frame.as_ref().clone();
        let mut reference = zipzi_core::Drawable {
            id: format!("__manual_mesh_reference_{}", target.object.id),
            texture_asset_id: target.texture.asset_id.clone(),
            ..Default::default()
        };
        reference.runtime_id = reference.id.clone();
        let corners = [
            zipzi_core::Vec2::new(0., 0.),
            zipzi_core::Vec2::new(1., 0.),
            zipzi_core::Vec2::new(1., 1.),
            zipzi_core::Vec2::new(0., 1.),
        ];
        reference.positions = corners
            .iter()
            .map(|&uv| target.mapping.position(uv))
            .collect();
        reference.uvs = corners.into();
        reference.indices = vec![0, 1, 2, 0, 2, 3].into();
        // The edit plane has one image and no model-space compositing stack.
        frame.render_plan = vec![zipzi_core::evaluation::RenderCommand::DrawMesh {
            mesh_id: reference.id.clone(),
        }];
        frame.drawables = vec![reference];
        self.model_opacity = 1.;
        self.viewport_frame = Arc::new(frame);
        self
    }

    pub fn render_frame(&self) -> anyhow::Result<Arc<DrawableFrame>> {
        if self.model_opacity == 1. {
            return Ok(self.viewport_frame.clone());
        }
        let mut frame = self.viewport_frame.as_ref().clone();
        zipzi_sdk::apply_model_opacity(&mut frame, self.model_opacity)?;
        Ok(Arc::new(frame))
    }
}

impl crate::Document {
    /// Clone only when editor preferences change color output. In particular, a
    /// hidden mask source retains geometry, texture and opacity for mask passes.
    pub(crate) fn viewport_frame(&self, frame: &Arc<DrawableFrame>) -> Arc<DrawableFrame> {
        use crate::{ObjectKind as K, ObjectRef};
        let index = self.object_state_index();
        let hidden_meshes: Vec<_> = frame
            .drawables
            .iter()
            .enumerate()
            .filter(|(_, mesh)| {
                mesh.visible && !index.state(&ObjectRef::new(K::Mesh, &mesh.id)).visible()
            })
            .map(|(i, _)| i)
            .collect();
        let hidden_groups: Vec<_> = frame
            .offscreens
            .iter()
            .enumerate()
            .filter(|(_, group)| {
                group.enabled
                    && !index
                        .state(&ObjectRef::new(K::Offscreen, &group.id))
                        .visible()
            })
            .map(|(i, _)| i)
            .collect();
        if hidden_meshes.is_empty() && hidden_groups.is_empty() {
            return frame.clone();
        }
        let mut viewport = frame.as_ref().clone();
        for i in hidden_meshes {
            viewport.drawables[i].visible = false;
        }
        // Keep the composite in the render plan: hiding must never bypass it.
        for i in hidden_groups {
            viewport.offscreens[i].enabled = false;
        }
        Arc::new(viewport)
    }
}

/// Editor viewport state. Never saved to the model or recorded in SDK history.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    /// Physical display pixels per source pixel; 1.0 is actual pixel size (100%).
    pub zoom: f32,
    pub pan: crate::ViewportPoint,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: crate::ViewportPoint::default(),
        }
    }
}
