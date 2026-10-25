//! Ordinary binding workflow. SDK plans own all grid conversion and validation.
use super::*;
use anyhow::{Result, ensure};
use zipzi_sdk::{BindingAuthoringEdit, BindingGridEdit, BindingGridTarget, GridOperation};

#[derive(Clone, Debug)]
pub enum OrdinaryBindingAction {
    Create(ObjectRef),
    AddAxis {
        binding: ObjectRef,
        parameter: String,
    },
    SetParameter {
        axis: ObjectRef,
        parameter: String,
        reduction: Option<f32>,
    },
    InsertSample {
        axis: ObjectRef,
        value: f32,
    },
    MoveSample {
        sample: ObjectRef,
        value: f32,
    },
    DeleteSample(ObjectRef),
    RemoveAxis {
        axis: ObjectRef,
        reduction: Option<f32>,
    },
    Delete(ObjectRef),
    PartDrawOrder {
        binding: ObjectRef,
        value: f32,
    },
}

/// Parameter-panel shortcut applied to every directly selected ordinary target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterKeyformAction {
    AddCurrent,
    DeleteCurrent,
    AddEndpoints,
    AddEndpointsAndMidpoint,
    DeleteAll,
}
impl ParameterKeyformAction {
    fn adds_samples(self) -> bool {
        matches!(
            self,
            Self::AddCurrent | Self::AddEndpoints | Self::AddEndpointsAndMidpoint
        )
    }
}
#[derive(Clone, Debug)]
enum Native {
    Request {
        snapshot: Arc<zipzi_sdk::AuthoringSnapshot>,
        edit: BindingAuthoringEdit,
        limits: zipzi_sdk::AuthoringLimits,
    },
    Grid(zipzi_sdk::BindingEditPlan),
    Part {
        binding_id: String,
        form: zipzi_core::PartKeyform,
    },
}
#[derive(Clone, Debug)]
pub struct OrdinaryBindingPlan {
    native: Native,
    control: zipzi_sdk::ConversionControl,
    version: zipzi_sdk::Version,
    preview: Vec<(String, f32)>,
    pub binding: ObjectRef,
    pub target: ObjectRef,
    pub selection: ObjectRef,
    pub deleted_binding: bool,
    pub report: Option<zipzi_sdk::ConversionReport>,
    pub cost: zipzi_sdk::ConversionCost,
}
#[derive(Clone, Debug)]
pub struct OrdinaryBindingCapture(OrdinaryBindingPlan);
impl OrdinaryBindingCapture {
    pub fn plan(&self, control: &zipzi_sdk::ConversionControl) -> Result<OrdinaryBindingPlan> {
        control
            .check()
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
        let mut plan = self.0.clone();
        plan.control = control.clone();
        if let Native::Request {
            snapshot,
            edit,
            limits,
        } = &plan.native
        {
            let native =
                snapshot.plan_binding_authoring_with_control(edit.clone(), *limits, control)?;
            plan.cost = native.cost();
            plan.report = Some(native.report().clone());
            plan.native = Native::Grid(native);
        }
        Ok(plan)
    }
}
impl Document {
    pub fn capture_ordinary_binding(
        &self,
        action: &OrdinaryBindingAction,
    ) -> Result<OrdinaryBindingCapture> {
        Ok(OrdinaryBindingCapture(
            self.prepare_ordinary_binding(action)?,
        ))
    }
    pub fn plan_ordinary_binding(
        &self,
        action: &OrdinaryBindingAction,
    ) -> Result<OrdinaryBindingPlan> {
        self.capture_ordinary_binding(action)?
            .plan(&Default::default())
    }

