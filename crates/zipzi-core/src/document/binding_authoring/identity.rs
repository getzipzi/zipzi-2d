use super::*;
use crate::BindingSample;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingIdentityKind {
    Axis,
    Sample,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingIdentity {
    pub kind: BindingIdentityKind,
    pub id: String,
    pub owner: String,
    pub axis: Option<String>,
}

pub(crate) fn normalize_axes(axes: &mut [BindingAxis]) -> Result<(), Status> {
    if axes.len() > 16 {
        return Err(Status::error("INVALID_BINDING", "At most 16 axes"));
    }
    let mut ids = std::collections::HashSet::new();
    for a in axes {
        if a.id.is_empty() {
            a.id = uuid::Uuid::new_v4().to_string();
        }
        if !valid_uuid(&a.id) || !ids.insert(a.id.clone()) {
            return Err(Status::error("INVALID_AXIS_ID", &a.id));
        }
        if a.samples.is_empty() && !a.keys.is_empty() {
            a.samples = a
                .keys
                .iter()
                .map(|&value| BindingSample {
                    id: uuid::Uuid::new_v4().to_string(),
                    value,
                })
                .collect();
        }
        let values: Vec<_> = a.samples.iter().map(|s| s.value).collect();
        if !a.keys.is_empty() && a.keys != values {
            return Err(Status::error("STALE_SAMPLE_VALUES", &a.id));
        }
        for (i, s) in a.samples.iter().enumerate() {
            if i > 0 && s.value <= a.samples[i - 1].value {
                return Err(Status::error("INVALID_KEYS", &a.id));
            }
            if !valid_uuid(&s.id) || !ids.insert(s.id.clone()) {
                return Err(Status::error("INVALID_SAMPLE_ID", &s.id));
            }
            if !s.value.is_finite() {
                return Err(Status::error("INVALID_KEYS", &a.id));
            }
        }
        a.keys = values;
    }
    Ok(())
}
pub(crate) fn coordinates(
    axes: &[BindingAxis],
    keys: &mut Vec<f32>,
    samples: &mut Vec<String>,
) -> Result<(), Status> {
    let active: Vec<_> = axes.iter().filter(|a| !a.parameter_id.is_empty()).collect();
    if samples.is_empty() && !keys.is_empty() {
        if keys.len() != active.len() {
            return Err(Status::error(
                "INVALID_KEY_COMBINATION",
                "Coordinate dimension mismatch",
            ));
        }
        *samples = active
            .iter()
            .zip(keys.iter())
            .map(|(a, k)| {
                a.samples
                    .iter()
                    .find(|s| s.value == *k)
                    .map(|s| s.id.clone())
                    .ok_or_else(|| Status::error("INVALID_KEY_COMBINATION", &a.id))
            })
            .collect::<Result<_, _>>()?;
    }
    if samples.len() != active.len() {
        return Err(Status::error(
            "INVALID_KEY_COMBINATION",
            "Sample dimension mismatch",
        ));
    }
    *samples = active
        .iter()
        .map(|axis| {
            samples
                .iter()
                .find(|id| axis.samples.iter().any(|s| &s.id == *id))
                .cloned()
                .ok_or_else(|| Status::error("MISSING_SAMPLE", &axis.id))
        })
        .collect::<Result<_, _>>()?;
    let derived = active
        .iter()
        .zip(samples.iter())
        .map(|(a, id)| {
            a.samples
                .iter()
                .find(|s| &s.id == id)
                .map(|s| s.value)
                .ok_or_else(|| Status::error("MISSING_SAMPLE", id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !keys.is_empty() && *keys != derived {
        return Err(Status::error(
            "STALE_SAMPLE_VALUES",
            "Keyform coordinates changed",
        ));
    }
    *keys = derived;
    Ok(())
}
impl Document {
    /// Numeric import adapters may omit IDs. Reuse an unambiguous existing axis
    /// and samples at unchanged values; an actual move uses the stable-ID planner.
    pub(in crate::document) fn normalize_owner_axes(
        &self,
        owner: &str,
        axes: &mut [BindingAxis],
    ) -> Result<(), Status> {
        self.validation_model().normalize_owner_axes(owner, axes)
    }
    fn build_binding_identities(&self) -> Vec<BindingIdentity> {
        let mut result = Vec::new();
        let mut axes = |owner: &str, axes: &[BindingAxis]| {
            for a in axes {
                result.push(BindingIdentity {
                    kind: BindingIdentityKind::Axis,
                    id: a.id.clone(),
                    owner: owner.into(),
                    axis: None,
                });
                result.extend(a.samples.iter().map(|s| BindingIdentity {
                    kind: BindingIdentityKind::Sample,
                    id: s.id.clone(),
                    owner: owner.into(),
                    axis: Some(a.id.clone()),
                }));
            }
        };
        for b in self
            .binding_order()
            .iter()
            .filter_map(|id| self.get_binding(id))
        {
            axes(&b.id, &b.axes);
        }
        for b in self
            .scene_binding_order()
            .iter()
            .filter_map(|id| self.get_scene_binding(id))
        {
            axes(&b.id, &b.axes);
        }
        for g in self.glue_order().iter().filter_map(|id| self.get_glue(id)) {
            if let Some(b) = &g.binding {
                axes(&g.id, &b.axes);
            }
        }
        for t in self
            .blend_key_table_order()
            .iter()
            .filter_map(|id| self.get_blend_key_table(id))
        {
            result.extend(t.samples.iter().map(|s| BindingIdentity {
                kind: BindingIdentityKind::Sample,
                id: s.id.clone(),
                owner: t.id.clone(),
                axis: None,
            }));
        }
        for c in self
            .blend_constraint_order()
            .iter()
            .filter_map(|id| self.get_blend_constraint(id))
        {
            result.extend(c.samples.iter().map(|s| BindingIdentity {
                kind: BindingIdentityKind::Sample,
                id: s.id.clone(),
                owner: c.id.clone(),
                axis: None,
            }));
        }
        result
    }
    fn binding_lookup(&self) -> &BindingLookup {
        self.binding_lookup.get_or_init(|| {
            let entries = self.build_binding_identities();
            let slots = entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e.id.clone(), i))
                .collect();
            Arc::new(BindingLookup { entries, slots })
        })
    }
    pub fn binding_identities_iter(&self) -> impl Iterator<Item = &BindingIdentity> {
        self.binding_lookup().entries.iter()
    }
    pub fn binding_identities(&self) -> Vec<BindingIdentity> {
        self.binding_lookup().entries.clone()
    }
    pub fn binding_identity(&self, id: &str) -> Option<BindingIdentity> {
        let lookup = self.binding_lookup();
        lookup.slots.get(id).map(|&i| lookup.entries[i].clone())
    }
    pub(in crate::document) fn check_binding_ids(
        &self,
        owner: &str,
        ids: impl Iterator<Item = String>,
    ) -> Result<(), Status> {
        let existing = self.binding_lookup();
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if id == owner
                || !valid_uuid(&id)
                || !seen.insert(id.clone())
                || (existing
                    .slots
                    .get(&id)
                    .is_some_and(|&i| existing.entries[i].owner != owner)
                    || self.contains_base_identity(&id)
                    || self.reference_identity(&id).is_some()
                    || self.get_glue_pair(&id).is_some()
                    || self.get_motion_track(&id).is_some()
                    || self.get_motion_key(&id).is_some()
                    || self.get_motion_event(&id).is_some())
            {
                return Err(Status::error("DUPLICATE_ID", id));
            }
        }
        Ok(())
    }
    pub(in crate::document) fn normalize_table(
        &self,
        t: &mut BlendShapeKeyTable,
    ) -> Result<(), Status> {
        self.validation_model().normalize_table(t)
    }
    pub(in crate::document) fn normalize_constraint(
        &self,
        c: &mut BlendShapeConstraint,
    ) -> Result<(), Status> {
        self.validation_model().normalize_constraint(c)
    }
    pub(in crate::document) fn normalize_glue_forms(&self, g: &mut Glue) -> Result<(), Status> {
        self.validation_model().normalize_glue_forms(g)
    }
    pub fn binding_context_signature(&self, id: &str) -> Option<String> {
        let signature = |axes: &[BindingAxis]| {
            let mut axes = axes.to_vec();
            axes.sort_by(|a, b| a.id.cmp(&b.id));
            for a in &mut axes {
                a.samples.sort_by(|a, b| a.id.cmp(&b.id));
            }
            let domains: Vec<_> = axes
                .iter()
                .map(|a| {
                    self.get_parameter(&a.parameter_id)
                        .map(|p| (p.minimum, p.maximum, p.kind, p.repeat, p.decimal_places))
                })
                .collect();
            serde_json::to_string(&(axes, domains)).ok()
        };
        if let Some(b) = self.get_binding(id) {
            return signature(&b.axes);
        }
        if let Some(b) = self.get_scene_binding(id) {
            return signature(&b.axes);
        }
        if let Some(b) = self.get_glue(id).and_then(|g| g.binding.as_ref()) {
            return signature(&b.axes);
        }
        if let Some(t) = self.get_blend_key_table(id) {
            return serde_json::to_string(&(
                t,
                self.get_parameter(&t.parameter_id)
                    .map(|p| (p.minimum, p.maximum, p.kind, p.repeat, p.decimal_places)),
            ))
            .ok();
        }
        self.get_blend_constraint(id).and_then(|c| {
            serde_json::to_string(&(
                c,
                self.get_parameter(&c.parameter_id)
                    .map(|p| (p.minimum, p.maximum, p.kind, p.repeat, p.decimal_places)),
            ))
            .ok()
        })
    }
    pub(in crate::document) fn normalize_blend_forms(
        &self,
        b: &mut BlendShapeBinding,
    ) -> Result<(), Status> {
        self.validation_model().normalize_blend_forms(b)
    }
}

#[derive(Debug)]
pub(in crate::document) struct BindingLookup {
    entries: Vec<BindingIdentity>,
    slots: std::collections::HashMap<String, usize>,
}
