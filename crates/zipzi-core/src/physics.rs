//! Physics definitions and invariants shared by authoring and evaluation.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

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

#[derive(Debug, Error, PartialEq, Eq)]
#[error("physics3 {code} at {path}: {message}")]
pub struct PhysicsValidationError {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

fn error(
    code: &'static str,
    path: impl Into<String>,
    message: impl Into<String>,
) -> PhysicsValidationError {
    PhysicsValidationError {
        code,
        path: path.into(),
        message: message.into(),
    }
}

fn finite(value: f32, path: String) -> Result<(), PhysicsValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(error(
            "NONFINITE_NUMBER",
            path,
            "physics number must be finite",
        ))
    }
}

fn check_extensions(
    fields: &BTreeMap<String, Value>,
    path: &str,
    known: &[&str],
) -> Result<(), PhysicsValidationError> {
    fn contains_number(value: &Value) -> bool {
        match value {
            Value::Number(_) => true,
            Value::Array(items) => items.iter().any(contains_number),
            Value::Object(items) => items.values().any(contains_number),
            _ => false,
        }
    }
    for (key, value) in fields {
        if known.contains(&key.as_str()) {
            return Err(error(
                "EXTENSION_KEY_COLLISION",
                format!("{path}.{key}"),
                "extension shadows typed field",
            ));
        }
        if contains_number(value) {
            return Err(error(
                "UNSUPPORTED_EXTENSION_NUMBER",
                format!("{path}.{key}"),
                "numeric unknown fields lack verified Framework encoding",
            ));
        }
    }
    Ok(())
}

fn validate(physics: &PhysicsDefinition, writer: bool) -> Result<(), PhysicsValidationError> {
    if physics.version != 3 {
        return Err(error(
            "INVALID_VERSION",
            "$.Version",
            "physics3 Version must be 3",
        ));
    }
    for (path, vector) in [
        (
            "$.Meta.EffectiveForces.Gravity",
            physics.meta.forces.gravity,
        ),
        ("$.Meta.EffectiveForces.Wind", physics.meta.forces.wind),
    ] {
        finite(vector.x, format!("{path}.X"))?;
        finite(vector.y, format!("{path}.Y"))?;
    }
    if let Some(fps) = physics.meta.fps {
        if !fps.is_finite() || fps < 0.0 {
            return Err(error(
                "INVALID_FPS",
                "$.Meta.Fps",
                "FPS must be nonnegative and finite",
            ));
        }
    }
    let mut ids = HashSet::new();
    for (index, setting) in physics.settings.iter().enumerate() {
        let path = format!("$.PhysicsSettings[{index}]");
        if setting.id.is_empty() || setting.id.contains('\0') || !ids.insert(setting.id.as_str()) {
            return Err(error(
                "INVALID_SETTING_ID",
                format!("{path}.Id"),
                "setting ID must be unique and nonempty",
            ));
        }
        validate_setting(setting, &path, writer)?;
    }

    let dictionary_ids: HashSet<_> = physics
        .meta
        .dictionary
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    if dictionary_ids.len() != physics.meta.dictionary.len() || ids != dictionary_ids {
        return Err(error(
            "INVALID_PHYSICS_DICTIONARY",
            "$.Meta.PhysicsDictionary",
            "dictionary IDs must match settings",
        ));
    }
    if writer {
        check_extensions(
            &physics.extensions,
            "$",
            &["Version", "Meta", "PhysicsSettings"],
        )?;
        check_extensions(
            &physics.meta.extensions,
            "$.Meta",
            &[
                "PhysicsSettingCount",
                "TotalInputCount",
                "TotalOutputCount",
                "VertexCount",
                "EffectiveForces",
                "PhysicsDictionary",
                "Fps",
            ],
        )?;
        check_extensions(
            &physics.meta.forces.extensions,
            "$.Meta.EffectiveForces",
            &["Gravity", "Wind"],
        )?;
        for (index, item) in physics.meta.dictionary.iter().enumerate() {
            check_extensions(
                &item.extensions,
                &format!("$.Meta.PhysicsDictionary[{index}]"),
                &["Id", "Name"],
            )?;
        }
    }
    Ok(())
}