    /// Add or remove the current parameter coordinate for all selected ordinary
    /// targets. The candidate composes every target edit and publishes once, so
    /// the whole shortcut is one undo step and cannot leave a partial result.
    pub fn edit_selected_parameter_keyforms(
        &mut self,
        parameter_id: &str,
        action: ParameterKeyformAction,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let parameter = self
            .parameters()
            .into_iter()
            .find(|parameter| parameter.id == parameter_id)
            .ok_or_else(|| anyhow::anyhow!("Parameter no longer exists"))?;
        ensure!(
            self.model()
                .get_parameter(parameter_id)
                .is_some_and(|parameter| parameter.kind == zipzi_core::ParameterKind::Normal),
            "Only normal parameters support ordinary keyforms"
        );
        let targets: Vec<_> = self
            .context()
            .selections
            .iter()
            .filter(|target| {
                matches!(
                    target.kind,
                    ObjectKind::Mesh | ObjectKind::Deformer | ObjectKind::Part
                )
            })
            .cloned()
            .collect();
        ensure!(!targets.is_empty(), "Select an editable object first");

        let version = self.session.version();
        let mut edit = self.session.begin_owned_edit(
            match action {
                ParameterKeyformAction::AddCurrent => "Add parameter keyform",
                ParameterKeyformAction::DeleteCurrent => "Delete parameter keyform",
                ParameterKeyformAction::AddEndpoints => "Add parameter endpoint keyforms",
                ParameterKeyformAction::AddEndpointsAndMidpoint => {
                    "Add parameter endpoint and midpoint keyforms"
                }
                ParameterKeyformAction::DeleteAll => "Delete all parameter keyforms",
            },
            Some(version),
        )?;
        let value = parameter.value;
        let mut added_values = match action {
            ParameterKeyformAction::AddCurrent => vec![value],
            ParameterKeyformAction::AddEndpoints => {
                vec![parameter.minimum, parameter.maximum]
            }
            ParameterKeyformAction::AddEndpointsAndMidpoint => vec![
                parameter.minimum,
                parameter.minimum + (parameter.maximum - parameter.minimum) / 2.,
                parameter.maximum,
            ],
            ParameterKeyformAction::DeleteCurrent | ParameterKeyformAction::DeleteAll => Vec::new(),
        };
        added_values.sort_by(f32::total_cmp);
        added_values.dedup();
        let changed = edit.apply(|candidate| {
            let mut changed = 0usize;
            for target in &targets {
                let existing = {
                    let document = candidate.candidate_document();
                    match target.kind {
                        ObjectKind::Mesh => document
                            .binding_for_mesh(&target.id)
                            .map(|binding| (binding.id.clone(), binding.axes.clone())),
                        ObjectKind::Deformer | ObjectKind::Part => document
                            .binding_for_scene(&target.id)
                            .map(|binding| (binding.id.clone(), binding.axes.clone())),
                        _ => None,
                    }
                };
                let (binding_id, axes) = if let Some(existing) = existing {
                    existing
                } else if action.adds_samples() {
                    let id = Uuid::new_v4().to_string();
                    candidate.binding_authoring(BindingAuthoringEdit::Grid(
                        BindingGridEdit::Create {
                            id: id.clone(),
                            target: if target.kind == ObjectKind::Mesh {
                                BindingGridTarget::Mesh(target.id.clone())
                            } else {
                                BindingGridTarget::Scene(target.id.clone())
                            },
                        },
                    ))?;
                    (id, Vec::new())
                } else {
                    continue;
                };

                let axis = axes.iter().find(|axis| axis.parameter_id == parameter_id);
                let operations = match (action, axis) {
                    (
                        ParameterKeyformAction::AddCurrent
                        | ParameterKeyformAction::AddEndpoints
                        | ParameterKeyformAction::AddEndpointsAndMidpoint,
                        None,
                    ) => vec![GridOperation::AddAxis {
                        axis: zipzi_core::BindingAxis::new(parameter_id, added_values.clone()),
                    }],
                    (
                        ParameterKeyformAction::AddCurrent
                        | ParameterKeyformAction::AddEndpoints
                        | ParameterKeyformAction::AddEndpointsAndMidpoint,
                        Some(axis),
                    ) => added_values
                        .iter()
                        .filter(|value| !axis.keys.contains(value))
                        .map(|&value| GridOperation::InsertSample {
                            axis_id: axis.id.clone(),
                            id: Uuid::new_v4().to_string(),
                            value,
                        })
                        .collect(),
                    (ParameterKeyformAction::DeleteCurrent, Some(axis)) => axis
                        .samples
                        .iter()
                        .find(|sample| sample.value == value)
                        .map(|sample| {
                            if axis.samples.len() == 1 {
                                GridOperation::RemoveAxis {
                                    axis_id: axis.id.clone(),
                                    reduction: Some(zipzi_sdk::AxisReduction::Slice(value)),
                                    keep_pending: false,
                                }
                            } else {
                                GridOperation::DeleteSample {
                                    sample_id: sample.id.clone(),
                                }
                            }
                        })
                        .into_iter()
                        .collect(),
                    (ParameterKeyformAction::DeleteAll, Some(axis)) => {
                        let reduction = value.clamp(axis.keys[0], *axis.keys.last().unwrap());
                        vec![GridOperation::RemoveAxis {
                            axis_id: axis.id.clone(),
                            reduction: Some(zipzi_sdk::AxisReduction::Resample(reduction)),
                            keep_pending: false,
                        }]
                    }
                    (
                        ParameterKeyformAction::DeleteCurrent | ParameterKeyformAction::DeleteAll,
                        None,
                    ) => Vec::new(),
                };
                for operation in operations {
                    candidate.binding_authoring(BindingAuthoringEdit::Grid(
                        BindingGridEdit::Edit {
                            owner: binding_id.clone(),
                            operation,
                        },
                    ))?;
                    changed += 1;
                }
            }
            Ok(changed)
        })?;
        ensure!(
            changed > 0,
            "The selected objects already have the requested keyform state"
        );
        let receipt = edit.commit_to(&mut self.session)?;
        self.pristine_blank = None;
        self.consume_authoring_receipt(&receipt, true);
        self.context.reconcile(&self.object_index());
        Ok(self.authoring_result(receipt))
    }

