use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Status {
    pub code: String,
    pub message: String,
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: String::new(),
            message: String::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code.is_empty()
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

crate::api_record! {
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[derive(zipzi_api_derive::BinaryWire)]
pub struct Vec2 {
    /// Horizontal coordinate; its space and units are determined by the containing geometry or
    /// pose.
    pub x: f32,
    /// Vertical coordinate; its space and units are determined by the containing geometry or pose.
    pub y: f32,
}
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

pub type VertexId = u32;

/// Source-format provenance and independent output settings. Neither affects evaluation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ModelSetup {
    /// Optional imported canvas metadata; None means this model has no source-canvas provenance.
    pub source_canvas: Option<Canvas>,
    /// Export-frame settings; padding is applied when computing an exported frame.
    pub export: ExportSettings,
}
impl From<Canvas> for ModelSetup {
    fn from(canvas: Canvas) -> Self {
        Self {
            source_canvas: Some(canvas),
            export: ExportSettings {
                pixels_per_unit: canvas.pixels_per_unit,
                ..Default::default()
            },
        }
    }
}
impl ModelSetup {
    pub fn is_valid(self) -> bool {
        self.source_canvas.is_none_or(Canvas::is_valid) && self.export.is_valid()
    }
}

/// Cubism output policy. A model unit corresponds to one canvas pixel; PPU
/// converts those lengths into Cubism runtime units only at the file boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ExportSettings {
    /// Positive model-length to Cubism-runtime-unit conversion scale at the file boundary;
    /// model units themselves remain canvas-pixel lengths.
    pub pixels_per_unit: f32,
    /// Optional stable rectangle [left, bottom, right, top] in model units.
    /// Specified bounds bypass automatic padding; pixel edges round outwards.
    pub bounds: Option<[f32; 4]>,
    /// Fractional export padding relative to the frame extent; dimensionless.
    pub padding_fraction: f32,
    /// Additional export padding measured in pixels.
    pub padding_pixels: f32,
    /// Finite nonnegative minimum automatic export extent in model units; native default is 256.
    pub minimum_extent: f32,
}
impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            pixels_per_unit: 1.,
            bounds: None,
            padding_fraction: 0.1,
            padding_pixels: 50.,
            minimum_extent: 256.,
        }
    }
}
impl ExportSettings {
    pub fn is_valid(self) -> bool {
        self.pixels_per_unit.is_finite()
            && self.pixels_per_unit > 0.
            && [
                self.padding_fraction,
                self.padding_pixels,
                self.minimum_extent,
            ]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.)
            && self.bounds.is_none_or(|[l, b, r, t]| {
                [l, b, r, t].into_iter().all(f32::is_finite) && r > l && t > b
            })
    }
}

/// Model and rotation-local lengths are X-right/Y-up. Warp parameters
/// have different dimensions, even though both are represented by Vec2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    Model,
    RotationLocal,
    WarpParameter,
}

fn default_canvas_flag() -> u8 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
/// A finite rectangle carried by an import/export format, not the model space.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct Canvas {
    /// Source canvas width in pixels.
    pub width: f32,
    /// Source canvas height in pixels.
    pub height: f32,
    /// Source canvas origin in the imported pixel coordinate system; this metadata does not
    /// redefine model space.
    pub origin: Vec2,
    /// Positive pixels-per-unit value retained from the source format for import/export;
    /// this metadata does not rescale model geometry or change evaluation.
    pub pixels_per_unit: f32,
    #[serde(default = "default_canvas_flag")]
    /// Preserved imported source-canvas flag; no implicit interpretation is applied by this DTO.
    pub flag: u8,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            origin: Vec2::default(),
            pixels_per_unit: 1.0,
            flag: 1,
        }
    }
}

impl Canvas {
    pub fn is_valid(self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && self.origin.x.is_finite()
            && self.origin.y.is_finite()
            && self.pixels_per_unit.is_finite()
            && self.pixels_per_unit > 0.0
    }

