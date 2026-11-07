use super::*;
impl Model<'_> {
    pub(crate) fn validate_glue(&self, glue: &Glue) -> Status {
        if !valid_uuid(&glue.id) {
            return Status::error("INVALID_ID", &glue.id);
        }
        for reference in [&glue.mesh_a_id, &glue.mesh_b_id] {
            let status = reference.validate();
            if !status.is_ok() {
                return status;
            }
            if reference
                .resolved_id()
                .is_some_and(|id| !self.content.meshes.contains_key(id))
            {
                return Status::error("MISSING_MESH", reference.as_str());
            }
        }
        let slots_a = glue
            .mesh_a_id
            .resolved_id()
            .and_then(|id| self.get_mesh(id));
        let slots_b = glue
            .mesh_b_id
            .resolved_id()
            .and_then(|id| self.get_mesh(id));
        let mut ids = HashSet::new();
        for pair in &glue.pairs {
            if !valid_uuid(&pair.id)
                || pair.id == glue.id
                || glue.binding.as_ref().is_some_and(|b| {
                    b.axes
                        .iter()
                        .any(|a| a.id == pair.id || a.samples.iter().any(|s| s.id == pair.id))
                })
                || !ids.insert(pair.id.as_str())
                || (self.contains_id(&pair.id)
                    && self.glue_pair_owner(&pair.id) != Some(glue.id.as_str()))
            {
                return Status::error("DUPLICATE_PAIR_ID", &pair.id);
            }
            if pair
                .vertex_a
                .is_some_and(|v| slots_a.is_some_and(|slots| !slots.vertex_ids.contains(&v)))
            {
                return Status::error(
                    "MISSING_VERTEX",
                    format!(
                        "{}: vertex {} not in mesh_a",
                        glue.id,
                        pair.vertex_a.unwrap()
                    ),
                );
            }
            if pair
                .vertex_b
                .is_some_and(|v| slots_b.is_some_and(|slots| !slots.vertex_ids.contains(&v)))
            {
                return Status::error(
                    "MISSING_VERTEX",
                    format!(
                        "{}: vertex {} not in mesh_b",
                        glue.id,
                        pair.vertex_b.unwrap()
                    ),
                );
            }
            if !pair.weight_a.is_finite()
                || pair.weight_a < 0.0
                || !pair.weight_b.is_finite()
                || pair.weight_b < 0.0
            {
                return Status::error(
                    "INVALID_WEIGHT",
                    format!("{}: weights must be finite and non-negative", glue.id),
                );
            }
        }
        if !glue.intensity.is_finite() {
            return Status::error(
                "INVALID_INTENSITY",
                format!("{}: intensity must be finite", glue.id),
            );
        }
        if let Some(binding) = &glue.binding {
            if binding.axes.len() > 16 {
                return Status::error("INVALID_BINDING", "Glue requires 1..16 axes");
            }
            let mut total = if binding.axes.iter().all(|a| a.parameter_id.is_empty()) {
                0usize
            } else {
                1usize
            };
            let mut seen = HashSet::new();
            for axis in binding.axes.iter().filter(|a| !a.parameter_id.is_empty()) {
                let Some(p) = self.get_parameter(&axis.parameter_id) else {
                    return Status::error("MISSING_PARAMETER", &axis.parameter_id);
                };
                if p.kind != ParameterKind::Normal {
                    return Status::error("INVALID_PARAMETER_KIND", &p.id);
                }
                if !seen.insert(&axis.parameter_id) {
                    return Status::error("DUPLICATE_AXIS", &p.id);
                }
                if axis.keys.is_empty() || axis.keys.len() > i32::MAX as usize / total {
                    return Status::error("INVALID_KEYS", &glue.id);
                }
                total *= axis.keys.len();
                for (i, &key) in axis.keys.iter().enumerate() {
                    if !key.is_finite()
                        || key < p.minimum
                        || key > p.maximum
                        || (i > 0 && key <= axis.keys[i - 1])
                    {
                        return Status::error("INVALID_KEYS", &p.id);
                    }
                }
            }
            if binding.keyforms.len() != total {
                return Status::error("INCOMPLETE_KEYFORMS", &glue.id);
            }
            let active: Vec<_> = binding
                .axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty())
                .collect();
            for (mut slot, f) in binding.keyforms.iter().enumerate() {
                let expected: Vec<_> = active
                    .iter()
                    .map(|a| {
                        let i = slot % a.samples.len();
                        slot /= a.samples.len();
                        &a.samples[i].id
                    })
                    .collect();
                if f.samples.iter().ne(expected.into_iter()) {
                    return Status::error("INVALID_SAMPLE_REFERENCE", &glue.id);
                }
            }
            if binding.keyforms.iter().any(|k| !k.intensity.is_finite()) {
                return Status::error("INVALID_GLUE", "Glue keyform intensity must be finite");
            }
        }
        Status::ok()
    }
}
