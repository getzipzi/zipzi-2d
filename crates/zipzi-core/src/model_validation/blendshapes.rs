use super::*;
impl Model<'_> {
    pub fn blend_binding_active(&self, b: &BlendShapeBinding) -> bool {
        self.get_blend_key_table(&b.key_table_id)
            .is_some_and(|t| !t.parameter_id.is_empty() && !t.keys.is_empty())
            && b.constraint_ids.iter().all(|id| {
                self.get_blend_constraint(id)
                    .is_some_and(|c| !c.parameter_id.is_empty() && !c.keys.is_empty())
            })
    }

    pub(crate) fn validate_blend_key_table(&self, table: &BlendShapeKeyTable) -> Status {
        if !valid_uuid(&table.id) {
            return Status::error("INVALID_ID", &table.id);
        }
        if table.keys != table.samples.iter().map(|s| s.value).collect::<Vec<_>>()
            || (!table.samples.is_empty()
                && table
                    .samples
                    .get(table.base_key_idx)
                    .is_none_or(|s| s.id != table.base_sample_id))
        {
            return Status::error("STALE_SAMPLE_VALUES", &table.id);
        }
        if table.parameter_id.is_empty() {
            return if table.samples.is_empty() && table.base_sample_id.is_empty()
                || !table.samples.is_empty()
                    && table
                        .keys
                        .iter()
                        .enumerate()
                        .all(|(i, k)| k.is_finite() && (i == 0 || *k > table.keys[i - 1]))
            {
                Status::ok()
            } else {
                Status::error("INVALID_KEYS", &table.id)
            };
        }
        let param = match self.get_parameter(&table.parameter_id) {
            Some(p) => p,
            None => return Status::error("MISSING_PARAMETER", &table.parameter_id),
        };
        if param.kind != ParameterKind::BlendShape {
            return Status::error(
                "INVALID_PARAMETER_KIND",
                format!("{}: expected blend_shape parameter", table.parameter_id),
            );
        }
        if table.keys.is_empty() {
            return Status::error("INVALID_KEYS", "Key table must contain at least one key");
        }
        if table.base_key_idx >= table.keys.len() {
            return Status::error("INVALID_BASE_KEY", "Base key index out of bounds");
        }
        for (i, &k) in table.keys.iter().enumerate() {
            if !k.is_finite()
                || k < param.minimum
                || k > param.maximum
                || (i > 0 && k <= table.keys[i - 1])
            {
                return Status::error(
                    "INVALID_KEYS",
                    "Keys must be finite, strictly increasing, and within parameter range",
                );
            }
        }
        Status::ok()
    }
    pub(crate) fn validate_blend_constraint(&self, constraint: &BlendShapeConstraint) -> Status {
        if !valid_uuid(&constraint.id) {
            return Status::error("INVALID_ID", &constraint.id);
        }
        if constraint.keys
            != constraint
                .samples
                .iter()
                .map(|s| s.value)
                .collect::<Vec<_>>()
            || constraint.weights
                != constraint
                    .samples
                    .iter()
                    .map(|s| s.weight)
                    .collect::<Vec<_>>()
        {
            return Status::error("STALE_SAMPLE_VALUES", &constraint.id);
        }
        if constraint.parameter_id.is_empty() {
            return if constraint.samples.iter().enumerate().all(|(i, s)| {
                s.value.is_finite()
                    && s.weight.is_finite()
                    && s.weight >= 0.
                    && (i == 0 || s.value > constraint.samples[i - 1].value)
            }) {
                Status::ok()
            } else {
                Status::error("INVALID_KEYS", &constraint.id)
            };
        }
        let param = match self.get_parameter(&constraint.parameter_id) {
            Some(p) => p,
            None => return Status::error("MISSING_PARAMETER", &constraint.parameter_id),
        };
        if constraint.keys.len() != constraint.weights.len() || constraint.keys.is_empty() {
            return Status::error(
                "INVALID_LENGTH",
                "Constraint keys and weights must have matching non-zero lengths",
            );
        }
        for (i, (&k, &w)) in constraint
            .keys
            .iter()
            .zip(constraint.weights.iter())
            .enumerate()
        {
            if !k.is_finite()
                || k < param.minimum
                || k > param.maximum
                || (i > 0 && k <= constraint.keys[i - 1])
            {
                return Status::error(
                    "INVALID_KEYS",
                    "Constraint keys must be finite, strictly increasing, and within parameter range",
                );
            }
            if !w.is_finite() || w < 0.0 {
                return Status::error(
                    "INVALID_WEIGHT",
                    "Constraint weights must be finite and non-negative",
                );
            }
        }
        Status::ok()
    }
    pub(crate) fn validate_blend_binding(&self, b: &BlendShapeBinding) -> Status {
        if !valid_uuid(&b.id) {
            return Status::error("INVALID_ID", &b.id);
        }
        let key_table = if b.key_table_id.is_empty() {
            None
        } else {
            match self.get_blend_key_table(&b.key_table_id) {
                Some(t) => Some(t),
                None => return Status::error("MISSING_KEY_TABLE", &b.key_table_id),
            }
        };
        for c_id in &b.constraint_ids {
            if !self.content.blend_constraints.contains_key(c_id) {
                return Status::error("MISSING_CONSTRAINT", c_id);
            }
        }
        if b.keyforms.len() != key_table.map_or(0, |t| t.keys.len()) {
            return Status::error(
                "INCOMPLETE_KEYFORMS",
                format!(
                    "{}: keyforms length {} does not match key table {}",
                    b.id,
                    b.keyforms.len(),
                    key_table.map_or(0, |t| t.keys.len())
                ),
            );
        }
        let expected: Vec<_> = key_table.map_or_else(Vec::new, |t| {
            t.samples.iter().map(|s| s.id.as_str()).collect()
        });
        let ids: Vec<_> = match &b.keyforms {
            DeltaKeyforms::Mesh(f) => f.iter().map(|f| f.sample_id.as_str()).collect(),
            DeltaKeyforms::Warp(f) => f.iter().map(|f| f.sample_id.as_str()).collect(),
            DeltaKeyforms::Rotation(f) => f.iter().map(|f| f.sample_id.as_str()).collect(),
            DeltaKeyforms::Part(f) => f.iter().map(|f| f.sample_id.as_str()).collect(),
            DeltaKeyforms::Glue(f) => f.iter().map(|f| f.sample_id.as_str()).collect(),
            DeltaKeyforms::Offscreen(f) => f.iter().map(|f| f.sample_id.as_str()).collect(),
        };
        if ids != expected {
            return Status::error("INVALID_SAMPLE_REFERENCE", &b.id);
        }
        match (&b.target_kind, &b.keyforms) {
            (BlendShapeTargetKind::Mesh, DeltaKeyforms::Mesh(forms)) => {
                let mesh = match self.get_mesh(&b.target_id) {
                    Some(m) => m,
                    None => return Status::error("MISSING_MESH", &b.target_id),
                };
                for f in forms {
                    if f.positions.len() != mesh.vertex_ids.len() {
                        return Status::error(
                            "INVALID_LENGTH",
                            format!(
                                "{}: delta positions len {} != mesh vertex len {}",
                                b.id,
                                f.positions.len(),
                                mesh.vertex_ids.len()
                            ),
                        );
                    }
                    for p in &f.positions {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            return Status::error(
                                "INVALID_POSITION",
                                format!("{}: non-finite delta position", b.id),
                            );
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(order) = f.draw_order {
                        let s = validate_draw_order(order, &b.id);
                        if !s.is_ok() {
                            return s;
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Warp, DeltaKeyforms::Warp(forms)) => {
                let warp = match self.get_transform(&b.target_id) {
                    Some(w) if w.kind() == TransformKind::Warp => w,
                    _ => {
                        return Status::error(
                            "MISSING_OBJECT",
                            format!("{}: expected Warp transform", b.target_id),
                        )
                    }
                };
                for f in forms {
                    if f.points.len() != warp.warp().unwrap().points.len() {
                        return Status::error(
                            "INVALID_LENGTH",
                            format!(
                                "{}: delta points len {} != warp points len {}",
                                b.id,
                                f.points.len(),
                                warp.warp().unwrap().points.len()
                            ),
                        );
                    }
                    for p in &f.points {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            return Status::error(
                                "INVALID_POSITION",
                                format!("{}: non-finite delta point", b.id),
                            );
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Rotation, DeltaKeyforms::Rotation(forms)) => {
                match self.get_transform(&b.target_id) {
                    Some(r) if r.kind() == TransformKind::Rotation => r,
                    _ => {
                        return Status::error(
                            "MISSING_OBJECT",
                            format!("{}: expected Rotation transform", b.target_id),
                        )
                    }
                };
                for f in forms {
                    if let Some(orig) = f.origin {
                        if !orig.x.is_finite() || !orig.y.is_finite() {
                            return Status::error("INVALID_POSITION", &b.id);
                        }
                    }
                    if let Some(angle) = f.angle {
                        if !angle.is_finite() {
                            return Status::error("INVALID_ROTATION", &b.id);
                        }
                    }
                    if let Some(scale) = f.scale {
                        if !scale.is_finite() {
                            return Status::error("INVALID_ROTATION", &b.id);
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Part, DeltaKeyforms::Part(forms)) => {
                if !self.content.parts.contains_key(&b.target_id) {
                    return Status::error(
                        "MISSING_OBJECT",
                        format!("{}: expected Part", b.target_id),
                    );
                }
                for f in forms {
                    let s = validate_draw_order(f.draw_order, &b.id);
                    if !s.is_ok() {
                        return s;
                    }
                }
            }
            (BlendShapeTargetKind::Glue, DeltaKeyforms::Glue(forms)) => {
                if !self.content.glues.contains_key(&b.target_id) {
                    return Status::error(
                        "MISSING_OBJECT",
                        format!("{}: expected Glue", b.target_id),
                    );
                }
                for f in forms {
                    if !f.intensity.is_finite() {
                        return Status::error("INVALID_INTENSITY", &b.id);
                    }
                }
            }
            (BlendShapeTargetKind::Offscreen, DeltaKeyforms::Offscreen(forms)) => {
                if !self.content.offscreens.contains_key(&b.target_id) {
                    return Status::error(
                        "MISSING_OBJECT",
                        format!("{}: expected Offscreen", b.target_id),
                    );
                }
                for f in forms {
                    if !f.opacity.is_finite() {
                        return Status::error("INVALID_OPACITY", &b.id);
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            _ => {
                return Status::error(
                    "MISMATCHED_TARGET_KIND",
                    format!(
                        "{}: target_kind {:?} does not match keyform type",
                        b.id, b.target_kind
                    ),
                );
            }
        }
        Status::ok()
    }
}
