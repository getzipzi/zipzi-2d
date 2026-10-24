//! Isolated neutral candidate rendering. The live document is never edited.
use crate::{MeshGenerationPreview, SceneSnapshot, SceneTexture};
use std::{collections::HashMap, sync::Arc};
use zipzi_core::evaluation::RenderCommand;
use zipzi_core::{Drawable, DrawableFrame, Vec2};
impl MeshGenerationPreview {
    pub fn scene(&self, texture: SceneTexture) -> SceneSnapshot {
        let texture = self.render_texture(texture);
        let [a, c] = self.source_bounds();
        let g = self.geometry();
        let index: HashMap<_, _> = g
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i as u32))
            .collect();
        let id = "mesh-generation-preview".to_string();
        let drawable = Drawable {
            id: id.clone(),
            texture_asset_id: texture.asset_id.clone(),
            positions: g
                .positions
                .iter()
                .map(|p| Vec2::new(p.x - a.x, p.y - c.y))
                .collect(),
            // DrawableFrame uses runtime UVs (Y up), just like PreparedEvaluation.
            uvs: g.uvs.iter().copied().collect::<Vec<_>>().into(),
            indices: g
                .triangles
                .iter()
                .flat_map(|f| [index[&f[0]], index[&f[1]], index[&f[2]]])
                .collect::<Vec<_>>()
                .into(),
            ..Default::default()
        };
        let frame = Arc::new(DrawableFrame {
            drawables: vec![drawable],
            render_plan: vec![RenderCommand::DrawMesh { mesh_id: id }],
            ..Default::default()
        });
        SceneSnapshot {
            model_opacity: 1.,
            runtime_frame: frame.clone(),
            viewport_frame: frame,
            textures: Arc::from([texture]),
        }
    }
}
