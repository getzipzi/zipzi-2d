//! Pixel coverage sampling and hit order through masks and offscreen compositing.
use super::geometry::barycentric;
use super::{CanvasScene, PICK_ALPHA};
use crate::{ModelPoint, ViewportTransform};
use kasane_core::{Vec2, evaluation::RenderCommand};
use std::collections::HashMap;

impl CanvasScene {
    fn raw_alpha(&self, slot: usize, p: ModelPoint) -> f32 {
        let d = &self.source.viewport_frame.drawables[slot];
        let Some(mesh) = self.mesh(&d.id) else {
            return 0.;
        };
        if mesh
            .bounds
            .is_none_or(|b| p.x < b[0] || p.y < b[1] || p.x > b[2] || p.y > b[3])
        {
            return 0.;
        }
        let Some(texture) = self
            .texture_slots
            .get(&d.texture_asset_id)
            .map(|&i| &self.source.textures[i])
        else {
            return 0.;
        };
        let canvas = self.source.viewport_frame.canvas;
        let mut alpha: f32 = 0.;
        for indices in d.indices.chunks_exact(3) {
            let [a, b, c] = [
                indices[0] as usize,
                indices[1] as usize,
                indices[2] as usize,
            ];
            let Some((&pa, &pb, &pc)) = d
                .positions
                .get(a)
                .zip(d.positions.get(b))
                .zip(d.positions.get(c))
                .map(|((a, b), c)| (a, b, c))
            else {
                continue;
            };
            let Some(w) = barycentric(
                p,
                [pa, pb, pc].map(|p| ViewportTransform::evaluated_to_source(canvas, p)),
            ) else {
                continue;
            };
            if let (Some(a), Some(b), Some(c)) = (d.uvs.get(a), d.uvs.get(b), d.uvs.get(c)) {
                alpha = alpha.max(texture_alpha(
                    texture,
                    Vec2::new(
                        a.x * w[0] + b.x * w[1] + c.x * w[2],
                        a.y * w[0] + b.y * w[1] + c.y * w[2],
                    ),
                ));
            }
        }
        alpha
    }
    /// Follow the actual compositing plan. Flattening render_order loses group ordering.
    pub fn mesh_hits(&self, p: ModelPoint) -> Vec<String> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Vec::new();
        }
        let frame = &self.source.viewport_frame;
        let raw: HashMap<_, _> = frame
            .drawables
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.as_str(), self.raw_alpha(i, p)))
            .collect();
        let mask = |ids: &[String], inverted: bool| {
            if ids.is_empty() {
                return 1.;
            }
            let coverage = 1.
                - ids.iter().fold(1., |a, id| {
                    a * (1. - raw.get(id.as_str()).copied().unwrap_or(0.))
                });
            if inverted { 1. - coverage } else { coverage }
        };
        let by_id: HashMap<_, _> = frame.drawables.iter().map(|d| (d.id.as_str(), d)).collect();
        let offscreens: HashMap<_, _> = frame
            .offscreens
            .iter()
            .map(|d| (d.id.as_str(), d))
            .collect();
        let mut stack = vec![Vec::<(String, f32)>::new()];
        let fallback;
        let plan = if frame.render_plan.is_empty() {
            let mut draws: Vec<_> = frame.drawables.iter().enumerate().collect();
            draws.sort_by_key(|(i, d)| (d.render_order, *i));
            fallback = draws
                .into_iter()
                .map(|(_, d)| RenderCommand::DrawMesh {
                    mesh_id: d.id.clone(),
                })
                .collect::<Vec<_>>();
            &fallback
        } else {
            &frame.render_plan
        };
        for command in plan {
            match command {
                RenderCommand::BeginOffscreen { .. } => stack.push(Vec::new()),
                RenderCommand::DrawMesh { mesh_id } => {
                    if let Some(d) = by_id
                        .get(mesh_id.as_str())
                        .filter(|d| d.enabled && d.visible)
                    {
                        let a = raw.get(mesh_id.as_str()).copied().unwrap_or(0.)
                            * d.opacity.clamp(0., 1.)
                            * mask(&d.masks, d.inverted_mask);
                        if a > PICK_ALPHA {
                            stack.last_mut().unwrap().push((mesh_id.clone(), a));
                        }
                    }
                }
                RenderCommand::EndOffscreen { offscreen_id } => {
                    if stack.len() == 1 {
                        continue;
                    }
                    let group = stack.pop().unwrap();
                    if let Some(os) = offscreens
                        .get(offscreen_id.as_str())
                        .filter(|os| os.enabled)
                    {
                        let factor = os.opacity.clamp(0., 1.) * mask(&os.masks, os.flags & 8 != 0);
                        stack
                            .last_mut()
                            .unwrap()
                            .extend(group.into_iter().filter_map(|(id, a)| {
                                (a * factor > PICK_ALPHA).then_some((id, a * factor))
                            }));
                    }
                }
            }
        }
        stack
            .into_iter()
            .next()
            .unwrap_or_default()
            .into_iter()
            .rev()
            .map(|(id, _)| id)
            .collect()
    }
}
fn texture_alpha(t: &crate::SceneTexture, uv: Vec2) -> f32 {
    if t.width == 0 || t.height == 0 || !uv.x.is_finite() || !uv.y.is_finite() {
        return 0.;
    }
    let x = uv.x.clamp(0., 1.) * t.width as f32 - 0.5;
    // Drawable UVs are bottom-up; decoded RGBA rows are top-down. Match the
    // conversion in kasane_render::gpu::append_vertices_for for draws and masks.
    let y = (1. - uv.y).clamp(0., 1.) * t.height as f32 - 0.5;
    let sample = |x: i32, y: i32| {
        let i = ((y.clamp(0, t.height as i32 - 1) as u32 * t.width
            + x.clamp(0, t.width as i32 - 1) as u32)
            * 4
            + 3) as usize;
        t.rgba.get(i).copied().unwrap_or(0) as f32 / 255.
    };
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    (sample(ix, iy) * (1. - fx) + sample(ix + 1, iy) * fx) * (1. - fy)
        + (sample(ix, iy + 1) * (1. - fx) + sample(ix + 1, iy + 1) * fx) * fy
}