    pub fn new(width: f32, height: f32, origin: Vec2, pixels_per_unit: f32) -> Self {
        Self {
            width,
            height,
            origin,
            pixels_per_unit,
            flag: 1,
        }
    }

    pub fn with_flag(
        width: f32,
        height: f32,
        origin: Vec2,
        pixels_per_unit: f32,
        flag: u8,
    ) -> Self {
        Self {
            width,
            height,
            origin,
            pixels_per_unit,
            flag,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ImageAsset {
    /// Original imported path or external slot when no decoded content exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_source: Option<String>,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Image content metadata; None explicitly represents an asset with no loaded image content.
    pub content: Option<ImageContent>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ImageContent {
    /// Stored resource location, normally a package-relative path or generated memory-resource
    /// locator; reading metadata does not open it.
    pub source: String,
    /// Image width in pixels; prepared image content requires a positive dimension.
    pub width: u32,
    /// Image height in pixels; prepared image content requires a positive dimension.
    pub height: u32,
    /// Hexadecimal SHA-256 of the stored image content; an empty string means no recorded digest.
    pub sha256: String,
}
impl ImageContent {
    pub fn is_valid(&self) -> bool {
        !self.source.is_empty() && !self.source.contains('\0') && self.width > 0 && self.height > 0
    }
}
impl ImageAsset {
    pub fn image_content(&self) -> Result<&ImageContent, Status> {
        self.content
            .as_ref()
            .ok_or_else(|| match &self.unresolved_source {
                Some(source) => {
                    Status::error("MISSING_IMAGE_SOURCE", format!("{}: {source}", self.id))
                }
                None => Status::error("MISSING_IMAGE_CONTENT", &self.id),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum BlendMode {
    #[default]
    Normal,
    Additive,
    Multiplicative,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct Appearance {
    /// Finite nonnegative opacity factor; 0 is transparent and 1 is opaque. Imported values
    /// above 1 are retained without clamping.
    pub opacity: f32,
    /// RGB multiply factors in channel order R, G, B; (1, 1, 1) is neutral.
    pub multiply: [f32; 3],
    /// RGB screen factors in channel order R, G, B; (0, 0, 0) is neutral.
    pub screen: [f32; 3],
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            multiply: [1.0, 1.0, 1.0],
            screen: [0.0, 0.0, 0.0],
        }
    }
}

/// Authoring-space origins need f64: adding a pixel-space canvas origin to an
/// imported f32 runtime position must remain reversible before evaluation.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct PreciseVec2 {
    /// Horizontal coordinate; its space and units are determined by the containing geometry or
    /// pose.
    pub x: f64,
    /// Vertical coordinate; its space and units are determined by the containing geometry or pose.
    pub y: f64,
}

impl PreciseVec2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}
impl From<Vec2> for PreciseVec2 {
    fn from(p: Vec2) -> Self {
        Self::new(p.x as f64, p.y as f64)
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct RotationPose {
    /// Rotation origin in parent authoring coordinates, stored at f64 precision; default is (0, 0).
    pub origin: PreciseVec2,
    /// Rotation angle in degrees; not radians.
    pub angle: f32,
    /// Dimensionless rotation-transform scale; 1 is identity.
    pub scale: f32,
    /// Reflect the local X axis about the rotation origin when true.
    pub reflect_x: bool,
    /// Reflect the local Y axis about the rotation origin when true.
    pub reflect_y: bool,
}

impl Default for RotationPose {
    fn default() -> Self {
        Self {
            origin: PreciseVec2::default(),
            angle: 0.0,
            scale: 1.0,
            reflect_x: false,
            reflect_y: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransformKind {
    Warp,
    #[default]
    Rotation,
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
pub struct Transform {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Organizational Part owner ID; independent of the deformation parent. None selects the
    /// root/unassigned container.
    pub part_id: Option<PartId>,
    /// Parent object ID in this hierarchy; independent of storage and drawing order. None
    /// selects the root/unassigned container.
    pub parent_id: Option<TransformId>,
    /// Typed Warp or Rotation deformation definition; its variant determines lattice versus
    /// rotation-pose interpretation.
    pub data: TransformData,
    /// Whether this object participates in evaluation; editor hiding is a separate state.
    pub enabled: bool,
    /// Opacity and multiply/screen color factors for this object or shape.
    pub appearance: Appearance,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            part_id: None,
            parent_id: None,
            data: TransformData::default(),
            enabled: true,
            appearance: Appearance::default(),
        }
    }
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
pub enum TransformData {
    Warp(
        /// Warp deformation definition with its baked lattice and optional authoring controller.
         WarpTransform,
    ),
    Rotation(
        /// Rotation deformation definition with its reference angle and base pose.
        RotationTransform,
    ),
}
impl Default for TransformData {
    fn default() -> Self {
        Self::Rotation(RotationTransform::default())
    }
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
pub struct WarpTransform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional authoring Bezier controller; None leaves the baked lattice without a controller.
    pub bezier: Option<crate::WarpBezier>,
    /// Number of warp lattice cells vertically; a lattice has rows + 1 point rows.
    pub rows: u32,
    /// Number of warp lattice cells horizontally; a lattice has columns + 1 point columns.
    pub columns: u32,
    /// Whether to use the quad warp interpolation mode for the lattice.
    pub quad: bool,
    /// Absolute lattice points in row-major order; index = row * (columns + 1) + column.
    pub points: Vec<Vec2>,
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
pub struct RotationTransform {
    /// Reference angle in degrees used to orient the rotation control.
    pub base_angle: f32,
    /// Base rotation pose; its origin is expressed in the parent authoring space.
    pub pose: RotationPose,
}
impl Transform {
    /// Borrow a lookup key; an absent parent uses the root coordinate space.
    pub fn parent(&self) -> &str {
        self.parent_id.as_ref().map_or("", TransformId::as_str)
    }
    pub fn part(&self) -> &str {
        self.part_id.as_ref().map_or("", PartId::as_str)
    }
    pub fn kind(&self) -> TransformKind {
        match self.data {
            TransformData::Warp(_) => TransformKind::Warp,
            TransformData::Rotation(_) => TransformKind::Rotation,
        }
    }
    pub fn warp(&self) -> Option<&WarpTransform> {
        match &self.data {
            TransformData::Warp(w) => Some(w),
            _ => None,
        }
    }
    pub fn warp_mut(&mut self) -> Option<&mut WarpTransform> {
        match &mut self.data {
            TransformData::Warp(w) => Some(w),
            _ => None,
        }
    }
    pub fn rotation(&self) -> Option<&RotationTransform> {
        match &self.data {
            TransformData::Rotation(r) => Some(r),
            _ => None,
        }
    }
    pub fn rotation_mut(&mut self) -> Option<&mut RotationTransform> {
        match &mut self.data {
            TransformData::Rotation(r) => Some(r),
            _ => None,
        }
    }
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
    pub bezier: Option<crate::WarpBezier>,
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
/// Borrowed projection for interpolation and external formats. Neutral values
/// are synthesized, never stored in the document as irrelevant source fields.
pub struct SceneSample<'a> {
    pub keys: &'a [f32],
    pub positions: &'a [Vec2],
    pub rotation: RotationPose,
    pub appearance: Appearance,
    pub draw_order: f32,
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
    pub texture_asset_id: crate::SceneReference,
    /// Stable u32 vertex IDs in the same order as positions and UVs; IDs need not be contiguous.
    pub vertex_ids: Vec<VertexId>,
    /// Absolute base-shape XY positions in the mesh authoring space, ordered by vertex_ids.
    pub base_positions: Vec<Vec2>,
    /// Texture UV pairs ordered by vertex_ids; keep one UV for each position.
    pub uvs: Vec<Vec2>,
    /// Triangle triples of stable vertex IDs; preserve winding and do not substitute position-
    /// array offsets.
    pub triangles: Vec<[VertexId; 3]>,
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
            texture_asset_id: crate::SceneReference::Unspecified,
            vertex_ids: Vec::new(),
            base_positions: Vec::new(),
            uvs: Vec::new(),
            triangles: Vec::new(),
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

fn default_decimal_places() -> i32 {
    6
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ParameterKind {
    #[default]
    Normal,
    BlendShape,
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
pub struct Parameter {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Lower endpoint of the parameter domain, in model-defined parameter units.
    pub minimum: f32,
    /// Upper endpoint of the parameter domain, in the same units as minimum.
    pub maximum: f32,
    /// Default parameter value used when no override is supplied.
    pub default_value: f32,
    #[serde(default = "default_decimal_places")]
    /// Decimal precision used for runtime parameter handling; native default is 6.
    pub decimal_places: i32,
    #[serde(default)]
    /// Normal or BlendShape parameter behavior; native default is Normal.
    pub kind: ParameterKind,
    #[serde(default)]
    /// Whether parameter values repeat across the declared range; native default is false.
    pub repeat: bool,
}

impl Default for Parameter {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 6,
            kind: ParameterKind::Normal,
            repeat: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum BlendShapeTargetKind {
    Mesh,
    Warp,
    Rotation,
    Part,
    Glue,
    Offscreen,
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
pub struct BlendShapeKeyTable {
    #[serde(default)]
    /// Ordered stable parameter samples with strictly increasing finite values; base_sample_id
    /// names the neutral sample.
    pub samples: Vec<BindingSample>,
    #[serde(default)]
    /// Sample ID defining the neutral/base shape in this key table.
    pub base_sample_id: String,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(skip)]
    pub base_key_idx: usize,
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
pub struct BlendShapeConstraint {
    #[serde(default)]
    /// Ordered parameter samples with weights used to attenuate the BlendShape contribution.
    pub samples: Vec<ConstraintSample>,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(skip)]
    pub weights: Vec<f32>,
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
pub struct DeltaMeshKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Per-vertex XY displacement vectors in mesh vertex order.
    pub positions: Vec<Vec2>,
    /// Additive opacity delta; zero is neutral. None supplies no delta for this channel.
    pub opacity: Option<f32>,
    /// Additive drawing-order delta; zero is neutral. None supplies no delta for this channel.
    pub draw_order: Option<f32>,
    /// Additive RGB multiply-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub multiply: Option<[f32; 3]>,
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
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
pub struct DeltaWarpKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Per-point XY displacement vectors in row-major warp point order.
    pub points: Vec<Vec2>,
    /// Additive opacity delta; zero is neutral. None supplies no delta for this channel.
    pub opacity: Option<f32>,
    /// Additive RGB multiply-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub multiply: Option<[f32; 3]>,
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
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
pub struct DeltaRotationKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Translation delta of the rotation origin in parent coordinates. None supplies no delta
    /// for this channel.
    pub origin: Option<Vec2>,
    /// Additive rotation-angle delta in degrees. None supplies no delta for this channel.
    pub angle: Option<f32>,
    /// Additive scale delta; zero is neutral. None supplies no delta for this channel.
    pub scale: Option<f32>,
    /// Additive opacity delta; zero is neutral. None supplies no delta for this channel.
    pub opacity: Option<f32>,
    /// Additive RGB multiply-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub multiply: Option<[f32; 3]>,
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
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
pub struct DeltaPartKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Additive drawing-order delta; zero is neutral.
    pub draw_order: f32,
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
pub struct DeltaGlueKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Additive Glue intensity delta; zero is neutral.
    pub intensity: f32,
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
pub struct DeltaOffscreenKeyform {
    #[serde(default)]
    /// Stable sample ID in the referenced BlendShape key table, not a numeric parameter value.
    pub sample_id: String,
    /// Additive opacity delta; zero is neutral.
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// RGB multiply factors stored as multipliers with neutral (1, 1, 1); None supplies
    /// no multiply-color delta. Native conversion subtracts 1 before additive blending.
    pub multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Additive RGB screen-color delta; (0, 0, 0) is neutral. None supplies no delta for this
    /// channel.
    pub screen: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "items", rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum DeltaKeyforms {
    Mesh(
        /// Ordered Mesh BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
         Vec<DeltaMeshKeyform>,
    ),
    Warp(
        /// Ordered Warp BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
         Vec<DeltaWarpKeyform>,
    ),
    Rotation(
        /// Ordered Rotation BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
         Vec<DeltaRotationKeyform>,
    ),
    Part(
        /// Ordered Part BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
         Vec<DeltaPartKeyform>,
    ),
    Glue(
        /// Ordered Glue BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
         Vec<DeltaGlueKeyform>,
    ),
    Offscreen(
        /// Ordered Offscreen BlendShape displacement keyforms, each keyed by stable sample_id;
        /// channels describe deltas from the base shape.
         Vec<DeltaOffscreenKeyform>,
    ),
}

impl DeltaKeyforms {
    pub fn len(&self) -> usize {
        match self {
            DeltaKeyforms::Mesh(v) => v.len(),
            DeltaKeyforms::Warp(v) => v.len(),
            DeltaKeyforms::Rotation(v) => v.len(),
            DeltaKeyforms::Part(v) => v.len(),
            DeltaKeyforms::Glue(v) => v.len(),
            DeltaKeyforms::Offscreen(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
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
pub struct BlendShapeBinding {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable object ID driven by this binding; the declared target kind determines its domain.
    pub target_id: String,
    /// Category of the BlendShape target; must agree with target_id and keyform payload kind.
    pub target_kind: BlendShapeTargetKind,
    /// Stable BlendShape key-table ID controlling the binding.
    pub key_table_id: String,
    /// Ordered IDs of BlendShape constraints applied to this binding.
    pub constraint_ids: Vec<String>,
    /// Shape samples for this binding; coordinate/sample identities must match its axes or key
    /// table.
    pub keyforms: DeltaKeyforms,
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
pub struct GlueVertexPair {
    /// Stable UUID of this Glue pair; not the identity of its target.
    pub id: String,
    /// Vertex ID in endpoint mesh A; None explicitly leaves this endpoint unattached.
    pub vertex_a: Option<VertexId>,
    /// Vertex ID in endpoint mesh B; None explicitly leaves this endpoint unattached.
    pub vertex_b: Option<VertexId>,
    /// Dimensionless contribution of endpoint A; default Glue pair weight is 0.5.
    pub weight_a: f32,
    /// Dimensionless contribution of endpoint B; default Glue pair weight is 0.5.
    pub weight_b: f32,
}
impl Default for GlueVertexPair {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            vertex_a: None,
            vertex_b: None,
            weight_a: 0.5,
            weight_b: 0.5,
        }
    }
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
pub struct Glue {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Stable mesh ID for the A endpoint of this Glue.
    pub mesh_a_id: crate::SceneReference,
    /// Stable mesh ID for the B endpoint of this Glue.
    pub mesh_b_id: crate::SceneReference,
    /// Ordered Glue vertex pairs with stable pair identities and endpoint vertex IDs.
    pub pairs: Vec<GlueVertexPair>,
    /// Glue influence factor; 0 disables the connection and 1 applies its full influence.
    pub intensity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional parameter-driven Glue binding; None leaves the Glue intensity static.
    pub binding: Option<GlueBinding>,
}

impl Default for Glue {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            mesh_a_id: Default::default(),
            mesh_b_id: Default::default(),
            pairs: vec![],
            intensity: 1.,
            binding: None,
        }
    }
}

/// Ordinary parameter grid for Glue intensity, owned and edited atomically with its Glue.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct GlueBinding {
    /// Ordered parameter axes; keyform coordinates follow this order and samples retain their
    /// own identities.
    pub axes: Vec<BindingAxis>,
    /// First axis varies fastest, as in MOC3 ordinary bindings.
    pub keyforms: Vec<GlueKeyform>,
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
pub struct GlueKeyform {
    #[serde(default)]
    /// Stable sample IDs in active binding-axis order; not numeric coordinates.
    pub samples: Vec<String>,
    /// Glue influence factor; 0 disables the connection and 1 applies its full influence.
    pub intensity: f32,
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
pub struct BindingSample {
    /// Stable sample UUID within the binding axis; distinct from its numeric value.
    pub id: String,
    /// Finite sample coordinate in the driving parameter domain; axis samples are strictly
    /// increasing.
    pub value: f32,
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
pub struct ConstraintSample {
    /// Stable constraint-sample UUID.
    pub id: String,
    /// Finite parameter coordinate for this constraint sample.
    pub value: f32,
    /// Dimensionless BlendShape constraint weight at this parameter sample.
    pub weight: f32,
}
impl BindingAxis {
    pub fn new(parameter_id: impl Into<String>, keys: Vec<f32>) -> Self {
        let samples = keys
            .iter()
            .map(|&value| BindingSample {
                id: uuid::Uuid::new_v4().to_string(),
                value,
            })
            .collect();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            parameter_id: parameter_id.into(),
            keys,
            samples,
        }
    }
    pub fn set_values(&mut self, values: Vec<f32>) {
        assert_eq!(
            values.len(),
            self.samples.len(),
            "Use a grid conversion to change sample count"
        );
        for (sample, &value) in self.samples.iter_mut().zip(&values) {
            sample.value = value;
        }
        self.keys = values;
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
pub struct BindingAxis {
    #[serde(default)]
    /// Stable axis UUID; an empty ID is allocated during native binding normalization.
    pub id: String,
    #[serde(default)]
    /// Ordered stable samples with strictly increasing finite parameter values.
    pub samples: Vec<BindingSample>,
    /// Document parameter ID driving the axis; an empty string represents a pending/unassigned
    /// axis.
    pub parameter_id: String,
    #[serde(skip)]
    pub keys: Vec<f32>,
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
pub struct OffscreenKeyform {
    /// Finite nonnegative opacity factor; 0 is transparent and 1 is opaque. Imported values
    /// above 1 are retained without clamping.
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// RGB multiply factors in channel order R, G, B; (1, 1, 1) is neutral.
    pub multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// RGB screen factors in channel order R, G, B; (0, 0, 0) is neutral.
    pub screen: Option<[f32; 3]>,
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
pub struct Offscreen {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Owning Part object ID; it must name an existing Part rather than a root sentinel.
    pub part_id: String,
    /// Controls visibility of the composited group without changing its contents.
    #[serde(default = "offscreen_enabled_default")]
    pub enabled: bool,
    /// Compositing mode used when drawing this object.
    pub blend_mode: u32,
    /// Preserved format-specific Offscreen compositing flags.
    pub flags: u8,
    /// Ordered stable mesh IDs used as clipping-mask sources, not texture IDs.
    pub masks: Vec<String>,
    /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
    pub part_keyform_indices: Vec<i32>,
    /// Shape samples for this binding; coordinate/sample identities must match its axes or key
    /// table.
    pub keyforms: Vec<OffscreenKeyform>,
}

fn offscreen_enabled_default() -> bool {
    true
}

impl Offscreen {
    /// Resolve a Part key slot. None is a neutral appearance, including negative
    /// sentinels. Only an unbound Part may implicitly use static keyform zero.
    pub fn keyform_index(
        &self,
        slot: usize,
        bound_key_count: Option<usize>,
    ) -> Result<Option<usize>, Status> {
        let count = bound_key_count.unwrap_or(1);
        if slot >= count
            || (!self.part_keyform_indices.is_empty() && self.part_keyform_indices.len() != count)
        {
            return Err(Status::error("INVALID_LENGTH", &self.id));
        }
        let index = self
            .part_keyform_indices
            .get(slot)
            .copied()
            .unwrap_or_else(|| {
                if bound_key_count.is_none() && !self.keyforms.is_empty() {
                    0
                } else {
                    -1
                }
            });
        if index < 0 {
            return Ok(None);
        }
        if index as usize >= self.keyforms.len() {
            return Err(Status::error("INDEX_OUT_OF_BOUNDS", &self.id));
        }
        Ok(Some(index as usize))
    }
}

impl Default for Offscreen {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            part_id: String::new(),
            enabled: true,
            blend_mode: 0,
            flags: 4,
            masks: Vec::new(),
            part_keyform_indices: Vec::new(),
            keyforms: Vec::new(),
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
pub struct MeshKeyform {
    /// Import-only numeric coordinates; canonical documents derive these from samples.
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(default)]
    /// Stable sample IDs, one per active binding axis in axis order; not numeric sample
    /// coordinates.
    pub samples: Vec<String>,
    /// Absolute mesh-local shape positions in target mesh vertex order; not displacement vectors.
    pub positions: Vec<Vec2>,
    /// Opacity and multiply/screen color factors for this object or shape.
    pub appearance: Appearance,
    /// Optional keyform drawing-order coordinate; None lets native evaluation use its
    /// unoverridden order.
    pub draw_order: Option<f32>,
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
pub struct MeshBinding {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Ordered parameter axes; keyform coordinates follow this order and samples retain their
    /// own identities.
    pub axes: Vec<BindingAxis>,
    /// Shape samples for this binding; coordinate/sample identities must match its axes or key
    /// table.
    pub keyforms: Vec<MeshKeyform>,
}

macro_rules! scene_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        #[derive(zipzi_api_derive::BinaryWire)]
        pub struct $name(String);
        impl crate::api_shape::ApiShape for $name {
            fn api_shape() -> crate::api_shape::Shape {
                crate::api_shape::Shape::SerdeRecord {
                    name: concat!(module_path!(), "::", stringify!($name)).into(),
                    transparent: true,
                    tuple: true,
                    fields: vec![crate::api_shape::SerdeField {
                        doc: "Persistent scene object identifier.".into(),
                        rust_name: "0".into(),
                        name: "value".into(),
                        shape: crate::api_shape::Shape::String,
                        skip: false,
                        default: None,
                    }],
                }
            }
        }
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
            pub(crate) fn capacity(&self) -> usize {
                self.0.capacity()
            }
            pub fn optional(id: impl Into<String>) -> Option<Self> {
                let id = id.into();
                if id.is_empty() { None } else { Some(Self(id)) }
            }
        }
        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }
        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.into())
            }
        }
    };
}
scene_id!(TransformId);
scene_id!(PartId);

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum SceneTrack {
    Warp {
        /// Stable warp transform ID driven by this ordinary binding.
        target_id: TransformId,
        /// Ordered Warp shape records whose samples refer to this binding's active axes.
        keyforms: Vec<WarpKeyform>,
    },
    Rotation {
        /// Stable rotation transform ID driven by this ordinary binding.
        target_id: TransformId,
        /// Ordered Rotation shape records whose samples refer to this binding's active axes.
        keyforms: Vec<RotationKeyform>,
    },
    Part {
        /// Stable Part ID driven by this ordinary binding.
        target_id: PartId,
        /// Ordered Part shape records whose samples refer to this binding's active axes.
        keyforms: Vec<PartKeyform>,
    },
}
impl SceneTrack {
    pub fn target_id(&self) -> &str {
        match self {
            Self::Warp { target_id, .. } | Self::Rotation { target_id, .. } => target_id.as_str(),
            Self::Part { target_id, .. } => target_id.as_str(),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Warp { keyforms, .. } => keyforms.len(),
            Self::Rotation { keyforms, .. } => keyforms.len(),
            Self::Part { keyforms, .. } => keyforms.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn sample_ids(&self, index: usize) -> &[String] {
        match self {
            Self::Warp { keyforms, .. } => &keyforms[index].samples,
            Self::Rotation { keyforms, .. } => &keyforms[index].samples,
            Self::Part { keyforms, .. } => &keyforms[index].samples,
        }
    }
    pub fn sample(&self, index: usize) -> SceneSample<'_> {
        match self {
            Self::Warp { keyforms, .. } => {
                let f = &keyforms[index];
                SceneSample {
                    keys: &f.keys,
                    positions: &f.positions,
                    rotation: RotationPose::default(),
                    appearance: f.appearance,
                    draw_order: 0.0,
                }
            }
            Self::Rotation { keyforms, .. } => {
                let f = &keyforms[index];
                SceneSample {
                    keys: &f.keys,
                    positions: &[],
                    rotation: f.rotation,
                    appearance: f.appearance,
                    draw_order: 0.0,
                }
            }
            Self::Part { keyforms, .. } => {
                let f = &keyforms[index];
                SceneSample {
                    keys: &f.keys,
                    positions: &[],
                    rotation: RotationPose::default(),
                    appearance: Appearance::default(),
                    draw_order: f.draw_order,
                }
            }
        }
    }
    pub(crate) fn reorder(&mut self, order: &[usize]) {
        fn apply<T>(values: &mut Vec<T>, order: &[usize]) {
            let mut old: Vec<_> = values.drain(..).map(Some).collect();
            values.extend(order.iter().map(|&i| old[i].take().unwrap()));
        }
        match self {
            Self::Warp { keyforms, .. } => apply(keyforms, order),
            Self::Rotation { keyforms, .. } => apply(keyforms, order),
            Self::Part { keyforms, .. } => apply(keyforms, order),
        }
    }
    pub(crate) fn replace(&mut self, form: SceneKeyform) -> Status {
        fn update<T>(forms: &mut [T], form: T, keys: impl Fn(&T) -> &[f32]) -> Status {
            if let Some(slot) = forms.iter_mut().find(|v| keys(v) == keys(&form)) {
                *slot = form;
                Status::ok()
            } else {
                Status::error("INVALID_KEY_COMBINATION", "No matching keyform")
            }
        }
        match (self, form) {
            (Self::Warp { keyforms, .. }, SceneKeyform::Warp(f)) => {
                update(keyforms, f, |f| &f.keys)
            }
            (Self::Rotation { keyforms, .. }, SceneKeyform::Rotation(f)) => {
                update(keyforms, f, |f| &f.keys)
            }
            (Self::Part { keyforms, .. }, SceneKeyform::Part(f)) => {
                update(keyforms, f, |f| &f.keys)
            }
            _ => Status::error("INVALID_KEYFORM_TYPE", "Keyform must match its track"),
        }
    }
    pub fn part_keyforms_mut(&mut self) -> Option<&mut Vec<PartKeyform>> {
        match self {
            Self::Part { keyforms, .. } => Some(keyforms),
            _ => None,
        }
    }
    pub fn samples(&self) -> impl ExactSizeIterator<Item = SceneSample<'_>> {
        (0..self.len()).map(|i| self.sample(i))
    }
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
pub struct SceneBinding {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Ordered parameter axes; keyform coordinates follow this order and samples retain their
    /// own identities.
    pub axes: Vec<BindingAxis>,
    /// Typed Warp, Rotation or Part keyform track; the track target determines shape
    /// interpretation.
    pub track: SceneTrack,
}
impl SceneBinding {
    pub fn target_id(&self) -> &str {
        self.track.target_id()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VertexMapping {
    pub new_id: VertexId,
    pub old_id: Option<VertexId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChangeKind {
    #[default]
    None,
    Metadata,
    /// Evaluated appearance changes without altering topology or dependencies.
    Appearance,
    Resources,
    Positions,
    Structure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChangeSet {
    pub kind: ChangeKind,
    pub mesh_ids: Vec<String>,
    pub revision: u64,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EditResult {
    pub status: Status,
    pub changes: ChangeSet,
    pub referrers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VertexPositionUpdate {
    pub mesh_id: String,
    pub vertex_ids: Vec<VertexId>,
    pub positions: Vec<Vec2>,
}

/// A validated edit that can be staged in a document transaction.
///
/// New atomic edit kinds can be added here without adding another parallel
/// transaction API. A failed commit applies none of the staged edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DocumentEdit {
    VertexPositions(VertexPositionUpdate),
    MeshName { mesh_id: String, name: String },
}
