#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct Part {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Parent object ID in this hierarchy; independent of storage and drawing order. An empty
    /// string represents the root/unassigned container.
    pub parent_id: String,
    /// Whether this object participates in evaluation; editor hiding is a separate state.
    pub enabled: bool,
    /// Drawing-order coordinate; distinct from the final evaluated render_order.
    pub draw_order: f32,
}

impl Default for Part {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 0.0,
        }
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct WarpKeyform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional Bezier controller for this warp shape; None means this shape has no controller.
    pub bezier: Option<crate::warp_bezier::WarpBezier>,
    /// Import-only numeric coordinates; canonical documents derive these from samples.
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(default)]
    /// Stable sample IDs in active binding-axis order.
    pub samples: Vec<String>,
    /// Absolute warp lattice positions in row-major point order; not displacement vectors.
    pub positions: Vec<Vec2>,
    /// Opacity and multiply/screen color factors for this object or shape.
    pub appearance: Appearance,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct RotationKeyform {
    /// Import-only numeric coordinates; canonical documents derive these from samples.
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(default)]
    /// Stable sample IDs in active binding-axis order; not numeric coordinates.
    pub samples: Vec<String>,
    /// Rotation pose containing origin, angle in degrees, scale and axis reflections.
    pub rotation: RotationPose,
    /// Opacity and multiply/screen color factors for this object or shape.
    pub appearance: Appearance,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct PartKeyform {
    /// Import-only numeric coordinates; canonical documents derive these from samples.
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(default)]
    /// Stable sample IDs in active binding-axis order; not numeric coordinates.
    pub samples: Vec<String>,
    /// Drawing-order coordinate; distinct from the final evaluated render_order.
    pub draw_order: f32,
}
/// A single typed replacement; a track only accepts its matching variant.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum SceneKeyform {
    Warp(
        /// Complete Warp ordinary-binding keyform; samples identify the active axis combination.
        WarpKeyform,
    ),
    Rotation(
        /// Complete Rotation ordinary-binding keyform; samples identify the active axis
        /// combination.
        RotationKeyform,
    ),
    Part(
        /// Complete Part ordinary-binding keyform; samples identify the active axis combination.
        PartKeyform,
    ),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct Mesh {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Image-asset reference; resolved values name the asset object ID and unresolved values
    /// preserve an external texture identifier.
    pub texture_asset_id: crate::scene_reference::SceneReference,
    /// Stable u32 vertex IDs in the same order as positions and UVs; IDs need not be contiguous.
    pub vertex_ids: Vec<VertexId>,
    /// Absolute base-shape XY positions in the mesh authoring space, ordered by vertex_ids.
    pub base_positions: Vec<Vec2>,
    /// Texture UV pairs ordered by vertex_ids; keep one UV for each position.
    pub uvs: Vec<Vec2>,
    /// Triangle triples of stable vertex IDs; preserve winding and do not substitute position-
    /// array offsets.
    pub triangles: Vec<[VertexId; 3]>,
    /// Optional single-region modeling topology in UV space; independent of posed positions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meshing: Option<MeshMeshing>,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Organizational Part owner ID; independent of the deformation parent. An empty string
    /// represents the root/unassigned container.
    pub part_id: String,
    /// Parent deformation transform ID; an empty string means model space.
    pub deformer_id: String,
    /// Opacity and multiply/screen color factors for this object or shape.
    pub appearance: Appearance,
    /// Drawing-order coordinate; distinct from the final evaluated render_order. None leaves
    /// this channel unspecified for native evaluation.
    pub draw_order: Option<f32>,
    /// Compositing mode used when drawing this object.
    pub blend_mode: BlendMode,
    /// Whether this object participates in evaluation; editor hiding is a separate state.
    pub enabled: bool,
    /// Whether both triangle faces contribute to drawing.
    pub double_sided: bool,
    /// Whether to invert the combined clipping mask for this drawable.
    pub inverted_mask: bool,
    /// Ordered stable mesh IDs used as clipping-mask sources, not texture IDs.
    pub masks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Original format-specific blend-mode code; None means no preserved raw code.
    pub raw_blend_mode: Option<u32>,
}

impl Default for Mesh {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            texture_asset_id: crate::scene_reference::SceneReference::Unspecified,
            vertex_ids: Vec::new(),
            base_positions: Vec::new(),
            uvs: Vec::new(),
            triangles: Vec::new(),
            meshing: None,
            runtime_id: String::new(),
            part_id: String::new(),
            deformer_id: String::new(),
            appearance: Appearance::default(),
            draw_order: None,
            blend_mode: BlendMode::Normal,
            enabled: true,
            double_sided: true,
            inverted_mask: false,
            masks: Vec::new(),
            raw_blend_mode: None,
        }
    }
}
