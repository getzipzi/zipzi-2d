//! Selected parameter authoring intents. Defaults and identity remain in Rust.
use crate::{EditSession, SdkError};
use serde::{Deserialize, Serialize};
use zipzi_core::{Parameter, ParameterKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct ParameterCreateSpec {
    pub name: String,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    pub id: Option<String>,
    pub runtime_id: Option<String>,
    pub kind: Option<ParameterKind>,
    pub decimal_places: Option<i32>,
    pub repeat: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct ParameterPatch {
    pub name: Option<String>,
    pub minimum: Option<f32>,
    pub maximum: Option<f32>,
    pub default_value: Option<f32>,
    pub runtime_id: Option<String>,
    pub kind: Option<ParameterKind>,
    pub decimal_places: Option<i32>,
    pub repeat: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub enum ParameterOperation {
    Create { spec: ParameterCreateSpec },
    Patch { id: String, patch: ParameterPatch },
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
