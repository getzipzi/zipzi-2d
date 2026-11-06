use super::*;
use crate::model_validation::binding_coordinates::normalize_axes;
impl Model<'_> {
    pub(crate) fn normalize_owner_axes(
        &self,
        owner: &str,
        axes: &mut [BindingAxis],
    ) -> Result<(), Status> {
        let old = self
            .get_binding(owner)
            .map(|b| b.axes.as_slice())
            .or_else(|| self.get_scene_binding(owner).map(|b| b.axes.as_slice()))
            .or_else(|| {
                self.get_glue(owner)
                    .and_then(|g| g.binding.as_ref())
                    .map(|b| b.axes.as_slice())
            });
        if let Some(old) = old {
            for axis in axes.iter_mut() {
                let mut matches = old.iter().filter(|a| {
                    if axis.id.is_empty() {
                        a.parameter_id == axis.parameter_id
                    } else {
                        a.id == axis.id
                    }
                });
                if let Some(previous) = matches.next().filter(|_| matches.next().is_none()) {
                    if axis.id.is_empty() {
                        axis.id = previous.id.clone();
                    }
                    if axis.samples.is_empty() && !axis.keys.is_empty() {
                        axis.samples = axis
                            .keys
                            .iter()
                            .map(|&value| BindingSample {
                                id: previous
                                    .samples
                                    .iter()
                                    .find(|s| s.value == value)
                                    .map_or_else(
                                        || uuid::Uuid::new_v4().to_string(),
                                        |s| s.id.clone(),
                                    ),
                                value,
                            })
                            .collect();
                    }
                }
            }
        }
        normalize_axes(axes)
    }
}
impl Model<'_> {
    pub(crate) fn normalize_table(&self, t: &mut BlendShapeKeyTable) -> Result<(), Status> {
        if t.samples.is_empty() && !t.keys.is_empty() {
            t.samples = t
                .keys
                .iter()
                .map(|&value| BindingSample {
                    id: self
                        .get_blend_key_table(&t.id)
                        .and_then(|old| old.samples.iter().find(|s| s.value == value))
                        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |s| s.id.clone()),
                    value,
                })
                .collect();
        }
        let keys: Vec<_> = t.samples.iter().map(|s| s.value).collect();
        if !t.keys.is_empty() && t.keys != keys {
            return Err(Status::error("STALE_SAMPLE_VALUES", &t.id));
        }
        if t.base_sample_id.is_empty() && !t.samples.is_empty() {
            t.base_sample_id = t
                .samples
                .get(t.base_key_idx)
                .ok_or_else(|| Status::error("INVALID_BASE_KEY", &t.id))?
                .id
                .clone();
        }
        t.base_key_idx = if t.samples.is_empty() {
            0
        } else {
            t.samples
                .iter()
                .position(|s| s.id == t.base_sample_id)
                .ok_or_else(|| Status::error("INVALID_BASE_KEY", &t.id))?
        };
        t.keys = keys;
        self.check_binding_ids(&t.id, t.samples.iter().map(|s| s.id.clone()))
    }
    pub(crate) fn normalize_constraint(&self, c: &mut BlendShapeConstraint) -> Result<(), Status> {
        if c.samples.is_empty() && !c.keys.is_empty() {
            if c.keys.len() != c.weights.len() {
                return Err(Status::error("INVALID_LENGTH", &c.id));
            }
            c.samples = c
                .keys
                .iter()
                .zip(&c.weights)
                .map(|(&value, &weight)| ConstraintSample {
                    id: self
                        .get_blend_constraint(&c.id)
                        .and_then(|old| old.samples.iter().find(|s| s.value == value))
                        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |s| s.id.clone()),
                    value,
                    weight,
                })
                .collect();
        }
        let keys: Vec<_> = c.samples.iter().map(|s| s.value).collect();
        let weights: Vec<_> = c.samples.iter().map(|s| s.weight).collect();
        if (!c.keys.is_empty() && c.keys != keys) || (!c.weights.is_empty() && c.weights != weights)
        {
            return Err(Status::error("STALE_SAMPLE_VALUES", &c.id));
        }
        c.keys = keys;
        c.weights = weights;
        self.check_binding_ids(&c.id, c.samples.iter().map(|s| s.id.clone()))
    }
    pub(crate) fn normalize_glue_forms(&self, g: &mut Glue) -> Result<(), Status> {
        let Some(b) = &mut g.binding else {
            return Ok(());
        };
        let active: Vec<_> = b
            .axes
            .iter()
            .filter(|a| !a.parameter_id.is_empty())
            .collect();
        if active.is_empty() {
            return if b.keyforms.is_empty() {
                Ok(())
            } else {
                Err(Status::error("ZERO_AXIS_KEYFORMS", &g.id))
            };
        }
        let total = active.iter().try_fold(1usize, |n, a| {
            n.checked_mul(a.samples.len())
                .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &g.id))
        })?;
        if b.keyforms.len() != total {
            return Err(Status::error("INCOMPLETE_KEYFORMS", &g.id));
        }
        let mut ordered: Vec<Option<crate::GlueKeyform>> = vec![None; total];
        for (implicit, mut f) in std::mem::take(&mut b.keyforms).into_iter().enumerate() {
            if f.samples.is_empty() {
                let mut slot = implicit;
                f.samples = active
                    .iter()
                    .map(|a| {
                        let i = slot % a.samples.len();
                        slot /= a.samples.len();
                        a.samples[i].id.clone()
                    })
                    .collect();
            }
            if f.samples.len() != active.len() {
                return Err(Status::error("INVALID_KEY_COMBINATION", &g.id));
            }
            f.samples = active
                .iter()
                .map(|axis| {
                    f.samples
                        .iter()
                        .find(|id| axis.samples.iter().any(|s| &s.id == *id))
                        .cloned()
                        .ok_or_else(|| Status::error("MISSING_SAMPLE", &axis.id))
                })
                .collect::<Result<_, _>>()?;
            let mut slot = 0;
            let mut stride = 1;
            for (a, id) in active.iter().zip(&f.samples) {
                slot += a
                    .samples
                    .iter()
                    .position(|s| &s.id == id)
                    .ok_or_else(|| Status::error("MISSING_SAMPLE", id))?
                    * stride;
                stride *= a.samples.len();
            }
            if ordered[slot].is_some() {
                return Err(Status::error("DUPLICATE_KEYFORM", &g.id));
            }
            ordered[slot] = Some(f);
        }
        b.keyforms = ordered.into_iter().map(Option::unwrap).collect();
        Ok(())
    }
    pub(crate) fn normalize_blend_forms(&self, b: &mut BlendShapeBinding) -> Result<(), Status> {
        let table = if b.key_table_id.is_empty() {
            None
        } else {
            Some(
                self.get_blend_key_table(&b.key_table_id)
                    .ok_or_else(|| Status::error("MISSING_KEY_TABLE", &b.key_table_id))?,
            )
        };
        let ids: Vec<_> = table.map_or_else(Vec::new, |t| {
            t.samples.iter().map(|s| s.id.clone()).collect()
        });
        macro_rules! normalize {
            ($forms:expr) => {{
                let forms = $forms;
                if forms.len() != ids.len() {
                    return Err(Status::error("INCOMPLETE_KEYFORMS", &b.id));
                }
                if forms.iter().all(|f| f.sample_id.is_empty()) {
                    for (f, id) in forms.iter_mut().zip(&ids) {
                        f.sample_id = id.clone();
                    }
                }
                let old = std::mem::take(forms);
                let mut map = std::collections::HashMap::new();
                for f in old {
                    if map.insert(f.sample_id.clone(), f).is_some() {
                        return Err(Status::error("DUPLICATE_KEYFORM", &b.id));
                    }
                }
                for id in &ids {
                    forms.push(
                        map.remove(id)
                            .ok_or_else(|| Status::error("MISSING_SAMPLE", id))?,
                    );
                }
            }};
        }
        match &mut b.keyforms {
            DeltaKeyforms::Mesh(f) => normalize!(f),
            DeltaKeyforms::Warp(f) => normalize!(f),
            DeltaKeyforms::Rotation(f) => normalize!(f),
            DeltaKeyforms::Part(f) => normalize!(f),
            DeltaKeyforms::Glue(f) => normalize!(f),
            DeltaKeyforms::Offscreen(f) => normalize!(f),
        }
        Ok(())
    }
}
