use super::*;

#[cfg(test)]
thread_local! {
    static VALIDATION_COUNTS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}
#[cfg(test)]
pub(crate) fn take_validation_counts() -> (usize, usize) {
    VALIDATION_COUNTS.with(|counts| counts.replace((0, 0)))
}
impl Model<'_> {
    // Touched bindings are normalized before whole-document identity admission.
    // The admitted path still checks references and keyforms against the candidate.
    fn normalize_binding_ids(&self, owner: &str, axes: &mut [BindingAxis]) -> Result<(), Status> {
        self.normalize_owner_axes(owner, axes)?;
        self.check_binding_ids(
            owner,
            axes.iter().flat_map(|axis| {
                std::iter::once(axis.id.clone()).chain(axis.samples.iter().map(|s| s.id.clone()))
            }),
        )
    }

    pub(crate) fn canonicalize_binding(&self, binding: &mut MeshBinding) -> Status {
        self.canonicalize_binding_forms(binding, false)
    }

    pub(crate) fn validate_admitted_binding(&self, binding: &MeshBinding) -> Status {
        #[cfg(test)]
        VALIDATION_COUNTS.with(|counts| {
            let (mesh, scene) = counts.get();
            counts.set((mesh + 1, scene));
        });
        self.canonicalize_binding_forms(&mut binding.clone(), true)
    }

    fn canonicalize_binding_forms(
        &self,
        b: &mut MeshBinding,
        identities_validated: bool,
    ) -> Status {
        #[cfg(feature = "bench-instrumentation")]
        let _span = crate::benchmark::span("core.canonicalize_binding");
        if !identities_validated {
            if let Err(status) = self.normalize_binding_ids(&b.id, &mut b.axes) {
                return status;
            }
        }
        if !valid_uuid(&b.id) {
            return Status::error(
                "INVALID_ID",
                format!("{}: binding requires a canonical UUID", b.id),
            );
        }
        let mesh = match self.get_mesh(&b.mesh_id) {
            Some(m) => m,
            None => {
                return Status::error("MISSING_MESH", format!("{}.mesh_id: {}", b.id, b.mesh_id));
            }
        };
        if let Some(old) = self.binding_for_mesh(&b.mesh_id) {
            if old.id != b.id {
                return Status::error(
                    "BINDING_CONFLICT",
                    format!("{}: mesh positions already bound by {}", b.id, old.id),
                );
            }
        }
        // An undriven binding owns no duplicate of the Mesh's base shape.
        if b.axes.iter().all(|a| a.parameter_id.is_empty()) {
            return if b.keyforms.is_empty() {
                Status::ok()
            } else {
                Status::error("ZERO_AXIS_KEYFORMS", "Edit the target base shape instead")
            };
        }
        if b.axes.len() > 16 {
            return Status::error(
                "INVALID_BINDING",
                format!("{}: bindings support 1..16 axes", b.id),
            );
        }
        let mut total = 1usize;
        let mut seen = HashSet::new();
        for axis in b.axes.iter().filter(|a| !a.parameter_id.is_empty()) {
            let p = match self.get_parameter(&axis.parameter_id) {
                Some(p) => p,
                None => {
                    return Status::error(
                        "MISSING_PARAMETER",
                        format!("{}.axes: {}", b.id, axis.parameter_id),
                    );
                }
            };
            if !seen.insert(p.id.clone()) {
                return Status::error("DUPLICATE_AXIS", format!("{}.axes: {}", b.id, p.id));
            }
            if p.kind != ParameterKind::Normal {
                return Status::error("INVALID_PARAMETER_KIND", &p.id);
            }
            if axis.keys.is_empty() || axis.keys.len() > (i32::MAX as usize) / total {
                return Status::error(
                    "INVALID_KEYS",
                    format!("{}.axes: empty or overflowing key grid", b.id),
                );
            }
            total *= axis.keys.len();
            for (i, &k) in axis.keys.iter().enumerate() {
                if !k.is_finite()
                    || k < p.minimum
                    || k > p.maximum
                    || (i > 0 && k <= axis.keys[i - 1])
                {
                    return Status::error(
                        "INVALID_KEYS",
                        format!(
                            "{}.axes[{}]: finite, strictly increasing keys within parameter range required",
                            b.id, p.id
                        ),
                    );
                }
            }
        }
        if b.keyforms.len() != total {
            return Status::error(
                "INCOMPLETE_KEYFORMS",
                format!("{}: require every Cartesian key combination", b.id),
            );
        }
        let mut ordered = vec![MeshKeyform::default(); total];
        let mut occupied = vec![false; total];
        for mut form in b.keyforms.drain(..) {
            if let Err(s) = crate::model_validation::binding_coordinates::coordinates(
                &b.axes,
                &mut form.keys,
                &mut form.samples,
            ) {
                return s;
            }
            if form.keys.len() != b.axes.iter().filter(|a| !a.parameter_id.is_empty()).count()
                || form.positions.len() != mesh.vertex_ids.len()
            {
                return Status::error(
                    "INVALID_LENGTH",
                    format!(
                        "{}.keyforms: axes and positions must match binding and mesh",
                        b.id
                    ),
                );
            }
            let s = validate_positions(&form.positions);
            if !s.is_ok() {
                return Status::error(s.code, format!("{}.keyforms: {}", b.id, s.message));
            }
            let s = validate_appearance(&form.appearance, &b.id);
            if !s.is_ok() {
                return s;
            }
            if let Some(order) = form.draw_order {
                let s = validate_draw_order(order, &b.id);
                if !s.is_ok() {
                    return s;
                }
            }
            let mut index = 0usize;
            let mut stride = 1usize;
            for (a, axis) in b
                .axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty())
                .enumerate()
            {
                let key_pos = match axis.keys.iter().position(|&k| k == form.keys[a]) {
                    Some(idx) => idx,
                    None => {
                        return Status::error(
                            "INVALID_KEY_COMBINATION",
                            format!("{}.keyforms: unknown key value", b.id),
                        );
                    }
                };
                index += key_pos * stride;
                stride *= axis.keys.len();
            }
            if occupied[index] {
                return Status::error(
                    "DUPLICATE_KEYFORM",
                    format!("{}.keyforms: repeated combination", b.id),
                );
            }
            occupied[index] = true;
            ordered[index] = form;
        }
        b.keyforms = ordered;
        Status::ok()
    }
    pub(crate) fn canonicalize_scene_binding(&self, binding: &mut SceneBinding) -> Status {
        self.canonicalize_scene_binding_forms(binding, false)
    }

    pub(crate) fn validate_admitted_scene_binding(&self, binding: &SceneBinding) -> Status {
        #[cfg(test)]
        VALIDATION_COUNTS.with(|counts| {
            let (mesh, scene) = counts.get();
            counts.set((mesh, scene + 1));
        });
        self.canonicalize_scene_binding_forms(&mut binding.clone(), true)
    }

    fn canonicalize_scene_binding_forms(
        &self,
        b: &mut SceneBinding,
        identities_validated: bool,
    ) -> Status {
        #[cfg(feature = "bench-instrumentation")]
        let _span = crate::benchmark::span("core.canonicalize_scene_binding");
        if !identities_validated {
            if let Err(status) = self.normalize_binding_ids(&b.id, &mut b.axes) {
                return status;
            }
        }
        if !valid_uuid(&b.id) {
            return Status::error("INVALID_ID", &b.id);
        }
        let t = self.get_transform(b.target_id());
        let p = self.get_part(b.target_id());
        if t.is_none() && p.is_none() {
            return Status::error("MISSING_OBJECT", b.target_id());
        }
        if let Some(old) = self.binding_for_scene(b.target_id()) {
            if old.id != b.id {
                return Status::error("BINDING_CONFLICT", b.target_id());
            }
        }
        let target_matches = match &b.track {
            SceneTrack::Warp { .. } => t.is_some_and(|t| t.kind() == TransformKind::Warp),
            SceneTrack::Rotation { .. } => t.is_some_and(|t| t.kind() == TransformKind::Rotation),
            SceneTrack::Part { .. } => p.is_some(),
        };
        if !target_matches {
            return Status::error("INVALID_BINDING_TARGET", &b.id);
        }
        if b.axes.iter().all(|a| a.parameter_id.is_empty()) {
            return if b.track.is_empty() {
                Status::ok()
            } else {
                Status::error("ZERO_AXIS_KEYFORMS", &b.id)
            };
        }
        if b.axes.len() > 16 {
            return Status::error("INVALID_BINDING", &b.id);
        }
        let mut total = 1usize;
        let mut seen = HashSet::new();
        for a in b.axes.iter().filter(|a| !a.parameter_id.is_empty()) {
            let param = match self.get_parameter(&a.parameter_id) {
                Some(param) => param,
                None => return Status::error("MISSING_PARAMETER", &a.parameter_id),
            };
            if !seen.insert(a.parameter_id.clone()) {
                return Status::error("DUPLICATE_AXIS", &b.id);
            }
            if param.kind != ParameterKind::Normal {
                return Status::error("INVALID_PARAMETER_KIND", &param.id);
            }
            if a.keys.is_empty() || a.keys.len() > (i32::MAX as usize) / total {
                return Status::error("INVALID_KEYS", &b.id);
            }
            total *= a.keys.len();
            for (i, &k) in a.keys.iter().enumerate() {
                if !k.is_finite()
                    || k < param.minimum
                    || k > param.maximum
                    || (i > 0 && k <= a.keys[i - 1])
                {
                    return Status::error("INVALID_KEYS", &b.id);
                }
            }
        }
        if total != b.track.len() {
            return Status::error("INCOMPLETE_KEYFORMS", &b.id);
        }
        let matches_target = match &b.track {
            crate::SceneTrack::Warp { .. } => t.is_some_and(|t| t.kind() == TransformKind::Warp),
            crate::SceneTrack::Rotation { .. } => {
                t.is_some_and(|t| t.kind() == TransformKind::Rotation)
            }
            crate::SceneTrack::Part { .. } => p.is_some(),
        };
        if !matches_target {
            return Status::error("INVALID_BINDING_TARGET", &b.id);
        }
        match &b.track {
            crate::SceneTrack::Warp { keyforms, .. } => {
                let point_count = t.unwrap().warp().unwrap().points.len();
                for f in keyforms {
                    if f.positions.len() != point_count {
                        return Status::error("INVALID_LENGTH", &b.id);
                    }
                    if let Some(b) = &f.bezier {
                        if t.unwrap()
                            .warp_authoring
                            .as_ref()
                            .and_then(|s| s.bezier_grid)
                            .is_some_and(|g| (g.rows, g.columns) != (b.rows, b.columns))
                        {
                            return Status::error(
                                "INVALID_WARP_BEZIER",
                                "Keyform controller density differs from the Warp editing configuration",
                            );
                        }
                        let status = b.validate();
                        if !status.is_ok() {
                            return status;
                        }
                    }
                    let status = validate_positions(&f.positions);
                    if !status.is_ok() {
                        return status;
                    }
                    let status = validate_appearance(&f.appearance, &b.id);
                    if !status.is_ok() {
                        return status;
                    }
                }
            }
            crate::SceneTrack::Rotation { keyforms, .. } => {
                for f in keyforms {
                    let status = pose_valid(&f.rotation, &b.id);
                    if !status.is_ok() {
                        return status;
                    }
                    let status = validate_appearance(&f.appearance, &b.id);
                    if !status.is_ok() {
                        return status;
                    }
                }
            }
            crate::SceneTrack::Part { keyforms, .. } => {
                for f in keyforms {
                    let status = validate_draw_order(f.draw_order, &b.id);
                    if !status.is_ok() {
                        return status;
                    }
                }
            }
        }
        let normalize = |keys: &mut Vec<f32>, samples: &mut Vec<String>| {
            crate::model_validation::binding_coordinates::coordinates(&b.axes, keys, samples)
        };
        let status: Result<(), Status> = match &mut b.track {
            SceneTrack::Warp { keyforms, .. } => keyforms
                .iter_mut()
                .try_for_each(|f| normalize(&mut f.keys, &mut f.samples)),
            SceneTrack::Rotation { keyforms, .. } => keyforms
                .iter_mut()
                .try_for_each(|f| normalize(&mut f.keys, &mut f.samples)),
            SceneTrack::Part { keyforms, .. } => keyforms
                .iter_mut()
                .try_for_each(|f| normalize(&mut f.keys, &mut f.samples)),
        };
        if let Err(s) = status {
            return s;
        }
        let mut ordered = vec![0; total];
        let mut occupied = vec![false; total];
        for (source_slot, f) in b.track.samples().enumerate() {
            if f.keys.len() != b.axes.iter().filter(|a| !a.parameter_id.is_empty()).count() {
                return Status::error("INVALID_LENGTH", &b.id);
            }
            let mut index = 0usize;
            let mut stride = 1usize;
            for (a, axis) in b
                .axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty())
                .enumerate()
            {
                let key_pos = match axis.keys.iter().position(|&k| k == f.keys[a]) {
                    Some(pos) => pos,
                    None => return Status::error("INVALID_KEY_COMBINATION", &b.id),
                };
                index += key_pos * stride;
                stride *= axis.keys.len();
            }
            if occupied[index] {
                return Status::error("DUPLICATE_KEYFORM", &b.id);
            }
            occupied[index] = true;
            ordered[index] = source_slot;
        }
        if let Some(os) = self.offscreen_for_part(b.target_id()) {
            if !os.part_keyform_indices.is_empty() && os.part_keyform_indices.len() != total {
                return Status::error(
                    "INVALID_LENGTH",
                    format!(
                        "{}: update offscreen mapping before changing Part keyform count",
                        os.id
                    ),
                );
            }
        }
        if let Some(previous) = self.get_scene_binding(&b.id) {
            if previous.target_id() != b.target_id() {
                if let Some(os) = self.offscreen_for_part(previous.target_id()) {
                    if !os.part_keyform_indices.is_empty() {
                        return Status::error("OBJECT_REFERENCED", &os.id);
                    }
                }
            }
        }
        b.track.reorder(&ordered);
        Status::ok()
    }
}
