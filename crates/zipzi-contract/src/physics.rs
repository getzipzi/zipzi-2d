//! Physics values used in authoring and snapshots.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsVector {
    #[serde(rename = "X")]
    /// Horizontal component in the native Physics coordinate convention.
    pub x: f32,
    #[serde(rename = "Y")]
    /// Vertical component in the native Physics coordinate convention.
    pub y: f32,
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
pub struct PhysicsForces {
    #[serde(rename = "Gravity")]
    /// Effective gravity vector in the native Physics coordinate convention.
    pub gravity: PhysicsVector,
    #[serde(rename = "Wind")]
    /// Effective wind vector in the native Physics coordinate convention.
    pub wind: PhysicsVector,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
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
pub struct PhysicsDictionaryEntry {
    #[serde(rename = "Id")]
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    #[serde(rename = "Name")]
    /// Human-readable display or registration name; distinct from persistent object identity.
    pub name: String,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
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
pub struct PhysicsMeta {
    #[serde(rename = "PhysicsSettingCount")]
    /// Exported count of Physics settings, maintained by native validation.
    pub setting_count: usize,
    #[serde(rename = "TotalInputCount")]
    /// Exported count of Physics inputs, maintained by native validation.
    pub input_count: usize,
    #[serde(rename = "TotalOutputCount")]
    /// Exported count of Physics outputs, maintained by native validation.
    pub output_count: usize,
    #[serde(rename = "VertexCount")]
    /// Exported count of Physics nodes, maintained by native validation.
    pub vertex_count: usize,
    #[serde(rename = "EffectiveForces")]
    /// Native effective gravity and wind vectors.
    pub forces: PhysicsForces,
    #[serde(rename = "PhysicsDictionary")]
    /// Ordered exported Physics rig ID/name metadata.
    pub dictionary: Vec<PhysicsDictionaryEntry>,
    #[serde(rename = "Fps", default, skip_serializing_if = "Option::is_none")]
    /// Nominal motion frames per second or explicit Physics fixed-step frequency, as declared by the owner.
    pub fps: Option<f32>,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
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
pub struct PhysicsParameterRef {
    #[serde(rename = "Target")]
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: String,
    #[serde(rename = "Id")]
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum PhysicsAxis {
    X,
    Y,
    Angle,
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
pub struct PhysicsInput {
    #[serde(rename = "Source")]
    /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
    pub source: PhysicsParameterRef,
    #[serde(rename = "Weight")]
    /// Native Physics influence weight, expressed as a percentage.
    pub weight: f32,
    #[serde(rename = "Type")]
    /// Native Physics axis/channel discriminator; distinct from an object identity.
    pub kind: PhysicsAxis,
    #[serde(rename = "Reflect")]
    /// Whether the native Physics mapping reflects the selected axis.
    pub reflect: bool,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
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
pub struct PhysicsOutput {
    #[serde(rename = "Destination")]
    /// Explicit destination owner/container; order is controlled separately by before.
    pub destination: PhysicsParameterRef,
    #[serde(rename = "VertexIndex")]
    /// Exported node index; native authoring rows use stable node IDs instead.
    pub vertex_index: usize,
    #[serde(rename = "Scale")]
    /// Native Physics output multiplier applied to the selected axis.
    pub scale: f32,
    #[serde(rename = "Weight")]
    /// Native Physics influence weight, expressed as a percentage.
    pub weight: f32,
    #[serde(rename = "Type")]
    /// Native Physics axis/channel discriminator; distinct from an object identity.
    pub kind: PhysicsAxis,
    #[serde(rename = "Reflect")]
    /// Whether the native Physics mapping reflects the selected axis.
    pub reflect: bool,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
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
pub struct PhysicsVertex {
    #[serde(rename = "Position")]
    /// Authored two-dimensional Physics node position or normalization domain, as declared by the field.
    pub position: PhysicsVector,
    #[serde(rename = "Mobility")]
    /// Native Physics node mobility coefficient.
    pub mobility: f32,
    #[serde(rename = "Delay")]
    /// Native Physics node delay coefficient.
    pub delay: f32,
    #[serde(rename = "Acceleration")]
    /// Native Physics node acceleration coefficient.
    pub acceleration: f32,
    #[serde(rename = "Radius")]
    /// Native Physics node segment length/radius.
    pub radius: f32,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PhysicsRange {
    #[serde(rename = "Minimum")]
    /// Lower normalization bound in the declared position or angle domain.
    pub minimum: f32,
    #[serde(rename = "Default")]
    /// Neutral normalization coordinate between the declared bounds.
    pub default: f32,
    #[serde(rename = "Maximum")]
    /// Upper normalization bound in the declared position or angle domain.
    pub maximum: f32,
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
pub struct PhysicsNormalization {
    #[serde(rename = "Position")]
    /// Authored two-dimensional Physics node position or normalization domain, as declared by the field.
    pub position: PhysicsRange,
    #[serde(rename = "Angle")]
    /// Angle normalization range, in the native Physics coordinate convention.
    pub angle: PhysicsRange,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
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
pub struct PhysicsSetting {
    #[serde(rename = "Id")]
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    #[serde(rename = "Input")]
    /// Ordered Physics input rows, each with a stable identity and typed parameter source.
    pub inputs: Vec<PhysicsInput>,
    #[serde(rename = "Output")]
    /// Ordered Physics output rows, each with a stable identity and node reference.
    pub outputs: Vec<PhysicsOutput>,
    #[serde(rename = "Vertices")]
    /// Ordered Physics node chain; outputs refer to stable node IDs.
    pub vertices: Vec<PhysicsVertex>,
    #[serde(rename = "Normalization")]
    /// Native position and angle normalization ranges for this Physics rig.
    pub normalization: PhysicsNormalization,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
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
pub struct PhysicsDefinition {
    #[serde(rename = "Version")]
    /// Native Physics format version number.
    pub version: u32,
    #[serde(rename = "Meta")]
    /// Physics format metadata and counts maintained by native authoring validation.
    pub meta: PhysicsMeta,
    #[serde(rename = "PhysicsSettings")]
    /// Ordered shared Physics rig definitions.
    pub settings: Vec<std::sync::Arc<PhysicsSetting>>,
    #[serde(flatten)]
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

impl PhysicsDefinition {
    pub fn actual_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.settings.len(),
            self.settings
                .iter()
                .map(|setting| setting.inputs.len())
                .sum(),
            self.settings
                .iter()
                .map(|setting| setting.outputs.len())
                .sum(),
            self.settings
                .iter()
                .map(|setting| setting.vertices.len())
                .sum(),
        )
    }
    pub fn counts_match(&self) -> bool {
        self.actual_counts()
            == (
                self.meta.setting_count,
                self.meta.input_count,
                self.meta.output_count,
                self.meta.vertex_count,
            )
    }
}