    fn ordinary_part_keyform(&self, binding: &ObjectRef) -> Result<zipzi_core::PartKeyform> {
        let model_read_guard_46 = self.model();
        let b = model_read_guard_46
            .get_scene_binding(&binding.id)
            .ok_or_else(|| anyhow::anyhow!("Expected Part binding"))?;
        let zipzi_core::SceneTrack::Part { keyforms, .. } = &b.track else {
            anyhow::bail!("Expected Part binding")
        };
        let parameters = self.parameters();
        let active: Vec<_> = b
            .axes
            .iter()
            .filter(|a| !a.parameter_id.is_empty())
            .collect();
        ensure!(!active.is_empty(), "Select a stored keyform first");
        keyforms
            .iter()
            .find(|f| {
                f.keys.len() == active.len()
                    && active.iter().zip(&f.keys).all(|(a, k)| {
                        parameters.iter().any(|p| {
                            if p.id != a.parameter_id {
                                return false;
                            }
                            let model_read_guard_47 = self.model();
                            let parameter = model_read_guard_47.get_parameter(&p.id).unwrap();
                            let value = if parameter.repeat {
                                let range = p.maximum - p.minimum;
                                p.minimum + (p.value - p.minimum).rem_euclid(range)
                            } else {
                                p.value
                            };
                            (value - k).abs() < 1e-6
                        })
                    })
            })
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Select a stored keyform first"))
    }
    pub fn ordinary_part_draw_order(&self, binding: &ObjectRef) -> Result<f32> {
        Ok(self.ordinary_part_keyform(binding)?.draw_order)
    }
    pub fn ordinary_binding(&self, object: &ObjectRef) -> Option<crate::BindingInfo> {
        let index = self.object_index();
        index
            .bindings
            .iter()
            .find(|b| {
                !b.blend
                    && matches!(
                        b.reference.kind,
                        ObjectKind::MeshBinding | ObjectKind::SceneBinding
                    )
                    && (&b.reference == object
                        || &b.target == object
                        || b.axes.iter().any(|a| {
                            a.id == object.id || a.samples.iter().any(|s| s.id == object.id)
                        }))
            })
            .cloned()
    }
    pub fn ordinary_binding_creation_capability(
        &self,
        object: &ObjectRef,
    ) -> Result<zipzi_sdk::CapabilityState> {
        ensure!(
            matches!(
                object.kind,
                ObjectKind::Mesh | ObjectKind::Deformer | ObjectKind::Part
            ),
            "Expected ordinary binding target"
        );
        self.session
            .handle(object.kind.sdk_kind().unwrap(), &object.id)?;
        if self.model().binding_for_mesh(&object.id).is_some()
            || self.model().binding_for_scene(&object.id).is_some()
        {
            return Ok(zipzi_sdk::CapabilityState::Unavailable {
                code: "BINDING_CONFLICT",
            });
        }
        Ok(self
            .authoring_capabilities(object)?
            .state(AuthoringAction::Patch))
    }
    fn prepare_ordinary_binding(
        &self,
        action: &OrdinaryBindingAction,
    ) -> Result<OrdinaryBindingPlan> {
        use OrdinaryBindingAction as A;
        self.ensure_authoring_idle()?;
        let source = match action {
            A::Create(o) | A::Delete(o) | A::DeleteSample(o) => o,
            A::AddAxis { binding, .. } | A::PartDrawOrder { binding, .. } => binding,
            A::SetParameter { axis, .. }
            | A::InsertSample { axis, .. }
            | A::RemoveAxis { axis, .. } => axis,
            A::MoveSample { sample, .. } => sample,
        };
        self.session.handle(
            source
                .kind
                .sdk_kind()
                .ok_or_else(|| anyhow::anyhow!("Expected binding object"))?,
            &source.id,
        )?;
        let existing = self.ordinary_binding(source);
        let (binding, target) = if let A::Create(target) = action {
            ensure!(
                matches!(
                    self.ordinary_binding_creation_capability(target)?,
                    zipzi_sdk::CapabilityState::Available
                ),
                "Binding target is unavailable"
            );
            (
                ObjectRef::new(
                    if target.kind == ObjectKind::Mesh {
                        ObjectKind::MeshBinding
                    } else {
                        ObjectKind::SceneBinding
                    },
                    Uuid::new_v4().to_string(),
                ),
                target.clone(),
            )
        } else {
            let b = existing
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Ordinary binding no longer exists"))?;
            (b.reference.clone(), b.target.clone())
        };
        let axis = || {
            existing
                .as_ref()
                .and_then(|b| b.axes.iter().find(|a| a.id == source.id))
                .ok_or_else(|| anyhow::anyhow!("Binding axis no longer exists"))
        };
        let mut selection = source.clone();
        let mut preview = vec![];
        let edit = match action {
            A::Create(_) => {
                selection = binding.clone();
                BindingGridEdit::Create {
                    id: binding.id.clone(),
                    target: if target.kind == ObjectKind::Mesh {
                        BindingGridTarget::Mesh(target.id.clone())
                    } else {
                        BindingGridTarget::Scene(target.id.clone())
                    },
                }
            }
            A::Delete(_) => {
                selection = target.clone();
                BindingGridEdit::Delete {
                    owner: binding.id.clone(),
                }
            }
            A::PartDrawOrder { value, .. } => {
                ensure!(value.is_finite(), "Invalid draw order");
                self.ensure_editable(std::slice::from_ref(&binding))?;
                let mut form = self.ordinary_part_keyform(&binding)?;
                preview = self
                    .parameters()
                    .iter()
                    .map(|p| (p.id.clone(), p.value))
                    .collect();
                form.draw_order = *value;
                return Ok(OrdinaryBindingPlan {
                    native: Native::Part {
                        binding_id: binding.id.clone(),
                        form,
                    },
                    control: Default::default(),
                    version: self.session.version(),
                    preview,
                    binding: binding.clone(),
                    target,
                    selection: binding,
                    deleted_binding: false,
                    report: None,
                    cost: Default::default(),
                });
            }
            _ => {
                let operation = match action {
                    A::AddAxis { parameter, .. } => {
                        let keys = if parameter.is_empty() {
                            vec![0., 1.]
                        } else {
                            let model = self.model();
                            let p = model
                                .get_parameter(parameter)
                                .ok_or_else(|| anyhow::anyhow!("Parameter no longer exists"))?;
                            let mut keys = vec![p.minimum, p.default_value, p.maximum];
                            keys.sort_by(f32::total_cmp);
                            keys.dedup();
                            keys
                        };
                        let a = zipzi_core::BindingAxis::new(parameter.clone(), keys);
                        selection = ObjectRef::new(ObjectKind::BindingAxis, &a.id);
                        GridOperation::AddAxis { axis: a }
                    }
                    A::SetParameter {
                        parameter,
                        reduction,
                        ..
                    } => {
                        axis()?;
                        GridOperation::SetParameter {
                            axis_id: source.id.clone(),
                            parameter_id: parameter.clone(),
                            reduction: reduction.map(zipzi_sdk::AxisReduction::Slice),
                        }
                    }
                    A::InsertSample { value, .. } => {
                        axis()?;
                        let id = Uuid::new_v4().to_string();
                        selection = ObjectRef::new(ObjectKind::BindingSample, &id);
                        GridOperation::InsertSample {
                            axis_id: source.id.clone(),
                            id,
                            value: *value,
                        }
                    }
                    A::MoveSample { value, .. } => GridOperation::MoveSample {
                        sample_id: source.id.clone(),
                        value: *value,
                    },
                    A::DeleteSample(_) => {
                        selection = ObjectRef::new(
                            ObjectKind::BindingAxis,
                            existing
                                .as_ref()
                                .unwrap()
                                .axes
                                .iter()
                                .find(|a| a.samples.iter().any(|s| s.id == source.id))
                                .ok_or_else(|| anyhow::anyhow!("Sample no longer exists"))?
                                .id
                                .clone(),
                        );
                        GridOperation::DeleteSample {
                            sample_id: source.id.clone(),
                        }
                    }
                    A::RemoveAxis { reduction, .. } => {
                        axis()?;
                        selection = binding.clone();
                        GridOperation::RemoveAxis {
                            axis_id: source.id.clone(),
                            reduction: reduction.map(zipzi_sdk::AxisReduction::Slice),
                            keep_pending: false,
                        }
                    }
                    _ => unreachable!(),
                };
                BindingGridEdit::Edit {
                    owner: binding.id.clone(),
                    operation,
                }
            }
        };
        Ok(OrdinaryBindingPlan {
            version: self.session.version(),
            report: None,
            cost: Default::default(),
            control: Default::default(),
            native: Native::Request {
                snapshot: Arc::new(self.session.read_snapshot()),
                edit: BindingAuthoringEdit::Grid(edit),
                limits: self.session.authoring_limits(),
            },
            preview,
            binding,
            target,
            selection,
            deleted_binding: matches!(action, A::Delete(_)),
        })
    }
    pub fn execute_ordinary_binding(
        &mut self,
        plan: &OrdinaryBindingPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        plan.control
            .check()
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
        ensure!(
            plan.version == self.session.version(),
            "Binding plan expired"
        );
        for (id, value) in &plan.preview {
            ensure!(
                self.parameters()
                    .iter()
                    .any(|p| &p.id == id && p.value == *value),
                "Keyform context changed"
            );
        }
        let receipt = match &plan.native {
            Native::Request { .. } => anyhow::bail!("Binding plan has not been built"),
            Native::Grid(p) => self.session.execute_binding_plan(p)?,
            Native::Part { binding_id, form } => {
                self.session
                    .edit("Edit Part keyform", Some(plan.version), |e| {
                        e.set_scene_keyform(
                            binding_id,
                            zipzi_core::SceneKeyform::Part(form.clone()),
                        )
                    })?
                    .1
            }
        };
        if receipt.changed {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, true);
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn execute_ordinary_binding(
        &mut self,
        document: crate::DocumentId,
        plan: &OrdinaryBindingPlan,
    ) -> Result<OperationResult> {
        ensure!(
            self.editing_context() == Some(crate::EditingContext::Modeling),
            "Modeling context changed"
        );
        self.ensure_authoring_target(document, "Binding")?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|d| d.execute_ordinary_binding(plan))
    }
}
