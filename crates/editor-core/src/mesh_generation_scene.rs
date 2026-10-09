//! Isolated neutral candidate rendering. The live document is never edited.
use crate::{MeshGenerationPreview, SceneSnapshot, SceneTexture};
use kasane_core::evaluation::RenderCommand;
use kasane_core::{Canvas, Drawable, DrawableFrame, Vec2};
use std::{collections::HashMap, sync::Arc};
impl MeshGenerationPreview {
    pub fn scene(&self, texture: SceneTexture) -> SceneSnapshot {
        let [a, c] = self.source_bounds();
        let canvas = Canvas::new(c.x - a.x, c.y - a.y, Vec2::default(), 1.);
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
                .map(|p| Vec2::new(p.x - a.x, -(p.y - a.y)))
                .collect(),
            // DrawableFrame uses runtime UVs (Y up), just like PreparedEvaluation.
            uvs: g
                .uvs
                .iter()
                .map(|uv| Vec2::new(uv.x, 1. - uv.y))
                .collect::<Vec<_>>()
                .into(),
            indices: g
                .triangles
                .iter()
                .flat_map(|f| [index[&f[0]], index[&f[2]], index[&f[1]]])
                .collect::<Vec<_>>()
                .into(),
            ..Default::default()
        };
        let frame = Arc::new(DrawableFrame {
            canvas,
            drawables: vec![drawable],
            render_plan: vec![RenderCommand::DrawMesh { mesh_id: id }],
            ..Default::default()
        });
        SceneSnapshot {
            runtime_frame: frame.clone(),
            viewport_frame: frame,
            textures: Arc::from([texture]),
        }
    }
}
