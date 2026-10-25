//! Read-only grid/range/parent plans and one SDK transaction on execution.
use crate::{AuthoringSession, EditReceipt, EditSession, SdkError, Version};
use serde::{Deserialize, Serialize};
use zipzi_core::Status;
pub use zipzi_core::document::{
    AuthoringLimits, AxisReduction, BindingGridEdit, BindingGridPlan, BindingGridTarget,
    BindingIdentityKind, BlendAuthoringEdit, BlendAuthoringPlan, BlendTableOperation,
    ConversionControl, ConversionCost, ConversionReport, GridGuarantee, GridOperation,
    MeshGridEdit, MeshGridPlan, ParameterRangeEdit, ParameterRangePlan, ParameterRangePolicy,
    ReparentContext, ReparentEdit, ReparentGuarantee, ReparentPlacement, ReparentPlan,
    ReparentStrategy, ReparentSupport,
};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum BindingAuthoringEdit {
    Grid(BindingGridEdit),
    Blend(BlendAuthoringEdit),
    MeshGrid(MeshGridEdit),
    ParameterRange(ParameterRangeEdit),
    Reparent(ReparentEdit),
}
#[derive(Debug, Clone)]
pub enum BindingConversion {
    Grid(BindingGridPlan),
    Blend(BlendAuthoringPlan),
    MeshGrid(MeshGridPlan),
    ParameterRange(ParameterRangePlan),
    Reparent(ReparentPlan),
}
impl BindingConversion {
    pub fn cost(&self) -> ConversionCost {
        match self {
            Self::Grid(p) => p.cost(),
            Self::Blend(p) => p.cost(),
            Self::MeshGrid(p) => p.cost(),
            Self::ParameterRange(p) => p.cost(),
            Self::Reparent(p) => p.cost(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct BindingEditPlan {
    control: ConversionControl,
    version: Version,
    conversion: BindingConversion,
}
impl BindingEditPlan {
    pub fn report(&self) -> &ConversionReport {
        match &self.conversion {
            BindingConversion::Grid(p) => p.report(),
            BindingConversion::Blend(p) => p.report(),
            BindingConversion::MeshGrid(p) => p.report(),
            BindingConversion::ParameterRange(p) => p.report(),
            BindingConversion::Reparent(p) => p.report(),
        }
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn conversion(&self) -> &BindingConversion {
        &self.conversion
    }
    pub fn affected_ids(&self) -> Vec<String> {
        match &self.conversion {
            BindingConversion::Grid(p) => p.affected_ids().to_vec(),
            BindingConversion::Blend(p) => p.affected_ids().to_vec(),
            BindingConversion::MeshGrid(p) => vec![p.binding_id().into(), p.target_id().into()],
            BindingConversion::ParameterRange(p) => p.affected_ids().to_vec(),
            BindingConversion::Reparent(p) => p.affected_ids().to_vec(),
        }
    }
    pub fn losses(&self) -> &[String] {
        &self.report().losses
    }
    pub fn cost(&self) -> ConversionCost {
        self.conversion.cost()
    }
}
fn error(s: Status, operation: &'static str) -> SdkError {
    SdkError::from_status(s, operation, vec![])
}
fn plan(
    document: &zipzi_core::Document,
    edit: BindingAuthoringEdit,
    limits: AuthoringLimits,
    control: &ConversionControl,
) -> Result<BindingConversion, Status> {
    match edit {
        BindingAuthoringEdit::Grid(e) => document
            .plan_binding_grid_with_control(e, limits, control)
            .map(BindingConversion::Grid),
        BindingAuthoringEdit::Blend(e) => document
            .plan_blend_authoring_with_control(e, limits, control)
            .map(BindingConversion::Blend),
        BindingAuthoringEdit::MeshGrid(e) => document
            .plan_mesh_grid_with_control(e, limits, control)
            .map(BindingConversion::MeshGrid),
        BindingAuthoringEdit::ParameterRange(e) => document
            .plan_parameter_range_with_control(e, limits, control)
            .map(BindingConversion::ParameterRange),
        BindingAuthoringEdit::Reparent(e) => document
            .plan_reparent_with_control(e, limits, control)
            .map(BindingConversion::Reparent),
    }
}
impl crate::AuthoringSnapshot {
    /// Plan a conversion off the UI thread against this immutable version.
    pub fn plan_binding_authoring_with_control(
        &self,
        edit: BindingAuthoringEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<BindingEditPlan, SdkError> {
        Ok(BindingEditPlan {
            control: control.clone(),
            version: self.version(),
            conversion: plan(self.document(), edit, limits, control)
                .map_err(|s| error(s, "plan_binding_authoring"))?,
        })
    }
}
impl AuthoringSession {
    pub fn reparent_support(&self, edit: &ReparentEdit) -> Result<ReparentSupport, SdkError> {
        self.document()
            .reparent_support(edit)
            .map_err(|s| error(s, "reparent_support"))
    }
    pub fn authoring_limits(&self) -> AuthoringLimits {
        self.authoring_limits
    }
    /// Session configuration, shared by all frontends; changing it is not a
    /// persistent model edit. Existing plans are checked against the new limits.
    pub fn set_authoring_limits(&mut self, limits: AuthoringLimits) -> Result<(), SdkError> {
        if limits.max_sample_combinations == 0
            || limits.max_conversion_bytes == 0
            || limits.max_history_bytes == 0
        {
            return Err(SdkError::new(
                "INVALID_AUTHORING_LIMITS",
                "Limits must be finite and nonzero",
                "set_authoring_limits",
            ));
        }
        self.authoring_limits = limits;
        Ok(())
    }
    pub fn plan_binding_authoring(
        &self,
        edit: BindingAuthoringEdit,
    ) -> Result<BindingEditPlan, SdkError> {
        self.plan_binding_authoring_with_control(edit, &ConversionControl::default())
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        edit: BindingAuthoringEdit,
        control: &ConversionControl,
    ) -> Result<BindingEditPlan, SdkError> {
        Ok(BindingEditPlan {
            control: control.clone(),
            version: self.version(),
            conversion: plan(self.document(), edit, self.authoring_limits, control)
                .map_err(|s| error(s, "plan_binding_authoring"))?,
        })
    }
    pub fn execute_binding_plan(
        &mut self,
        plan: &BindingEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        plan.control
            .check()
            .map_err(|s| error(s, "execute_binding_plan"))?;
        check_cost(plan.cost(), self.authoring_limits)?;
        let (_, receipt) = self.edit("Edit binding authoring", Some(plan.version), |e| {
            e.apply_binding_plan(plan)
        })?;
        Ok(receipt)
    }
}
pub(crate) fn check_cost(cost: ConversionCost, limits: AuthoringLimits) -> Result<(), SdkError> {
    let code = if cost.sample_combinations > limits.max_sample_combinations {
        "AUTHORING_GRID_LIMIT"
    } else if cost.temporary_bytes > limits.max_conversion_bytes {
        "AUTHORING_MEMORY_LIMIT"
    } else if cost.history_bytes > limits.max_history_bytes {
        "AUTHORING_HISTORY_LIMIT"
    } else {
        return Ok(());
    };
    Err(SdkError::new(
        code,
        "The current session budget requires a new plan",
        "apply_binding_plan",
    ))
}
impl EditSession<'_> {
    pub fn apply_binding_plan(&mut self, plan: &BindingEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(
            plan.version,
            "apply_binding_plan",
            "Replan against the current session",
        )?;
        plan.control
            .check()
            .map_err(|s| self.abort_with(error(s, "apply_binding_plan")))?;
        self.apply_binding_conversion(&plan.conversion)?;
        self.binding_controls.push(plan.control.clone());
        Ok(())
    }
    fn apply_binding_conversion(&mut self, conversion: &BindingConversion) -> Result<(), SdkError> {
        let limits = self
            .session
            .as_ref()
            .map_or(self.authoring_limits, |s| s.authoring_limits);
        check_cost(conversion.cost(), limits).map_err(|e| self.abort_with(e))?;
        self.apply_authoring_change(
            "apply_binding_plan",
            "",
            false,
            |document| match conversion {
                BindingConversion::Grid(p) => document.apply_binding_grid_plan(p),
                BindingConversion::Blend(p) => document.apply_blend_authoring_plan(p),
                BindingConversion::MeshGrid(p) => document.apply_mesh_grid_plan(p),
                BindingConversion::ParameterRange(p) => document.apply_parameter_range_plan(p),
                BindingConversion::Reparent(p) => document.apply_reparent_plan(p),
            },
        )?;
        let cost = conversion.cost();
        self.binding_cost = Some(self.binding_cost.map_or(cost, |old| ConversionCost {
            sample_combinations: old.sample_combinations.max(cost.sample_combinations),
            payload_bytes: old.payload_bytes.max(cost.payload_bytes),
            temporary_bytes: old.temporary_bytes.max(cost.temporary_bytes),
            history_bytes: old.history_bytes.max(cost.history_bytes),
        }));
        Ok(())
    }
    /// Multiple dependent fields are validated as one final state, never through
    /// temporarily relaxed setters. Composes with other operations in this edit.
    pub fn binding_authoring(&mut self, edit: BindingAuthoringEdit) -> Result<(), SdkError> {
        self.binding_authoring_with_control(edit, &ConversionControl::default())
    }
    /// Plan against the current candidate, including prior writes in this edit,
    /// and keep its cancellation lifetime through the final publication check.
    pub fn binding_authoring_with_control(
        &mut self,
        edit: BindingAuthoringEdit,
        control: &ConversionControl,
    ) -> Result<(), SdkError> {
        self.ensure_active("binding_authoring")?;
        let limits = self
            .session
            .as_ref()
            .map_or(self.authoring_limits, |s| s.authoring_limits);
        let conversion = plan(self.candidate_document(), edit, limits, control)
            .map_err(|s| self.abort_with(error(s, "binding_authoring")))?;
        self.apply_binding_conversion(&conversion)?;
        self.binding_controls.push(control.clone());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BindingFieldAddress {
    entity: crate::ObjectHandle,
    owner: crate::ObjectHandle,
    epoch: u64,
}
impl BindingFieldAddress {
    pub fn id(&self) -> &str {
        self.entity.id()
    }
    pub fn owner_id(&self) -> &str {
        self.owner.id()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
pub enum BindingFieldPatch {
    SampleValue(f32),
    ConstraintSample {
        value: f32,
        weight: f32,
    },
    AxisParameter {
        parameter_id: String,
        reduction: Option<AxisReduction>,
    },
}
impl AuthoringSession {
    pub fn binding_field_address(&self, id: &str) -> Result<BindingFieldAddress, SdkError> {
        let identity = self.document().binding_identity(id).ok_or_else(|| {
            SdkError::new("MISSING_BINDING_IDENTITY", id, "binding_field_address")
        })?;
        let kind = match identity.kind {
            BindingIdentityKind::Axis => crate::ObjectKind::BindingAxis,
            BindingIdentityKind::Sample => crate::ObjectKind::BindingSample,
        };
        let d = self.document();
        let owner_kind = if d.get_binding(&identity.owner).is_some() {
            crate::ObjectKind::MeshBinding
        } else if d.get_scene_binding(&identity.owner).is_some() {
            crate::ObjectKind::SceneBinding
        } else if d.get_glue(&identity.owner).is_some() {
            crate::ObjectKind::Glue
        } else if d.get_blend_key_table(&identity.owner).is_some() {
            crate::ObjectKind::BlendKeyTable
        } else {
            crate::ObjectKind::BlendConstraint
        };
        Ok(BindingFieldAddress {
            entity: self.handle(kind, id)?,
            owner: self.handle(owner_kind, &identity.owner)?,
            epoch: self
                .binding_contexts
                .get(&identity.owner)
                .map_or(0, |c| c.1),
        })
    }
    pub fn plan_binding_field_patch(
        &self,
        address: &BindingFieldAddress,
        patch: BindingFieldPatch,
    ) -> Result<BindingEditPlan, SdkError> {
        self.resolve_handle(&address.entity)?;
        self.resolve_handle(&address.owner)?;
        if self
            .binding_contexts
            .get(address.owner.id())
            .map_or(0, |c| c.1)
            != address.epoch
        {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Capture the current axis/sample context",
                "plan_binding_field_patch",
            ));
        }
        let identity = self
            .document()
            .binding_identity(address.id())
            .ok_or_else(|| {
                SdkError::new("MISSING_SAMPLE", address.id(), "plan_binding_field_patch")
            })?;
        if identity.owner != address.owner.id() {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Owner changed",
                "plan_binding_field_patch",
            ));
        }
        let wrong = || {
            SdkError::new(
                "WRONG_FIELD_ADDRESS",
                "Patch does not match this axis/sample",
                "plan_binding_field_patch",
            )
        };
        let intent = match patch {
            BindingFieldPatch::AxisParameter {
                parameter_id,
                reduction,
            } => {
                if identity.kind != BindingIdentityKind::Axis {
                    return Err(wrong());
                }
                BindingAuthoringEdit::Grid(BindingGridEdit::Edit {
                    owner: identity.owner,
                    operation: GridOperation::SetParameter {
                        axis_id: identity.id,
                        parameter_id,
                        reduction,
                    },
                })
            }
            BindingFieldPatch::SampleValue(value) if identity.axis.is_some() => {
                BindingAuthoringEdit::Grid(BindingGridEdit::Edit {
                    owner: identity.owner,
                    operation: GridOperation::MoveSample {
                        sample_id: identity.id,
                        value,
                    },
                })
            }
            BindingFieldPatch::SampleValue(value)
                if self
                    .document()
                    .get_blend_key_table(&identity.owner)
                    .is_some() =>
            {
                BindingAuthoringEdit::Blend(BlendAuthoringEdit::Table {
                    id: identity.owner,
                    operation: BlendTableOperation::Move {
                        sample_id: identity.id,
                        value,
                    },
                })
            }
            patch => {
                let c = self
                    .document()
                    .get_blend_constraint(&identity.owner)
                    .ok_or_else(wrong)?;
                let mut samples = c.samples.clone();
                let sample = samples
                    .iter_mut()
                    .find(|s| s.id == identity.id)
                    .ok_or_else(wrong)?;
                match patch {
                    BindingFieldPatch::SampleValue(value) => sample.value = value,
                    BindingFieldPatch::ConstraintSample { value, weight } => {
                        sample.value = value;
                        sample.weight = weight;
                    }
                    _ => return Err(wrong()),
                }
                samples.sort_by(|a, b| a.value.total_cmp(&b.value));
                BindingAuthoringEdit::Blend(BlendAuthoringEdit::Constraint {
                    id: c.id.clone(),
                    parameter_id: c.parameter_id.clone(),
                    samples,
                })
            }
        };
        self.plan_binding_authoring(intent)
    }
}

#[derive(Debug, Clone, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct BindingCapabilities {
    pub inspect: crate::CapabilityState,
    pub patch: crate::CapabilityState,
    pub create_child: crate::CapabilityState,
    pub reorder: crate::CapabilityState,
    pub delete: crate::CapabilityState,
    pub repair: crate::CapabilityState,
    pub preview: crate::CapabilityState,
}
impl AuthoringSession {
    pub fn binding_capabilities(
        &self,
        handle: &crate::ObjectHandle,
    ) -> Result<BindingCapabilities, SdkError> {
        use crate::{CapabilityState as C, ObjectKind as K};
        self.resolve_handle(handle)?;
        let d = self.document();
        if !matches!(
            handle.kind(),
            K::MeshBinding
                | K::SceneBinding
                | K::Glue
                | K::BlendKeyTable
                | K::BlendConstraint
                | K::BlendBinding
                | K::BindingAxis
                | K::BindingSample
        ) {
            return Err(SdkError::new(
                "WRONG_OBJECT_KIND",
                "Expected a binding, axis, or sample",
                "binding_capabilities",
            ));
        }
        let editable = if d.binding_editability(handle.id()).is_ok() {
            C::Available
        } else {
            C::Unavailable {
                code: "OBJECT_LOCKED",
            }
        };
        let identity = d.binding_identity(handle.id());
        let owner = identity.as_ref().map_or(handle.id(), |i| i.owner.as_str());
        let axes = d
            .get_binding(owner)
            .map(|b| b.axes.as_slice())
            .or_else(|| d.get_scene_binding(owner).map(|b| b.axes.as_slice()))
            .or_else(|| {
                d.get_glue(owner)
                    .and_then(|g| g.binding.as_ref())
                    .map(|b| b.axes.as_slice())
            });
        let pending_axis = identity.as_ref().is_some_and(|identity| {
            let axis = identity.axis.as_deref().unwrap_or(&identity.id);
            axes.is_some_and(|axes| {
                axes.iter()
                    .any(|a| a.id == axis && a.parameter_id.is_empty())
            })
        });
        let pending = pending_axis
            || d.get_blend_binding(owner)
                .is_some_and(|b| !d.blend_binding_active(b))
            || d.get_binding(owner)
                .is_some_and(|b| b.axes.iter().all(|a| a.parameter_id.is_empty()))
            || d.get_scene_binding(owner)
                .is_some_and(|b| b.axes.iter().all(|a| a.parameter_id.is_empty()))
            || d.get_glue(owner)
                .and_then(|g| g.binding.as_ref())
                .is_some_and(|b| b.axes.iter().all(|a| a.parameter_id.is_empty()))
            || d.get_blend_key_table(owner)
                .is_some_and(|t| t.parameter_id.is_empty())
            || d.get_blend_constraint(owner)
                .is_some_and(|c| c.parameter_id.is_empty());
        let preview = if pending {
            C::Unavailable {
                code: "NEEDS_CONFIGURATION",
            }
        } else {
            C::Available
        };
        Ok(BindingCapabilities {
            inspect: C::Available,
            patch: editable.clone(),
            create_child: if matches!(handle.kind(), K::BindingSample | K::BlendBinding) {
                C::Unsupported {
                    code: "SAMPLES_OWNED_BY_GRID_OR_TABLE",
                }
            } else {
                editable.clone()
            },
            reorder: if handle.kind() == K::BindingSample {
                C::Unsupported {
                    code: "SEMANTIC_KEY_ORDER",
                }
            } else {
                editable.clone()
            },
            delete: if editable != C::Available {
                editable.clone()
            } else if handle.kind() == K::SceneBinding
                && d.get_scene_binding(owner)
                    .is_some_and(|b| d.offscreen_for_part(b.target_id()).is_some())
            {
                C::Unavailable {
                    code: "OFFSCREEN_MAPPING_REQUIRES_CONVERSION",
                }
            } else if identity.as_ref().is_some_and(|i| {
                i.kind == BindingIdentityKind::Axis
                    && axes.is_some_and(|axes| {
                        axes.iter()
                            .any(|a| a.id == i.id && !a.parameter_id.is_empty())
                    })
            }) {
                C::Unavailable {
                    code: "AXIS_REDUCTION_REQUIRED",
                }
            } else if d
                .get_blend_key_table(owner)
                .is_some_and(|t| t.base_sample_id == handle.id())
            {
                C::Unavailable {
                    code: "BASE_REPLACEMENT_REQUIRED",
                }
            } else if identity.as_ref().is_some_and(|i| {
                i.kind == BindingIdentityKind::Sample
                    && i.axis.as_ref().is_some_and(|axis| {
                        axes.is_some_and(|axes| {
                            axes.iter().any(|a| {
                                &a.id == axis && a.samples.len() == 1 && !a.parameter_id.is_empty()
                            })
                        })
                    })
            }) {
                C::Unavailable {
                    code: "AXIS_REDUCTION_REQUIRED",
                }
            } else {
                editable.clone()
            },
            repair: editable,
            preview,
        })
    }
}
#[derive(Debug, Clone)]
pub struct BindingExportPlan {
    version: Version,
    pub omitted: Vec<zipzi_core::document::BindingOmission>,
}
impl AuthoringSession {
    pub fn plan_available_binding_export(&self) -> BindingExportPlan {
        BindingExportPlan {
            version: self.version(),
            omitted: self.document().binding_export_omissions(),
        }
    }
    pub fn export_binding_plan(
        &self,
        plan: &BindingExportPlan,
        options: &zipzi_project::Cmo3Options,
    ) -> Result<zipzi_project::Cmo3Artifact, zipzi_project::Cmo3Error> {
        if self.version() != plan.version {
            return Err(zipzi_project::Cmo3Error::InvalidDocument(
                "STALE_PLAN: replan binding omissions".into(),
            ));
        }
        if options.loss_policy == zipzi_project::LossPolicy::Allow {
            return self.encode_cmo3(options);
        }
        match self.encode_cmo3(options) {
            Ok(artifact) => Ok(artifact),
            Err(zipzi_project::Cmo3Error::Unsupported(report))
                if report.diagnostics.iter().all(|d| {
                    plan.omitted
                        .iter()
                        .any(|o| o.object_id == d.object_id && o.code == d.code)
                }) =>
            {
                let mut options = options.clone();
                options.loss_policy = zipzi_project::LossPolicy::Allow;
                self.encode_cmo3(&options)
            }
            Err(e) => Err(e),
        }
    }
}
