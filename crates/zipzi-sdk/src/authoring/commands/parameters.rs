use super::*;
use zipzi_contract::authoring::{ParameterCreateSpec, ParameterPatch};

impl Commands<'_> {
    pub fn create_parameter(&mut self, parameter: Parameter) -> Result<(), Status> {
        self.run(|this| {
            let id = parameter.id.clone();
            this.builder.create_parameter(parameter)?;
            let display = this.builder.display_info();
            if display.origin == zipzi_core::document::DisplayInfoOrigin::Generated
                && display.parameters.is_some()
            {
                let mut display = display.clone();
                display.parameters.as_mut().unwrap().push(
                    zipzi_core::document::CdiParameterEntry::Resolved {
                        parameter_id: id,
                        group_id: None,
                        extensions: Default::default(),
                    },
                );
                this.builder.set_display_info(display)?;
            }
            Ok(())
        })
    }

    pub fn replace_parameter(&mut self, parameter: Parameter) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&parameter.id)?;
            this.builder.replace_parameter(parameter)
        })
    }
    pub fn create_parameter_spec(&mut self, spec: ParameterCreateSpec) -> Result<String, Status> {
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

    pub fn patch_parameter(&mut self, id: &str, patch: ParameterPatch) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(id)?;
            let mut parameter = this
                .builder
                .parameter(id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_PARAMETER", id))?;
            patch.apply_to(&mut parameter);
            this.builder.replace_parameter(parameter)
        })
    }
}
trait ParameterPatchApply {
    fn apply_to(self, parameter: &mut Parameter);
}
impl ParameterPatchApply for ParameterPatch {
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
