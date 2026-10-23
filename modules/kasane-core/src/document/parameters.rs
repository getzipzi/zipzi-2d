use super::*;
use crate::types::{ChangeKind, EditResult, Parameter, Status};

impl Document {
    pub fn parameter_order(&self) -> &[String] {
        &self.content.parameter_order
    }

    pub fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.content.parameters.get(id)
    }

    pub(super) fn validate_parameter(&self, p: &Parameter) -> Status {
        if !valid_uuid(&p.id) {
            return Status::error(
                "INVALID_ID",
                format!("{}: parameter requires a canonical UUID", p.id),
            );
        }
        if p.runtime_id.is_empty() {
            return Status::error("INVALID_ID", format!("{}.runtime_id is empty", p.id));
        }
        for (id, other) in &self.content.parameters {
            if id != &p.id && other.runtime_id == p.runtime_id {
                return Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", p.id, id),
                );
            }
        }
        if self.content.display_info.parameters.as_ref().is_some_and(|entries| entries.iter().any(|entry| {
            matches!(entry, CdiParameterEntry::Unresolved { runtime_id, .. } if runtime_id == &p.runtime_id)
        })) {
            return Status::error("DUPLICATE_RUNTIME_ID", format!("{}.runtime_id collides with unresolved CDI parameter", p.id));
        }
        if validate_parameter_range(p.minimum, p.maximum, p.default_value).is_err() {
            return Status::error(
                "INVALID_PARAMETER",
                format!(
                    "{}: require finite minimum < maximum and default in range",
                    p.id
                ),
            );
        }
        if p.decimal_places < 0 || p.decimal_places > 9 {
            return Status::error(
                "INVALID_PARAMETER",
                format!("{}.decimal_places must be 0..9", p.id),
            );
        }
        Status::ok()
    }

    pub fn create_parameter(&mut self, mut p: Parameter) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.contains_id(&p.id) {
            return self.failed(Status::error("DUPLICATE_ID", &p.id));
        }
        if p.runtime_id.is_empty() {
            p.runtime_id = p.id.clone();
        }
        let s = self.validate_parameter(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = p.id.clone();
        self.content.parameters.insert(id.clone(), p);
        self.content.parameter_order.push(id.clone());
        // An edited generated CDI table is explicit. Keep newly authored
        // parameters visible there; imported tables retain their file scope.
        if self.content.display_info.origin == DisplayInfoOrigin::Generated {
            if let Some(entries) = &mut self.content.display_info.parameters {
                entries.push(CdiParameterEntry::Resolved {
                    parameter_id: id.clone(),
                    group_id: None,
                    extensions: Default::default(),
                });
            }
        }
        self.changed(ChangeKind::Structure, Vec::new(), vec![id])
    }

    pub fn replace_parameter(&mut self, p: Parameter) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&p.id) {
            return self.failed(status);
        }
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.replace_parameter");
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.content.parameters.contains_key(&p.id) {
            return self.failed(Status::error("MISSING_PARAMETER", &p.id));
        }
        let s = self.validate_parameter(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        // Dependency validation only reads the proposed parameter and existing
        // bindings. Restore the slot on rejection; no revisions or caches change
        // until the complete replacement has been accepted.
        let id = p.id.clone();
        let old = self.content.parameters.insert(id.clone(), p).unwrap();
        let proposed = &self.content.parameters[&id];
        let dependencies_changed = old.minimum != proposed.minimum
            || old.maximum != proposed.maximum
            || old.kind != proposed.kind;
        let structure_changed =
            dependencies_changed || old.decimal_places != proposed.decimal_places;
        let meshes = match self.parameter_dependents(&id, dependencies_changed) {
            Ok(meshes) => meshes,
            Err(status) => {
                self.content.parameters.insert(id, old);
                return self.failed(status);
            }
        };
        self.changed(
            if structure_changed {
                ChangeKind::Structure
            } else {
                ChangeKind::Positions
            },
            meshes,
            vec![id],
        )
    }

    fn parameter_dependents(&self, id: &str, validate: bool) -> Result<Vec<String>, Status> {
        let mut meshes = Vec::new();
        for binding in self.content.bindings.values() {
            if binding.axes.iter().any(|axis| axis.parameter_id == *id) {
                let status = if validate {
                    self.canonicalize_binding(&mut binding.clone())
                } else {
                    Status::ok()
                };
                if !status.is_ok() {
                    return Err(status);
                }
                meshes.push(binding.mesh_id.clone());
            }
        }
        for binding in self.content.scene_bindings.values() {
            if binding.axes.iter().any(|axis| axis.parameter_id == *id) {
                let status = if validate {
                    self.canonicalize_scene_binding(&mut binding.clone())
                } else {
                    Status::ok()
                };
                if !status.is_ok() {
                    return Err(status);
                }
                meshes = self.content.mesh_order.clone();
            }
        }
        for glue in self.content.glues.values() {
            if glue
                .binding
                .as_ref()
                .is_some_and(|b| b.axes.iter().any(|a| a.parameter_id == *id))
            {
                let status = if validate {
                    self.validate_glue(glue)
                } else {
                    Status::ok()
                };
                if !status.is_ok() {
                    return Err(status);
                }
                meshes = self.content.mesh_order.clone();
            }
        }
        for table in self
            .content
            .blend_key_tables
            .values()
            .filter(|t| t.parameter_id == *id)
        {
            let status = if validate {
                self.validate_blend_key_table(table)
            } else {
                Status::ok()
            };
            if !status.is_ok() {
                return Err(status);
            }
            meshes = self.content.mesh_order.clone();
        }
        for constraint in self
            .content
            .blend_constraints
            .values()
            .filter(|c| c.parameter_id == *id)
        {
            let status = if validate {
                self.validate_blend_constraint(constraint)
            } else {
                Status::ok()
            };
            if !status.is_ok() {
                return Err(status);
            }
            meshes = self.content.mesh_order.clone();
        }
        meshes.sort();
        meshes.dedup();
        Ok(meshes)
    }
}

/// Shared scalar parameter-range validation for native edits and host drafts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterRangeError {
    MinimumInvalid,
    MaximumInvalid,
    DefaultInvalid,
    RangeInvalid,
    RangeOverflow,
    DefaultOutOfRange,
}
impl std::fmt::Display for ParameterRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MinimumInvalid => "Enter a finite minimum",
            Self::MaximumInvalid => "Enter a finite maximum",
            Self::DefaultInvalid => "Enter a finite default",
            Self::RangeInvalid => "Minimum must be less than maximum",
            Self::RangeOverflow => "Parameter range length must be finite",
            Self::DefaultOutOfRange => "Default must be within the parameter range",
        })
    }
}
impl std::error::Error for ParameterRangeError {}
pub fn validate_parameter_range(
    minimum: f32,
    maximum: f32,
    default: f32,
) -> Result<(), ParameterRangeError> {
    use ParameterRangeError as E;
    let error = if !minimum.is_finite() {
        Some(E::MinimumInvalid)
    } else if !maximum.is_finite() {
        Some(E::MaximumInvalid)
    } else if !default.is_finite() {
        Some(E::DefaultInvalid)
    } else if minimum >= maximum {
        Some(E::RangeInvalid)
    } else if !(maximum - minimum).is_finite() {
        Some(E::RangeOverflow)
    } else if !(minimum..=maximum).contains(&default) {
        Some(E::DefaultOutOfRange)
    } else {
        None
    };
    error.map_or(Ok(()), Err)
}