/// Validate a single rig after a local edit; asset identity is checked separately.
pub fn validate_physics_setting(setting: &PhysicsSetting) -> Result<(), PhysicsValidationError> {
    validate_setting(setting, "$", false)
}
fn validate_setting(
    setting: &PhysicsSetting,
    path: &str,
    writer: bool,
) -> Result<(), PhysicsValidationError> {
    if setting.vertices.is_empty() {
        return Err(error(
            "EMPTY_VERTICES",
            format!("{path}.Vertices"),
            "rig needs at least one vertex",
        ));
    }
    for (i, input) in setting.inputs.iter().enumerate() {
        let item = format!("{path}.Input[{i}]");
        if input.source.target != "Parameter"
            || input.source.id.is_empty()
            || input.source.id.contains('\0')
        {
            return Err(error(
                "INVALID_INPUT_TARGET",
                format!("{item}.Source"),
                "input must refer to a Parameter ID",
            ));
        }
        finite(input.weight, format!("{item}.Weight"))?;
        if writer {
            check_extensions(
                &input.source.extensions,
                &format!("{item}.Source"),
                &["Target", "Id"],
            )?;
            check_extensions(
                &input.extensions,
                &item,
                &["Source", "Weight", "Type", "Reflect"],
            )?;
        }
    }
    for (i, output) in setting.outputs.iter().enumerate() {
        let item = format!("{path}.Output[{i}]");
        if output.destination.target != "Parameter"
            || output.destination.id.is_empty()
            || output.destination.id.contains('\0')
        {
            return Err(error(
                "INVALID_OUTPUT_TARGET",
                format!("{item}.Destination"),
                "output must refer to a Parameter ID",
            ));
        }
        if output.vertex_index >= setting.vertices.len() {
            return Err(error(
                "INVALID_VERTEX_INDEX",
                format!("{item}.VertexIndex"),
                "output vertex is absent",
            ));
        }
        finite(output.weight, format!("{item}.Weight"))?;
        finite(output.scale, format!("{item}.Scale"))?;
        if writer {
            check_extensions(
                &output.destination.extensions,
                &format!("{item}.Destination"),
                &["Target", "Id"],
            )?;
            check_extensions(
                &output.extensions,
                &item,
                &[
                    "Destination",
                    "VertexIndex",
                    "Scale",
                    "Weight",
                    "Type",
                    "Reflect",
                ],
            )?;
        }
    }
    for (i, vertex) in setting.vertices.iter().enumerate() {
        let item = format!("{path}.Vertices[{i}]");
        for (label, value) in [
            ("Position.X", vertex.position.x),
            ("Position.Y", vertex.position.y),
            ("Mobility", vertex.mobility),
            ("Delay", vertex.delay),
            ("Acceleration", vertex.acceleration),
            ("Radius", vertex.radius),
        ] {
            finite(value, format!("{item}.{label}"))?;
        }
        if writer {
            check_extensions(
                &vertex.extensions,
                &item,
                &["Position", "Mobility", "Delay", "Acceleration", "Radius"],
            )?;
        }
    }
    for (label, range) in [
        ("Position", setting.normalization.position),
        ("Angle", setting.normalization.angle),
    ] {
        for (field, value) in [
            ("Minimum", range.minimum),
            ("Default", range.default),
            ("Maximum", range.maximum),
        ] {
            finite(value, format!("{path}.Normalization.{label}.{field}"))?;
        }
        if range.minimum > range.default || range.default > range.maximum {
            return Err(error(
                "INVALID_NORMALIZATION",
                format!("{path}.Normalization.{label}"),
                "normalization default must be within range",
            ));
        }
    }
    if writer {
        check_extensions(
            &setting.normalization.extensions,
            &format!("{path}.Normalization"),
            &["Position", "Angle"],
        )?;
        check_extensions(
            &setting.extensions,
            &path,
            &["Id", "Input", "Output", "Vertices", "Normalization"],
        )?;
    }
    Ok(())
}

pub fn validate_physics_definition(
    physics: &PhysicsDefinition,
) -> Result<(), PhysicsValidationError> {
    validate(physics, false)
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

pub fn validate_physics_export(physics: &PhysicsDefinition) -> Result<(), PhysicsValidationError> {
    validate(physics, true)
}
