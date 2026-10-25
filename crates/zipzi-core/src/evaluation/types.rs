use std::collections::HashMap;
use std::sync::Arc;

use crate::document::Document;
use crate::geometry::validate_positions;
use crate::types::{BlendMode, Status, Vec2};

pub type PreviewValues = HashMap<String, f32>;

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct Drawable {
    pub id: String,
    pub runtime_id: String,
    pub part_id: String,
    pub raw_blend_mode: Option<u32>,
    pub texture_asset_id: String,
    pub texture_slot: i32,
    /// Geometry remains editable when this resource cannot contribute color.
    #[serde(default = "renderable_default")]
    pub renderable: bool,
    pub positions: Vec<Vec2>,
    pub uvs: Arc<[Vec2]>,
    pub indices: Arc<[u32]>,
    pub draw_order: i32,
    pub render_order: i32,
    pub opacity: f32,
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
    pub blend_mode: BlendMode,
    pub enabled: bool,
    pub visible: bool,
    pub double_sided: bool,
    pub inverted_mask: bool,
    pub masks: Vec<String>,
}

impl Default for Drawable {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            part_id: String::new(),
            raw_blend_mode: None,
            texture_asset_id: String::new(),
            texture_slot: 0,
            renderable: true,
            positions: Vec::new(),
            uvs: Arc::from([]),
            indices: Arc::from([]),
            draw_order: 0,
            render_order: 0,
            opacity: 1.0,
            multiply_color: [1.0, 1.0, 1.0, 1.0],
            screen_color: [0.0, 0.0, 0.0, 1.0],
            blend_mode: BlendMode::Normal,
            enabled: true,
            visible: true,
            double_sided: true,
            inverted_mask: false,
            masks: Vec::new(),
        }
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct EvaluatedParameter {
    pub id: String,
    pub requested: f32,
    pub value: f32,
    pub clamped: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub enum RenderCommand {
    BeginOffscreen { offscreen_id: String },
    DrawMesh { mesh_id: String },
    EndOffscreen { offscreen_id: String },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct OffscreenFrame {
    #[serde(default = "renderable_default")]
    pub renderable: bool,
    pub id: String,
    pub runtime_id: String,
    pub owner_part_id: String,
    pub parent_offscreen_id: Option<String>,
    pub render_order: i32,
    pub opacity: f32,
    pub enabled: bool,
    pub blend_mode: u32,
    pub flags: u8,
    pub masks: Vec<String>,
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct DrawableFrame {
    /// Revision used to evaluate this immutable snapshot; later name edits may reuse it.
    pub source_revision: u64,
    pub parameters: Vec<EvaluatedParameter>,
    pub drawables: Vec<Drawable>,
    pub offscreens: Vec<OffscreenFrame>,
    pub render_plan: Vec<RenderCommand>,
    /// Editor controls evaluated in the same pose as the drawables.
    pub deformers: Vec<EvaluatedDeformer>,
}

impl DrawableFrame {
    /// Tight AABB of evaluated vertices in Y-up model units, without export padding.
    /// This is geometry, not an alpha/mask silhouette or a collision shape.
    /// To include invisible geometry, obtain the frame using
    /// `evaluate_frame_including_hidden` before passing `include_hidden = true`.
    pub fn model_bounds(&self, include_hidden: bool) -> Option<[f64; 4]> {
        let mut bounds = None::<[f64; 4]>;
        for d in &self.drawables {
            if !include_hidden && !(d.enabled && d.visible && d.opacity > 0.) {
                continue;
            }
            for p in &d.positions {
                let (x, y) = (f64::from(p.x), f64::from(p.y));
                bounds = Some(match bounds {
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                    None => [x, y, x, y],
                });
            }
        }
        bounds
    }
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct EvaluatedDeformer {
    pub id: String,
    pub enabled: bool,
    /// Final model coordinates (X-right/Y-up), matching
    /// Drawable::positions. Warp points are row-major; rotation points are the
    /// transformed origin and unit X/Y axes.
    pub points: Vec<Vec2>,
    /// Unit direction of the rotation's local X axis in model space, evaluated
    /// without translation or scale so tiny imported scales retain their angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_axis: Option<Vec2>,
}

pub fn parent_positions<'a>(
    doc: &Document,
    parent: &str,
    positions: &'a [Vec2],
) -> Result<&'a [Vec2], Status> {
    doc.coordinate_space(parent)?;
    let s = validate_positions(positions);
    if !s.is_ok() {
        return Err(s);
    }
    Ok(positions)
}

pub fn parent_origin(
    doc: &Document,
    parent: &str,
    origin: crate::types::PreciseVec2,
) -> Result<Vec2, Status> {
    doc.coordinate_space(parent)?;
    let p = Vec2::new(origin.x as f32, origin.y as f32);
    let status = validate_positions(&[p]);
    if !status.is_ok() {
        return Err(status);
    }
    Ok(p)
}

fn renderable_default() -> bool {
    true
}

impl Default for OffscreenFrame {
    fn default() -> Self {
        Self {
            renderable: true,
            id: String::new(),
            runtime_id: String::new(),
            owner_part_id: String::new(),
            parent_offscreen_id: None,
            render_order: 0,
            opacity: 0.,
            enabled: false,
            blend_mode: 0,
            flags: 0,
            masks: vec![],
            multiply_color: [0.; 4],
            screen_color: [0.; 4],
        }
    }
}
