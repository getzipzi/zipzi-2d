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
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Organizational Part owner ID; independent of the deformation parent. An empty string
    /// represents the root/unassigned container.
    pub part_id: String,
    /// Original format-specific blend-mode code; None means no preserved raw code.
    pub raw_blend_mode: Option<u32>,
    /// Resolved image-asset object ID used by this drawable; separate from its evaluated
    /// texture slot.
    pub texture_asset_id: String,
    /// Resolved texture slot index; separate from the persistent texture asset ID.
    pub texture_slot: i32,
    /// Geometry remains editable when this resource cannot contribute color.
    #[serde(default = "renderable_default")]
    pub renderable: bool,
    /// Final evaluated XY vertex positions in Y-up model units; ordered for indices and UVs.
    pub positions: Vec<Vec2>,
    /// Texture UV pairs ordered by vertex_ids; keep one UV for each position.
    pub uvs: Arc<[Vec2]>,
    /// Flattened triangle indices into evaluated positions/UVs; these are offsets, not
    /// authoring vertex IDs.
    pub indices: Arc<[u32]>,
    /// Evaluated integer drawing-order coordinate before final render sorting.
    pub draw_order: i32,
    /// Final evaluated drawing sequence rank after hierarchy and group ordering.
    pub render_order: i32,
    /// Finite nonnegative opacity factor; 0 is transparent and 1 is opaque. Imported values
    /// above 1 are retained without clamping.
    pub opacity: f32,
    /// Evaluated RGBA multiply factors, channel order R, G, B, A.
    pub multiply_color: [f32; 4],
    /// Evaluated RGBA screen factors, channel order R, G, B, A.
    pub screen_color: [f32; 4],
    /// Compositing mode used when drawing this object.
    pub blend_mode: BlendMode,
    /// Whether this object participates in evaluation; editor hiding is a separate state.
    pub enabled: bool,
    /// Whether the evaluated object is visible after hierarchical visibility rules.
    pub visible: bool,
    /// Whether both triangle faces contribute to drawing.
    pub double_sided: bool,
    /// Whether to invert the combined clipping mask for this drawable.
    pub inverted_mask: bool,
    /// Ordered stable mesh IDs used as clipping-mask sources, not texture IDs.
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
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Requested parameter value before range/repeat handling, in the parameter domain.
    pub requested: f32,
    /// Effective parameter value after native range/repeat handling, in the parameter domain.
    pub value: f32,
    /// Whether the effective parameter value differs due to range clamping.
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
    BeginOffscreen {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
    },
    DrawMesh {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
    },
    EndOffscreen {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
    },
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
    /// Whether required content is available for rendering; unavailable textures do not make
    /// geometry uneditable.
    pub renderable: bool,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Stable Part ID whose contents are composited by this Offscreen.
    pub owner_part_id: String,
    /// Enclosing Offscreen ID; None means output is composited directly to the main target.
    pub parent_offscreen_id: Option<String>,
    /// Final evaluated drawing sequence rank after hierarchy and group ordering.
    pub render_order: i32,
    /// Finite nonnegative opacity factor; 0 is transparent and 1 is opaque. Imported values
    /// above 1 are retained without clamping.
    pub opacity: f32,
    /// Whether this object participates in evaluation; editor hiding is a separate state.
    pub enabled: bool,
    /// Compositing mode used when drawing this object.
    pub blend_mode: u32,
    /// Preserved format-specific Offscreen compositing flags.
    pub flags: u8,
    /// Ordered stable mesh IDs used as clipping-mask sources, not texture IDs.
    pub masks: Vec<String>,
    /// Evaluated RGBA multiply factors, channel order R, G, B, A.
    pub multiply_color: [f32; 4],
    /// Evaluated RGBA screen factors, channel order R, G, B, A.
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
    /// Requested/effective/clamped parameter records for this evaluated pose.
    pub parameters: Vec<EvaluatedParameter>,
    /// Evaluated drawable records in this pose, including their immutable geometry and appearance.
    pub drawables: Vec<Drawable>,
    /// Evaluated Offscreen compositing records for this pose.
    pub offscreens: Vec<OffscreenFrame>,
    /// Ordered render commands; Offscreen begin/end delimit the compositing scopes.
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
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Whether this object participates in evaluation; editor hiding is a separate state.
    pub enabled: bool,
    /// Final model coordinates (X-right/Y-up), matching
    /// Drawable::positions. Warp points are row-major; rotation points are the
    /// transformed origin and unit X/Y axes.
    pub points: Vec<Vec2>,
    /// Unit direction of the rotation's local X axis in model space, evaluated
    /// without translation or scale so tiny imported scales retain their angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_axis: Option<Vec2>,
    /// Local X/Y basis vectors with evaluated scale and reflections, before
    /// translation. Unlike subtracting points, these retain small rotation
    /// coefficients at large model origins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_basis: Option<[Vec2; 2]>,
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
