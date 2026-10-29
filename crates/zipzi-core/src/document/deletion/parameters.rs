//! Freeze a removed ordinary dimension using native scalar selection and grid
//! conversion. The deleted control never reads an Editor or playback slider.
use super::*;
use crate::{BindingAxis, SceneTrack};

impl Document {
    pub(super) fn changed_deletion_object(
        &self,
        candidate: &Document,
        id: &str,
    ) -> Option<DeletionObject> {
        macro_rules! changed {
            ($getter:ident,$kind:ident) => {
                if self.$getter(id) != candidate.$getter(id) {
                    return Some(object(DeletionKind::$kind, id));
                }
            };
        }
        changed!(get_mesh, Mesh);
        changed!(get_part, Part);
        changed!(get_transform, Deformer);
        changed!(get_glue, Glue);
        changed!(get_offscreen, Offscreen);
        changed!(get_binding, MeshBinding);
        changed!(get_scene_binding, SceneBinding);
        changed!(get_blend_binding, BlendBinding);
        changed!(get_blend_key_table, BlendKeyTable);
        changed!(get_blend_constraint, BlendConstraint);
        None
    }
    fn deletion_grids(&self) -> Vec<(DeletionObject, &[BindingAxis], DeletionObject)> {
        let mut out = vec![];
        for id in self.binding_order() {
            let b = self.get_binding(id).unwrap();
            out.push((
                object(DeletionKind::MeshBinding, id),
                b.axes.as_slice(),
                object(DeletionKind::Mesh, &b.mesh_id),
            ));
        }
        for id in self.scene_binding_order() {
            let b = self.get_scene_binding(id).unwrap();
            out.push((
                object(DeletionKind::SceneBinding, id),
                b.axes.as_slice(),
                self.deletion_scene_object(b.target_id()).unwrap(),
            ));
        }
        for id in self.glue_order() {
            if let Some(b) = &self.get_glue(id).unwrap().binding {
                let key = object(DeletionKind::Glue, id);
                out.push((
                    object(DeletionKind::GlueBinding, id),
                    b.axes.as_slice(),
                    key,
                ));
            }
        }
        out
    }
    fn deletion_slice(
        &self,
        request: &DeletionRequest,
        axis: &BindingAxis,
    ) -> Result<(f32, f32, bool), Status> {
        let parameter = self
            .get_parameter(&axis.parameter_id)
            .ok_or_else(|| Status::error("MISSING_PARAMETER", &axis.parameter_id))?;
        let value = request
            .parameter_slices
            .get(&parameter.id)
            .copied()
            .unwrap_or(parameter.default_value);
        if !value.is_finite() || value < parameter.minimum || value > parameter.maximum {
            return Err(Status::error("INVALID_PARAMETER_SLICE", &parameter.id));
        }
        let normalized = crate::evaluation::normalize_authoring_parameter(parameter, value);
        let eps = 0.1f32.powi(parameter.decimal_places);
        let segment = crate::keyforms::find_key_segment(normalized, &axis.keys, eps, eps * 1.5);
        if axis.keys.is_empty() {
            return Err(Status::error("INVALID_BINDING", &axis.id));
        }
        let effective = if segment.weight == 0. {
            axis.keys[segment.index as usize]
        } else {
            normalized
        };
        Ok((value, effective, segment.is_outside))
    }
    pub(super) fn analyze_parameter_deletion(
        &self,
        request: &DeletionRequest,
        removed: &BTreeMap<DeletionObject, DeletionReason>,
        report: &mut DeletionReport,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        for (owner, axes, host) in self.deletion_grids() {
            control.checkpoint()?;
            if removed.contains_key(&owner) || removed.contains_key(&host) {
                continue;
            }
            for axis in axes
                .iter()
                .filter(|a| removed.contains_key(&object(DeletionKind::Axis, &a.id)))
            {
                let target = object(DeletionKind::Axis, &axis.id);
                report.rewritten.push(DeletionRelation {
                    owner: owner.clone(),
                    field: DeletionField::ConvertedData,
                    target,
                });
                if !axis.parameter_id.is_empty() {
                    let (value, effective_value, outside) = self.deletion_slice(request, axis)?;
                    let disabled = outside && host.kind != DeletionKind::Glue;
                    report
                        .effects
                        .push(DeletionEffect::ParameterDimensionRemoved {
                            object: owner.clone(),
                            parameter: object(DeletionKind::Parameter, &axis.parameter_id),
                            value,
                            effective_value,
                            disabled,
                        });
                    if disabled {
                        report.rewritten.push(DeletionRelation {
                            owner: host.clone(),
                            field: DeletionField::ConvertedData,
                            target: object(DeletionKind::Parameter, &axis.parameter_id),
                        });
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn install_parameter_deletion(
        &self,
        candidate: &mut Document,
        request: &DeletionRequest,
        report: &mut DeletionReport,
        limits: AuthoringLimits,
        control: &ConversionControl,
        cost: &mut ConversionCost,
    ) -> Result<bool, Status> {
        let removed: BTreeSet<_> = report.removed.iter().map(|r| r.object.clone()).collect();
        let mut affected = BTreeSet::new();
        let mut disabled = BTreeSet::new();
        for (owner, axes, host) in self.deletion_grids() {
            control.checkpoint()?;
            if removed.contains(&owner) || removed.contains(&host) {
                continue;
            }
            for axis in axes
                .iter()
                .filter(|a| removed.contains(&object(DeletionKind::Axis, &a.id)))
            {
                let reduction = if axis.parameter_id.is_empty() {
                    None
                } else {
                    let (_, value, outside) = self.deletion_slice(request, axis)?;
                    if outside && host.kind != DeletionKind::Glue {
                        disabled.insert(host.clone());
                    }
                    Some(AxisReduction::Resample(value))
                };
                let edit = BindingGridEdit::Edit {
                    owner: owner.id.clone(),
                    operation: GridOperation::RemoveAxis {
                        axis_id: axis.id.clone(),
                        reduction,
                        keep_pending: false,
                    },
                };
                let plan = match candidate.plan_deletion_grid(edit, limits, control) {
                    Ok(plan) => plan,
                    Err(status)
                        if status.code == "AUTHORING_CANCELLED"
                            || status.code.starts_with("AUTHORING_") =>
                    {
                        return Err(status)
                    }
                    Err(status) => {
                        report.decisions.push(DeletionDecision {
                            object: owner.clone(),
                            reason: DeletionDecisionReason::InvalidContainer { code: status.code },
                            choices: vec![DeletionChoice::DeleteSubtree, DeletionChoice::Cancel],
                        });
                        return Ok(false);
                    }
                };
                let payload = cost
                    .payload_bytes
                    .checked_add(plan.cost().payload_bytes)
                    .ok_or_else(|| {
                        Status::error("AUTHORING_CAPACITY_OVERFLOW", "Deletion grids")
                    })?;
                *cost = limits.check_conversion(
                    cost.sample_combinations
                        .max(plan.cost().sample_combinations),
                    payload,
                    self.estimated_content_bytes(),
                )?;
                affected.extend(plan.affected_ids().iter().cloned());
                let result = candidate.apply_binding_grid_plan(&plan);
                if !result.status.is_ok() {
                    return Err(result.status);
                }
            }
        }
        for host in disabled {
            match host.kind {
                DeletionKind::Mesh => {
                    candidate.content.meshes.get_mut(&host.id).unwrap().enabled = false
                }
                DeletionKind::Deformer => {
                    candidate
                        .content
                        .transforms
                        .get_mut(&host.id)
                        .unwrap()
                        .enabled = false
                }
                DeletionKind::Part => {
                    candidate.content.parts.get_mut(&host.id).unwrap().enabled = false;
                    let original = self.get_part(&host.id).unwrap().draw_order;
                    if let Some(id) = candidate.binding_for_scene(&host.id).map(|b| b.id.clone()) {
                        if let Some(binding) = candidate.content.scene_bindings.get_mut(&id) {
                            if let SceneTrack::Part { keyforms, .. } = &mut binding.track {
                                for shape in keyforms {
                                    shape.draw_order = original;
                                }
                            }
                        }
                    }
                    candidate
                        .content
                        .parts
                        .get_mut(&host.id)
                        .unwrap()
                        .draw_order = original;
                }
                _ => {}
            }
        }
        // Read-only Offscreens/destinations do not acquire a write lock merely
        // because the converter inspected them. Compare against the true source.
        for id in affected {
            let Some(key) = self.changed_deletion_object(candidate, &id) else {
                continue;
            };
            if let Err(status) = self.deletion_unlocked(&key) {
                report.blockers.push(DeletionLock {
                    object: key,
                    source_id: status.message,
                });
                return Ok(false);
            }
            if !report.rewritten.iter().any(|r| r.owner == key) {
                report.rewritten.push(DeletionRelation {
                    owner: key.clone(),
                    field: DeletionField::ConvertedData,
                    target: key,
                });
            }
        }
        // Actual parameter/map/metadata removal is merged with other domains in
        // the business patch, after every surviving grid has been converted.
        Ok(true)
    }
}
