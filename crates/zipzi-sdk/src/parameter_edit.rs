//! Selected parameter authoring intents. Defaults and identity remain in Rust.
use crate::{EditSession, SdkError};
use serde::{Deserialize, Serialize};
use zipzi_core::{Parameter, ParameterKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct ParameterCreateSpec {
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Lower endpoint of the parameter domain, in model-defined parameter units.
    pub minimum: f32,
    /// Upper endpoint of the parameter domain, in the same units as minimum.
    pub maximum: f32,
    /// Default parameter value in its declared domain; native validation checks it against
    /// minimum and maximum.
    pub default_value: f32,
    /// Optional new parameter UUID; None allocates a fresh UUID when the create operation executes.
    pub id: Option<String>,
    /// Optional external runtime identifier; None uses the native default empty string.
    pub runtime_id: Option<String>,
    /// Optional parameter kind; None uses Normal.
    pub kind: Option<ParameterKind>,
    /// Optional decimal precision for runtime parameter handling; None uses the native default
    /// of 6.
    pub decimal_places: Option<i32>,
    /// Optional range-repeat flag; None uses the native default false.
    pub repeat: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct ParameterPatch {
    /// Human-readable display name; use the stable ID for identity because names can change.
    /// None preserves the current value; it does not reset the field to its default.
    pub name: Option<String>,
    /// Lower endpoint of the parameter domain, in model-defined parameter units. None preserves
    /// the current value; it does not reset the field to its default.
    pub minimum: Option<f32>,
    /// Upper endpoint of the parameter domain, in the same units as minimum. None preserves the
    /// current value; it does not reset the field to its default.
    pub maximum: Option<f32>,
    /// Default parameter value in its declared domain; native validation checks it against
    /// minimum and maximum. None preserves the current value; it does not reset the field to
    /// its default.
    pub default_value: Option<f32>,
    /// Replacement external runtime identifier; an empty string explicitly clears it. None
    /// preserves the current value; it does not reset the field to its default.
    pub runtime_id: Option<String>,
    /// Replacement parameter kind. None preserves the current value; it does not reset the
    /// field to its default.
    pub kind: Option<ParameterKind>,
    /// Replacement decimal precision for runtime parameter handling. None preserves the current
    /// value; it does not reset the field to its default.
    pub decimal_places: Option<i32>,
    /// Replacement repeat flag; false explicitly disables repeating. None preserves the current
    /// value; it does not reset the field to its default.
    pub repeat: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub enum ParameterOperation {
    Create {
        /// Creation specification; native admission supplies identities/defaults and rejects
        /// invalid input.
        spec: ParameterCreateSpec,
    },
    Patch {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Typed patch for the addressed object; apply through the Edit to mutate candidate state.
        patch: ParameterPatch,
    },
}

impl ParameterOperation {
    pub fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError> {
        match self {
            Self::Create { spec } => edit.create_parameter_spec(spec),
            Self::Patch { id, patch } => {
                edit.patch_parameter(&id, patch)?;
                Ok(id)
            }
        }
    }
}
impl EditSession<'_> {
    pub fn create_parameter_spec(&mut self, spec: ParameterCreateSpec) -> Result<String, SdkError> {
        let id = spec.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let mut parameter = Parameter {
            id: id.clone(),
            name: spec.name,
            minimum: spec.minimum,
            maximum: spec.maximum,
            default_value: spec.default_value,
            ..Default::default()
        };
        ParameterPatch {
            runtime_id: spec.runtime_id,
            kind: spec.kind,
            decimal_places: spec.decimal_places,
            repeat: spec.repeat,
            ..Default::default()
        }
        .apply_to(&mut parameter);
        self.create_parameter(parameter)?;
        Ok(id)
    }

    pub fn patch_parameter(&mut self, id: &str, patch: ParameterPatch) -> Result<(), SdkError> {
        self.ensure_active("patch_parameter")?;
        let mut parameter = self
            .candidate_document()
            .get_parameter(id)
            .cloned()
            .ok_or_else(|| {
                let mut error = SdkError::new(
                    "MISSING_PARAMETER",
                    "Parameter does not exist",
                    "patch_parameter",
                );
                error.object_ids.push(id.into());
                self.abort_with(error)
            })?;
        patch.apply_to(&mut parameter);
        self.replace_parameter(parameter)
    }
}
impl ParameterPatch {
    fn apply_to(self, parameter: &mut Parameter) {
        if let Some(value) = self.name {
            parameter.name = value;
        }
        if let Some(value) = self.minimum {
            parameter.minimum = value;
        }
        if let Some(value) = self.maximum {
            parameter.maximum = value;
        }
        if let Some(value) = self.default_value {
            parameter.default_value = value;
        }
        if let Some(value) = self.runtime_id {
            parameter.runtime_id = value;
        }
        if let Some(value) = self.kind {
            parameter.kind = value;
        }
        if let Some(value) = self.decimal_places {
            parameter.decimal_places = value;
        }
        if let Some(value) = self.repeat {
            parameter.repeat = value;
        }
    }
}
