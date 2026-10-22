use super::*;

/// These coordinates use the existing grid model. The plan captures the complete
/// structural baseline; a changed grid requires a fresh plan before execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MeshGridEdit {
    Create {
        id: String,
        mesh_id: String,
    },
    AddAxis {
        binding_id: String,
        axis: BindingAxis,
    },
    ReorderAxes {
        binding_id: String,
        parameter_ids: Vec<String>,
    },
    InsertSample {
        binding_id: String,
        parameter_id: String,
        value: f32,
    },
    MoveSample {
        binding_id: String,
        parameter_id: String,
        from: f32,
        to: f32,
    },
    DeleteSample {
        binding_id: String,
        parameter_id: String,
        value: f32,
    },
    RemoveAxis {
        binding_id: String,
        parameter_id: String,
        reduction: AxisReduction,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AxisReduction {
    Slice(f32),
    Resample(f32),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GridGuarantee {
    BaseShape,
    /// Piecewise multilinear Mesh positions, appearance and effective draw order
    /// over the old grid's effective parameter domain, with float32 arithmetic.
    PreservedOnDomain,
    /// The underlying multilinear interpolant is preserved. Runtime precision
    /// snapping around the new sample can change values within its snap window.
    InterpolantPreserved {
        parameter_id: String,
        value: f32,
    },
    ParameterPlacementChanged,
    DimensionRemoved {
        parameter_id: String,
        value: f32,
    },
    SamplesRemoved {
        parameter_id: String,
        value: f32,
    },
}
#[derive(Debug, Clone)]
pub struct MeshGridPlan {
    domain: BindingGridPlan,
}
impl MeshGridPlan {
    pub fn report(&self) -> &ConversionReport {
        self.domain.report()
    }
    pub fn guarantee(&self) -> &GridGuarantee {
        self.domain.guarantee()
    }
    pub fn cost(&self) -> ConversionCost {
        self.domain.cost()
    }
    pub fn binding_id(&self) -> &str {
        &self.result().id
    }
    pub fn target_id(&self) -> &str {
        &self.result().mesh_id
    }
    pub fn result(&self) -> &MeshBinding {
        self.domain.mesh_binding().unwrap()
    }
}
impl MeshGridEdit {
    pub fn binding_id(&self) -> &str {
        match self {
            Self::Create { id, .. } => id,
            Self::AddAxis { binding_id, .. }
            | Self::ReorderAxes { binding_id, .. }
            | Self::InsertSample { binding_id, .. }
            | Self::MoveSample { binding_id, .. }
            | Self::DeleteSample { binding_id, .. }
            | Self::RemoveAxis { binding_id, .. } => binding_id,
        }
    }
}
impl Document {
    pub fn plan_mesh_grid(
        &self,
        edit: MeshGridEdit,
        limits: AuthoringLimits,
    ) -> Result<MeshGridPlan, Status> {
        self.plan_mesh_grid_with_control(edit, limits, &ConversionControl::default())
    }
    pub fn plan_mesh_grid_with_control(
        &self,
        edit: MeshGridEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<MeshGridPlan, Status> {
        control.checkpoint()?;
        if let MeshGridEdit::Create { id, mesh_id } = edit {
            return Ok(MeshGridPlan {
                domain: self.plan_binding_grid_with_control(
                    BindingGridEdit::Create {
                        id,
                        target: BindingGridTarget::Mesh(mesh_id),
                    },
                    limits,
                    control,
                )?,
            });
        }
        let b = self
            .get_binding(edit.binding_id())
            .ok_or_else(|| Status::error("MISSING_BINDING", edit.binding_id()))?;
        let axis = |parameter: &str| {
            b.axes
                .iter()
                .find(|a| a.parameter_id == parameter)
                .ok_or_else(|| Status::error("MISSING_AXIS", parameter))
        };
        let sample = |parameter: &str, value: f32| {
            axis(parameter)?
                .samples
                .iter()
                .find(|s| s.value == value)
                .map(|s| s.id.clone())
                .ok_or_else(|| Status::error("MISSING_SAMPLE", parameter))
        };
        let operation = match edit {
            MeshGridEdit::AddAxis { axis, .. } => GridOperation::AddAxis { axis },
            MeshGridEdit::ReorderAxes { parameter_ids, .. } => GridOperation::ReorderAxes {
                axis_ids: parameter_ids
                    .iter()
                    .map(|p| Ok(axis(p)?.id.clone()))
                    .collect::<Result<_, Status>>()?,
            },
            MeshGridEdit::InsertSample {
                parameter_id,
                value,
                ..
            } => GridOperation::InsertSample {
                axis_id: axis(&parameter_id)?.id.clone(),
                id: uuid::Uuid::new_v4().to_string(),
                value,
            },
            MeshGridEdit::MoveSample {
                parameter_id,
                from,
                to,
                ..
            } => GridOperation::MoveSample {
                sample_id: sample(&parameter_id, from)?,
                value: to,
            },
            MeshGridEdit::DeleteSample {
                parameter_id,
                value,
                ..
            } => GridOperation::DeleteSample {
                sample_id: sample(&parameter_id, value)?,
            },
            MeshGridEdit::RemoveAxis {
                parameter_id,
                reduction,
                ..
            } => GridOperation::RemoveAxis {
                axis_id: axis(&parameter_id)?.id.clone(),
                reduction: Some(reduction),
                keep_pending: false,
            },
            MeshGridEdit::Create { .. } => unreachable!(),
        };
        Ok(MeshGridPlan {
            domain: self.plan_binding_grid_with_control(
                BindingGridEdit::Edit {
                    owner: b.id.clone(),
                    operation,
                },
                limits,
                control,
            )?,
        })
    }
    pub fn apply_mesh_grid_plan(&mut self, plan: &MeshGridPlan) -> EditResult {
        self.apply_binding_grid_plan(&plan.domain)
    }
}
