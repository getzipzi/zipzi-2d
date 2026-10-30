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

/// A canonical counterclockwise outer boundary and the internal edges retained by remeshing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MeshMeshing {
    /// Closed UV boundary without a repeated endpoint, starting at its smallest vertex ID.
    pub boundary: Vec<VertexId>,
    /// Internal constraints sorted by their canonical endpoint pairs; excludes boundary edges.
    pub constraints: Vec<MeshConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MeshConstraint {
    /// Two distinct vertex IDs in increasing order; the segment is an actual triangle edge.
    pub endpoints: [VertexId; 2],
    /// Whether this edge was fixed by the user or supplied by the edge-layout generator.
    pub kind: MeshConstraintKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MeshConstraintKind {
    User,
    EdgeSupport,
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
