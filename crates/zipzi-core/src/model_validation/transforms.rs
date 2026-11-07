use super::*;
impl Model<'_> {
    pub(crate) fn validate_transform(&self, t: &Transform) -> Status {
        let (minimum, maximum) = t.rotation_scale_bounds();
        if t.rotation_scale_unit.is_some()
            && (t.rotation().is_none()
                || minimum <= 0.
                || !minimum.is_finite()
                || !maximum.is_finite())
        {
            return Status::error("INVALID_ROTATION", format!("{}.rotation_scale_unit", t.id));
        }
        if let Some(record) = &t.original_shape {
            if let Err(status) = self.validate_deformer_original(record) {
                return status;
            }
        }
        if !valid_uuid(&t.id) || t.runtime_id.is_empty() {
            return Status::error("INVALID_ID", &t.id);
        }
        if t.parent_id
            .as_ref()
            .is_some_and(|id| !valid_uuid(id.as_str()))
            || t.part_id
                .as_ref()
                .is_some_and(|id| !valid_uuid(id.as_str()))
        {
            return Status::error("INVALID_ID", &t.id);
        }
        for (id, other) in &self.content.transforms {
            if id != &t.id && other.runtime_id == t.runtime_id {
                return Status::error("DUPLICATE_RUNTIME_ID", &t.id);
            }
        }
        if !t.part_id.is_none() && !self.content.parts.contains_key(t.part()) {
            return Status::error("MISSING_PART", format!("{}.part_id", t.id));
        }
        let mut seen = HashSet::new();
        seen.insert(t.id.clone());
        let mut parent_id = t.parent().to_owned();
        while !parent_id.is_empty() {
            if !seen.insert(parent_id.clone()) {
                return Status::error("RELATION_CYCLE", format!("{}.parent_id", t.id));
            }
            match self.get_transform(&parent_id) {
                Some(parent) => parent_id = parent.parent().to_owned(),
                None => return Status::error("MISSING_TRANSFORM", parent_id),
            }
        }
        match &t.data {
            crate::TransformData::Rotation(r) => {
                if t.warp_authoring.is_some() {
                    return Status::error(
                        "NOT_WARP",
                        "Warp authoring settings require a Warp transform",
                    );
                }
                if !r.base_angle.is_finite() {
                    return Status::error("NON_FINITE", format!("{}.base_angle", t.id));
                }
                let status = pose_valid(&r.pose, &t.id);
                if !status.is_ok() {
                    return status;
                }
            }
            crate::TransformData::Warp(w) => {
                if let Some(grid) = t.warp_authoring.as_ref().and_then(|s| s.bezier_grid) {
                    let status = grid.validate();
                    if !status.is_ok() {
                        return status;
                    }
                    if w.bezier
                        .as_ref()
                        .is_some_and(|b| (b.rows, b.columns) != (grid.rows, grid.columns))
                    {
                        return Status::error(
                            "INVALID_WARP_BEZIER",
                            "Controller density differs from the Warp editing configuration",
                        );
                    }
                }
                if w.rows == 0
                    || w.columns == 0
                    || w.rows > 1024
                    || w.columns > 1024
                    || w.points.len() != ((w.rows + 1) * (w.columns + 1)) as usize
                {
                    return Status::error("INVALID_WARP_GRID", &t.id);
                }
                if let Some(b) = &w.bezier {
                    let status = b.validate();
                    if !status.is_ok() {
                        return status;
                    }
                }
                let status = validate_positions(&w.points);
                if !status.is_ok() {
                    return status;
                }
            }
        }
        validate_appearance(&t.appearance, &t.id)
    }
}
