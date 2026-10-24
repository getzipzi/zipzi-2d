use super::*;
use crate::{Appearance, BindingSample, RotationPose, WarpBezier};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GridOperation {
    AddAxis {
        axis: BindingAxis,
    },
    RemapParameter {
        axis_id: String,
        parameter_id: String,
        policy: ParameterRangePolicy,
    },
    SetParameter {
        axis_id: String,
        parameter_id: String,
        reduction: Option<AxisReduction>,
    },
    ReorderAxes {
        axis_ids: Vec<String>,
    },
    InsertSample {
        axis_id: String,
        id: String,
        value: f32,
    },
    MoveSample {
        sample_id: String,
        value: f32,
    },
    DeleteSample {
        sample_id: String,
    },
    RemoveAxis {
        axis_id: String,
        reduction: Option<AxisReduction>,
        keep_pending: bool,
    },
}
pub(super) struct Lattice {
    pub axes: Vec<BindingAxis>,
    pub weights: Vec<Vec<(usize, f32)>>,
    pub coordinates: Vec<Vec<String>>,
    pub cost: ConversionCost,
    pub guarantee: GridGuarantee,
}
pub(super) fn active(axes: &[BindingAxis]) -> Vec<&BindingAxis> {
    axes.iter().filter(|a| !a.parameter_id.is_empty()).collect()
}
pub(super) fn selection(axes: &[BindingAxis], values: &[f32]) -> Vec<(usize, f32)> {
    let mut weights = vec![(0, 1.0)];
    let mut stride = 1;
    for (a, &value) in active(axes).into_iter().zip(values) {
        let hi = a.keys.partition_point(|k| *k < value).min(a.keys.len() - 1);
        let lo = if a.keys[hi] <= value || hi == 0 {
            hi
        } else {
            hi - 1
        };
        let t = if lo == hi {
            0.0
        } else {
            (value - a.keys[lo]) / (a.keys[hi] - a.keys[lo])
        };
        let len = weights.len();
        for i in 0..len {
            let (slot, w) = weights[i];
            weights[i] = (slot + lo * stride, w * (1.0 - t));
            if t != 0.0 {
                weights.push((slot + hi * stride, w * t));
            }
        }
        stride *= a.keys.len();
    }
    weights
}
impl Document {
    pub(super) fn convert_lattice(
        &self,
        old: &[BindingAxis],
        op: GridOperation,
        bytes_per_form: usize,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<Lattice, Status> {
        let mut axes = old.to_vec();
        let mut removed = None;
        let mut guarantee = GridGuarantee::PreservedOnDomain;
        let axis_index = |id: &str| {
            old.iter()
                .position(|a| a.id == id)
                .ok_or_else(|| Status::error("MISSING_AXIS", id))
        };
        let sample_index = |id: &str| {
            old.iter()
                .enumerate()
                .find_map(|(i, a)| a.samples.iter().position(|s| s.id == id).map(|j| (i, j)))
                .ok_or_else(|| Status::error("MISSING_SAMPLE", id))
        };
        let reduce = |a: &BindingAxis, policy: Option<AxisReduction>| -> Result<f32, Status> {
            let policy = policy.ok_or_else(|| Status::error("AXIS_REDUCTION_REQUIRED", &a.id))?;
            let value = match policy {
                AxisReduction::Slice(v) | AxisReduction::Resample(v) => v,
            };
            if !value.is_finite()
                || a.keys.is_empty()
                || value < a.keys[0]
                || value > *a.keys.last().unwrap()
            {
                return Err(Status::error("OUTSIDE_GRID_DOMAIN", &a.id));
            }
            if matches!(policy, AxisReduction::Slice(_)) && !a.keys.contains(&value) {
                return Err(Status::error("MISSING_SAMPLE", &a.id));
            }
            Ok(value)
        };
        match op {
            GridOperation::AddAxis { mut axis } => {
                identity::normalize_axes(std::slice::from_mut(&mut axis))?;
                axes.push(axis);
            }
            GridOperation::RemapParameter {
                axis_id,
                parameter_id,
                policy,
            } => {
                let i = axis_index(&axis_id)?;
                let previous = self
                    .get_parameter(&axes[i].parameter_id)
                    .ok_or_else(|| Status::error("MISSING_PARAMETER", &axes[i].parameter_id))?;
                let next = self
                    .get_parameter(&parameter_id)
                    .ok_or_else(|| Status::error("MISSING_PARAMETER", &parameter_id))?;
                if policy == ParameterRangePolicy::MapKeyValues {
                    for s in &mut axes[i].samples {
                        s.value = super::range::map_value(
                            s.value,
                            previous.minimum,
                            previous.maximum,
                            next.minimum,
                            next.maximum,
                        );
                    }
                }
                axes[i].parameter_id = parameter_id;
                guarantee = GridGuarantee::ParameterPlacementChanged;
            }
            GridOperation::SetParameter {
                axis_id,
                parameter_id,
                reduction,
            } => {
                let i = axis_index(&axis_id)?;
                if parameter_id.is_empty() && !axes[i].parameter_id.is_empty() {
                    let value = reduce(&axes[i], reduction)?;
                    removed = Some((axis_id.clone(), value));
                    guarantee = GridGuarantee::DimensionRemoved {
                        parameter_id: axes[i].parameter_id.clone(),
                        value,
                    };
                }
                if !axes[i].parameter_id.is_empty()
                    && !parameter_id.is_empty()
                    && axes[i].parameter_id != parameter_id
                {
                    guarantee = GridGuarantee::ParameterPlacementChanged;
                }
                axes[i].parameter_id = parameter_id;
                if !axes[i].parameter_id.is_empty() && axes[i].samples.is_empty() {
                    let p = self
                        .get_parameter(&axes[i].parameter_id)
                        .ok_or_else(|| Status::error("MISSING_PARAMETER", &axes[i].parameter_id))?;
                    let mut keys = vec![p.minimum, p.default_value, p.maximum];
                    keys.dedup();
                    axes[i].samples = keys
                        .iter()
                        .map(|&value| BindingSample {
                            id: uuid::Uuid::new_v4().to_string(),
                            value,
                        })
                        .collect();
                    axes[i].keys = keys;
                }
            }
            GridOperation::ReorderAxes { axis_ids } => {
                if axis_ids.len() != axes.len() {
                    return Err(Status::error(
                        "INVALID_AXIS_ORDER",
                        "Every axis is required",
                    ));
                }
                axes = axis_ids
                    .iter()
                    .map(|id| Ok(old[axis_index(id)?].clone()))
                    .collect::<Result<_, Status>>()?;
            }
            GridOperation::InsertSample { axis_id, id, value } => {
                let i = axis_index(&axis_id)?;
                if !valid_uuid(&id) || old.iter().any(|a| a.samples.iter().any(|s| s.id == id)) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                if !value.is_finite()
                    || (!axes[i].parameter_id.is_empty()
                        && (value <= axes[i].keys[0] || value >= *axes[i].keys.last().unwrap()))
                {
                    return Err(Status::error("OUTSIDE_GRID_DOMAIN", axis_id));
                }
                if axes[i].keys.contains(&value) {
                    return Err(Status::error("DUPLICATE_KEY", axis_id));
                }
                axes[i].samples.push(BindingSample { id, value });
                axes[i].samples.sort_by(|a, b| a.value.total_cmp(&b.value));
                guarantee = GridGuarantee::InterpolantPreserved {
                    parameter_id: axes[i].parameter_id.clone(),
                    value,
                };
            }
            GridOperation::MoveSample { sample_id, value } => {
                let (i, j) = sample_index(&sample_id)?;
                axes[i].samples[j].value = value;
                axes[i].samples.sort_by(|a, b| a.value.total_cmp(&b.value));
                guarantee = GridGuarantee::ParameterPlacementChanged;
            }
            GridOperation::DeleteSample { sample_id } => {
                let (i, j) = sample_index(&sample_id)?;
                if axes[i].samples.len() == 1 && !axes[i].parameter_id.is_empty() {
                    return Err(Status::error("AXIS_REDUCTION_REQUIRED", &axes[i].id));
                }
                let sample = axes[i].samples.remove(j);
                guarantee = GridGuarantee::SamplesRemoved {
                    parameter_id: axes[i].parameter_id.clone(),
                    value: sample.value,
                };
            }
            GridOperation::RemoveAxis {
                axis_id,
                reduction,
                keep_pending,
            } => {
                let i = axis_index(&axis_id)?;
                if !axes[i].parameter_id.is_empty() {
                    let value = reduce(&axes[i], reduction)?;
                    removed = Some((axis_id, value));
                    guarantee = GridGuarantee::DimensionRemoved {
                        parameter_id: axes[i].parameter_id.clone(),
                        value,
                    };
                }
                if keep_pending {
                    axes[i].parameter_id.clear();
                } else {
                    axes.remove(i);
                }
            }
        }
        let mut ids = std::collections::HashSet::new();
        let mut parameters = std::collections::HashSet::new();
        if axes.len() > 16 {
            return Err(Status::error("INVALID_BINDING", "At most 16 axes"));
        }
        for a in &mut axes {
            if !ids.insert(a.id.clone()) {
                return Err(Status::error("DUPLICATE_AXIS", &a.id));
            }
            a.keys = a.samples.iter().map(|s| s.value).collect();
            if !a.parameter_id.is_empty() {
                let p = self
                    .get_parameter(&a.parameter_id)
                    .ok_or_else(|| Status::error("MISSING_PARAMETER", &a.parameter_id))?;
                if p.kind != ParameterKind::Normal {
                    return Err(Status::error("INVALID_PARAMETER_KIND", &p.id));
                }
                if !parameters.insert(&p.id) {
                    return Err(Status::error("DUPLICATE_AXIS", &p.id));
                }
                check_keys(p, &a.keys)?;
            }
            for (i, s) in a.samples.iter().enumerate() {
                if i > 0 && s.value <= a.samples[i - 1].value {
                    return Err(Status::error("INVALID_KEYS", &a.id));
                }
                if !valid_uuid(&s.id) || !s.value.is_finite() || !ids.insert(s.id.clone()) {
                    return Err(Status::error("INVALID_SAMPLE_ID", &s.id));
                }
            }
        }
        let dims: Vec<_> = active(&axes).iter().map(|a| a.samples.len()).collect();
        // Inventories/provenance coexist with old and new coordinates. Each edit
        // changes at most one sampled dimension, hence at most two source slots.
        let address_bytes = old
            .len()
            .max(axes.len())
            .checked_mul(80)
            .and_then(|n| n.checked_add(192))
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", "Conversion addresses"))?;
        let bytes_per_form = bytes_per_form
            .checked_add(address_bytes)
            .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", "Conversion payload"))?;
        let cost = limits.check_grid(&dims, bytes_per_form, self.estimated_content_bytes())?;
        let new_active = active(&axes);
        let old_active = active(old);
        let mut weights = Vec::with_capacity(cost.sample_combinations);
        let mut coordinates = Vec::with_capacity(cost.sample_combinations);
        for mut slot in 0..cost.sample_combinations {
            control.checkpoint()?;
            let combination: Vec<_> = new_active
                .iter()
                .map(|a| {
                    let i = slot % a.samples.len();
                    slot /= a.samples.len();
                    &a.samples[i]
                })
                .collect();
            let values: Vec<_> = old_active
                .iter()
                .map(|a| {
                    if let Some((_, value)) = removed.as_ref().filter(|(id, _)| *id == a.id) {
                        return *value;
                    }
                    let i = new_active
                        .iter()
                        .position(|n| n.id == a.id)
                        .expect("active old axis must be retained or explicitly reduced");
                    a.samples
                        .iter()
                        .find(|s| s.id == combination[i].id)
                        .map_or(combination[i].value, |s| s.value)
                })
                .collect();
            weights.push(selection(old, &values));
            coordinates.push(combination.iter().map(|s| s.id.clone()).collect());
        }
        Ok(Lattice {
            axes,
            weights,
            coordinates,
            cost,
            guarantee,
        })
    }
}
pub(super) fn appearance(
    weights: &[(usize, f32)],
    get: impl Fn(usize) -> Appearance,
) -> Appearance {
    let mut out = Appearance {
        opacity: 0.,
        multiply: [0.; 3],
        screen: [0.; 3],
    };
    for &(i, w) in weights {
        let a = get(i);
        out.opacity += a.opacity * w;
        for c in 0..3 {
            out.multiply[c] += a.multiply[c] * w;
            out.screen[c] += a.screen[c] * w;
        }
    }
    out
}
pub(super) fn points<'a>(
    weights: &[(usize, f32)],
    get: impl Fn(usize) -> &'a [Vec2],
) -> Result<Vec<Vec2>, Status> {
    let mut out = vec![Vec2::default(); get(weights[0].0).len()];
    for &(i, w) in weights {
        let v = get(i);
        if v.len() != out.len() {
            return Err(Status::error("INVALID_LENGTH", "Shape dimensions differ"));
        }
        for (p, v) in out.iter_mut().zip(v) {
            p.x += v.x * w;
            p.y += v.y * w;
        }
    }
    require_ok(crate::geometry::validate_positions(&out))?;
    Ok(out)
}
pub(super) fn rotation(
    weights: &[(usize, f32)],
    get: impl Fn(usize) -> RotationPose,
) -> Result<RotationPose, Status> {
    let first = get(weights[0].0);
    let mut out = RotationPose {
        origin: crate::PreciseVec2::default(),
        angle: 0.,
        scale: 0.,
        reflect_x: first.reflect_x,
        reflect_y: first.reflect_y,
    };
    for &(i, w) in weights {
        let p = get(i);
        if p.reflect_x != first.reflect_x || p.reflect_y != first.reflect_y {
            return Err(Status::error(
                "DISCRETE_ROTATION_NOT_REPRESENTABLE",
                "Reflection changes across the resampled segment",
            ));
        }
        out.origin.x += p.origin.x * f64::from(w);
        out.origin.y += p.origin.y * f64::from(w);
        out.angle += p.angle * w;
        out.scale += p.scale * w;
    }
    Ok(out)
}
pub(super) fn bezier<'a>(
    weights: &[(usize, f32)],
    get: impl Fn(usize) -> Option<&'a WarpBezier>,
) -> Result<Option<WarpBezier>, Status> {
    let Some(first) = get(weights[0].0) else {
        if weights.iter().any(|(i, _)| get(*i).is_some()) {
            return Err(Status::error(
                "BEZIER_NOT_REPRESENTABLE",
                "Mixed controller presence",
            ));
        }
        return Ok(None);
    };
    let mut out = first.clone();
    for n in &mut out.nodes {
        n.position = Vec2::default();
        n.handles.fill(Vec2::default());
    }
    for &(i, w) in weights {
        let b =
            get(i).ok_or_else(|| Status::error("BEZIER_NOT_REPRESENTABLE", "Mixed controllers"))?;
        if b.rows != out.rows || b.columns != out.columns || b.nodes.len() != out.nodes.len() {
            return Err(Status::error(
                "BEZIER_NOT_REPRESENTABLE",
                "Controller topology differs",
            ));
        }
        for (n, s) in out.nodes.iter_mut().zip(&b.nodes) {
            n.position.x += s.position.x * w;
            n.position.y += s.position.y * w;
            for (h, s) in n.handles.iter_mut().zip(s.handles) {
                h.x += s.x * w;
                h.y += s.y * w;
            }
        }
    }
    Ok(Some(out))
}
