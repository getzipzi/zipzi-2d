use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterRangePolicy {
    KeepKeyValues,
    /// Map every dependent key by the same increasing affine change of domain.
    MapKeyValues,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterRangeEdit {
    pub parameter_id: String,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    pub policy: ParameterRangePolicy,
}
#[derive(Debug, Clone, Copy)]
enum GridOwner {
    Mesh,
    Scene,
    Glue,
}
#[derive(Debug, Clone)]
struct AxisWrite {
    owner: GridOwner,
    id: String,
    axis: usize,
    keys: Vec<f32>,
}
#[derive(Debug, Clone)]
pub struct ParameterRangePlan {
    snapshot: Arc<()>,
    parameter: Parameter,
    old_min: f32,
    old_max: f32,
    policy: ParameterRangePolicy,
    axes: Vec<AxisWrite>,
    tables: Vec<(String, Vec<f32>)>,
    constraints: Vec<(String, Vec<f32>)>,
    affected: Vec<String>,
    cost: ConversionCost,
}
impl ParameterRangePlan {
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
    pub fn parameter(&self) -> &Parameter {
        &self.parameter
    }
    pub fn policy(&self) -> ParameterRangePolicy {
        self.policy
    }
}

pub(super) fn map_value(value: f32, old_min: f32, old_max: f32, new_min: f32, new_max: f32) -> f32 {
    if value == old_min {
        return new_min;
    }
    if value == old_max {
        return new_max;
    }
    let t = (f64::from(value) - f64::from(old_min)) / (f64::from(old_max) - f64::from(old_min));
    (f64::from(new_min) * (1.0 - t) + f64::from(new_max) * t) as f32
}
impl Document {
    pub fn plan_parameter_range(
        &self,
        edit: ParameterRangeEdit,
        limits: AuthoringLimits,
    ) -> Result<ParameterRangePlan, Status> {
        let old = self
            .get_parameter(&edit.parameter_id)
            .ok_or_else(|| Status::error("MISSING_PARAMETER", &edit.parameter_id))?;
        self.authoring_unlocked(&old.id)?;
        let mut p = old.clone();
        p.minimum = edit.minimum;
        p.maximum = edit.maximum;
        p.default_value = edit.default_value;
        require_ok(self.validate_parameter(&p))?;
        let remap = |keys: &[f32]| -> Result<Vec<f32>, Status> {
            let mapped: Vec<_> = keys
                .iter()
                .map(|&v| match edit.policy {
                    ParameterRangePolicy::KeepKeyValues => v,
                    ParameterRangePolicy::MapKeyValues => {
                        map_value(v, old.minimum, old.maximum, p.minimum, p.maximum)
                    }
                })
                .collect();
            check_keys(&p, &mapped)?;
            Ok(mapped)
        };
        // Preflight all existing dependency payloads before collecting replacement
        // arrays. Shape data and shared DeltaKeyforms are never copied here.
        let key_count = self
            .content
            .bindings
            .values()
            .flat_map(|b| &b.axes)
            .chain(self.content.scene_bindings.values().flat_map(|b| &b.axes))
            .chain(
                self.content
                    .glues
                    .values()
                    .filter_map(|g| g.binding.as_ref())
                    .flat_map(|b| &b.axes),
            )
            .filter(|a| a.parameter_id == p.id)
            .map(|a| a.keys.len())
            .chain(
                self.content
                    .blend_key_tables
                    .values()
                    .filter(|t| t.parameter_id == p.id)
                    .map(|t| t.keys.len()),
            )
            .chain(
                self.content
                    .blend_constraints
                    .values()
                    .filter(|c| c.parameter_id == p.id)
                    .map(|c| c.keys.len()),
            )
            .try_fold(0usize, |n, len| {
                n.checked_add(len)
                    .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &p.id))
            })?;
        let payload = key_count
            .checked_mul(std::mem::size_of::<f32>() + std::mem::size_of::<AxisWrite>())
            .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &p.id))?;
        let combinations = self
            .content
            .bindings
            .values()
            .filter(|b| b.axes.iter().any(|a| a.parameter_id == p.id))
            .map(|b| b.keyforms.len())
            .chain(
                self.content
                    .scene_bindings
                    .values()
                    .filter(|b| b.axes.iter().any(|a| a.parameter_id == p.id))
                    .map(|b| b.track.len()),
            )
            .chain(
                self.content
                    .glues
                    .values()
                    .filter_map(|g| g.binding.as_ref())
                    .filter(|b| b.axes.iter().any(|a| a.parameter_id == p.id))
                    .map(|b| b.keyforms.len()),
            )
            .max()
            .unwrap_or(1)
            .max(1);
        let cost =
            limits.check_conversion(combinations, payload, self.estimated_content_bytes())?;
        let mut axes = Vec::new();
        let mut affected = vec![p.id.clone()];
        for (owner, bindings) in [
            (
                GridOwner::Mesh,
                self.content
                    .bindings
                    .values()
                    .map(|b| (&b.id, b.axes.as_slice()))
                    .collect::<Vec<_>>(),
            ),
            (
                GridOwner::Scene,
                self.content
                    .scene_bindings
                    .values()
                    .map(|b| (&b.id, b.axes.as_slice()))
                    .collect(),
            ),
            (
                GridOwner::Glue,
                self.content
                    .glues
                    .values()
                    .filter_map(|g| g.binding.as_ref().map(|b| (&g.id, b.axes.as_slice())))
                    .collect(),
            ),
        ] {
            for (id, values) in bindings {
                for (axis, value) in values
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.parameter_id == p.id)
                {
                    let keys = remap(&value.keys)?;
                    if keys != value.keys {
                        self.authoring_unlocked(id)?;
                        affected.push(id.clone());
                        axes.push(AxisWrite {
                            owner,
                            id: id.clone(),
                            axis,
                            keys,
                        });
                    }
                }
            }
        }
        let mut tables = Vec::new();
        let mut constraints = Vec::new();
        for t in self
            .content
            .blend_key_tables
            .values()
            .filter(|t| t.parameter_id == p.id)
        {
            let keys = remap(&t.keys)?;
            if keys != t.keys {
                self.authoring_unlocked(&t.id)?;
                affected.push(t.id.clone());
                tables.push((t.id.clone(), keys));
            }
        }
        for c in self
            .content
            .blend_constraints
            .values()
            .filter(|c| c.parameter_id == p.id)
        {
            let keys = remap(&c.keys)?;
            if keys != c.keys {
                self.authoring_unlocked(&c.id)?;
                affected.push(c.id.clone());
                constraints.push((c.id.clone(), keys));
            }
        }
        // Shared users are part of the accepted impact and lock check even though
        // the unchanged deltas/weights keep their sample correspondence.
        for b in self.content.blend_bindings.values() {
            if tables.iter().any(|(id, _)| id == &b.key_table_id)
                || constraints
                    .iter()
                    .any(|(id, _)| b.constraint_ids.contains(id))
            {
                self.authoring_unlocked(&b.id)?;
                affected.push(b.id.clone());
                affected.push(b.target_id.clone());
            }
        }
        for a in &axes {
            match a.owner {
                GridOwner::Mesh => affected.push(self.get_binding(&a.id).unwrap().mesh_id.clone()),
                GridOwner::Scene => {
                    affected.push(self.get_scene_binding(&a.id).unwrap().target_id().into())
                }
                GridOwner::Glue => {}
            }
        }
        affected.sort();
        affected.dedup();
        Ok(ParameterRangePlan {
            snapshot: self.snapshot_identity.clone(),
            old_min: old.minimum,
            old_max: old.maximum,
            parameter: p,
            policy: edit.policy,
            axes,
            tables,
            constraints,
            affected,
            cost,
        })
    }
    pub fn apply_parameter_range_plan(&mut self, plan: &ParameterRangePlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.snapshot) {
            return self.failed(Status::error("STALE_PLAN", "Replan parameter dependencies"));
        }
        for id in &plan.affected {
            if let Err(s) = self.authoring_unlocked(id) {
                return self.failed(s);
            }
        }
        if self.get_parameter(&plan.parameter.id) == Some(&plan.parameter)
            && plan.axes.is_empty()
            && plan.tables.is_empty()
            && plan.constraints.is_empty()
        {
            return self.failed(Status::ok());
        }
        let map = |v: &mut f32| {
            *v = map_value(
                *v,
                plan.old_min,
                plan.old_max,
                plan.parameter.minimum,
                plan.parameter.maximum,
            );
        };
        // This is the complete validated write set. No setter observes the
        // temporary mismatch between the new range and old grid coordinates.
        self.content
            .parameters
            .insert(plan.parameter.id.clone(), plan.parameter.clone());
        for a in &plan.axes {
            match a.owner {
                GridOwner::Mesh => {
                    let b = self.content.bindings.get_mut(&a.id).unwrap();
                    let dimension = b.axes[..a.axis]
                        .iter()
                        .filter(|a| !a.parameter_id.is_empty())
                        .count();
                    b.axes[a.axis].set_values(a.keys.clone());
                    for f in &mut b.keyforms {
                        map(&mut f.keys[dimension]);
                    }
                }
                GridOwner::Scene => {
                    let b = self.content.scene_bindings.get_mut(&a.id).unwrap();
                    let dimension = b.axes[..a.axis]
                        .iter()
                        .filter(|a| !a.parameter_id.is_empty())
                        .count();
                    b.axes[a.axis].set_values(a.keys.clone());
                    match &mut b.track {
                        crate::SceneTrack::Warp { keyforms, .. } => {
                            for f in keyforms {
                                map(&mut f.keys[dimension]);
                            }
                        }
                        crate::SceneTrack::Rotation { keyforms, .. } => {
                            for f in keyforms {
                                map(&mut f.keys[dimension]);
                            }
                        }
                        crate::SceneTrack::Part { keyforms, .. } => {
                            for f in keyforms {
                                map(&mut f.keys[dimension]);
                            }
                        }
                    }
                }
                GridOwner::Glue => {
                    self.content
                        .glues
                        .get_mut(&a.id)
                        .unwrap()
                        .binding
                        .as_mut()
                        .unwrap()
                        .axes[a.axis]
                        .set_values(a.keys.clone());
                }
            }
        }
        for (id, keys) in &plan.tables {
            let t = self.content.blend_key_tables.get_mut(id).unwrap();
            t.keys.clone_from(keys);
            for (s, &value) in t.samples.iter_mut().zip(keys) {
                s.value = value;
            }
        }
        for (id, keys) in &plan.constraints {
            let c = self.content.blend_constraints.get_mut(id).unwrap();
            c.keys.clone_from(keys);
            for (s, &value) in c.samples.iter_mut().zip(keys) {
                s.value = value;
            }
        }
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            plan.affected.clone(),
        )
    }
}
