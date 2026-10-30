use super::*;
use crate::geometry::validate_positions;
use crate::types::{
    ChangeKind, EditResult, MeshBinding, MeshKeyform, Offscreen, ParameterKind, SceneBinding,
    SceneKeyform, SceneTrack, Status, TransformKind,
};
use std::collections::HashSet;
use zipzi_core::SceneTrackEvaluation;

impl Document {
    pub fn binding_order(&self) -> &[String] {
        &self.content.binding_order
    }

    pub fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.content.bindings.get(id)
    }

    pub fn binding_for_mesh(&self, mesh_id: &str) -> Option<&MeshBinding> {
        self.lookup()
            .mesh_bindings
            .get(mesh_id)
            .and_then(|id| self.content.bindings.get(id))
    }

    pub(super) fn canonicalize_binding(&self, b: &mut MeshBinding) -> Status {
        #[cfg(feature = "bench-instrumentation")]
        let _span = crate::benchmark::span("core.canonicalize_binding");
        if let Err(s) = self.normalize_owner_axes(&b.id, &mut b.axes) {
            return s;
        }
        if let Err(s) = self.check_binding_ids(
            &b.id,
            b.axes.iter().flat_map(|a| {
                std::iter::once(a.id.clone()).chain(a.samples.iter().map(|s| s.id.clone()))
            }),
        ) {
            return s;
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
                return Status::error("MISSING_MESH", format!("{}.mesh_id: {}", b.id, b.mesh_id))
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
                    )
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
            if let Err(s) = super::binding_authoring::identity::coordinates(
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
                        )
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

    pub fn create_binding(&mut self, mut b: MeshBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&b.mesh_id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let mesh = b.mesh_id.clone();
        self.content.bindings.insert(id.clone(), b);
        self.content.binding_order.push(id.clone());
        self.changed(ChangeKind::Structure, vec![mesh], vec![id])
    }

    pub fn replace_binding(&mut self, mut b: MeshBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&b.mesh_id) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&b.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let previous_mesh = match self.content.bindings.get(&b.id) {
            Some(old) => old.mesh_id.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", &b.id)),
        };
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let mesh = b.mesh_id.clone();
        let mut affected = vec![mesh.clone()];
        if previous_mesh != mesh {
            affected.push(previous_mesh.clone());
        }
        let tracked =
            super::tracking::binding_payload_is_tracked(self.get_binding(&id).unwrap(), &b)
                .then(|| self.track_binding_payload_write(&id));
        self.content.bindings.insert(id.clone(), b);
        if let Some(write) = tracked {
            self.changed_tracked(
                write,
                ChangeKind::Structure,
                affected,
                vec![id, mesh, previous_mesh],
            )
        } else {
            self.changed(
                ChangeKind::Structure,
                affected,
                vec![id, mesh, previous_mesh],
            )
        }
    }

    pub fn set_mesh_keyform(&mut self, id: &str, mut form: MeshKeyform) -> EditResult {
        if let Err(status) = self.authoring_unlocked(id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let mut b = match self.get_binding(id) {
            Some(old) => old.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", id)),
        };
        if let Err(s) = super::binding_authoring::identity::coordinates(
            &b.axes,
            &mut form.keys,
            &mut form.samples,
        ) {
            return self.failed(s);
        }
        let it = b.keyforms.iter_mut().find(|f| f.samples == form.samples);
        let changes_order = match it {
            Some(f) => {
                let changes_order = f.draw_order != form.draw_order;
                *f = form;
                changes_order
            }
            None => return self.failed(Status::error("INVALID_KEY_COMBINATION", id)),
        };
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let mesh = b.mesh_id.clone();
        let tracked =
            super::tracking::binding_payload_is_tracked(self.get_binding(id).unwrap(), &b)
                .then(|| self.track_binding_payload_write(id));
        self.content.bindings.insert(id.to_string(), b);
        let kind = if changes_order {
            ChangeKind::Structure
        } else {
            ChangeKind::Positions
        };
        if let Some(write) = tracked {
            self.changed_tracked(write, kind, vec![mesh.clone()], vec![id.to_string(), mesh])
        } else {
            self.changed(kind, vec![mesh.clone()], vec![id.to_string(), mesh])
        }
    }

    pub fn scene_binding_order(&self) -> &[String] {
        &self.content.scene_binding_order
    }

    pub fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.content.scene_bindings.get(id)
    }

    pub fn binding_for_scene(&self, target_id: &str) -> Option<&SceneBinding> {
        self.lookup()
            .scene_bindings
            .get(target_id)
            .and_then(|id| self.content.scene_bindings.get(id))
    }

    pub(super) fn canonicalize_scene_binding(&self, b: &mut SceneBinding) -> Status {
        #[cfg(feature = "bench-instrumentation")]
        let _span = crate::benchmark::span("core.canonicalize_scene_binding");
        if let Err(s) = self.normalize_owner_axes(&b.id, &mut b.axes) {
            return s;
        }
        if let Err(s) = self.check_binding_ids(
            &b.id,
            b.axes.iter().flat_map(|a| {
                std::iter::once(a.id.clone()).chain(a.samples.iter().map(|s| s.id.clone()))
            }),
        ) {
            return s;
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
            super::binding_authoring::identity::coordinates(&b.axes, keys, samples)
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

    pub fn create_scene_binding(&mut self, mut b: SceneBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(b.target_id()) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.canonicalize_scene_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id().to_owned();
        self.content.scene_bindings.insert(id.clone(), b);
        self.content.scene_binding_order.push(id.clone());
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target])
    }

    pub fn replace_scene_binding(&mut self, mut b: SceneBinding) -> EditResult {
        if let Err(status) = self.authoring_unlocked(b.target_id()) {
            return self.failed(status);
        }
        if let Err(status) = self.authoring_unlocked(&b.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        let previous = match self.content.scene_bindings.get(&b.id) {
            Some(old) => old.target_id().to_owned(),
            None => return self.failed(Status::error("MISSING_BINDING", &b.id)),
        };
        let s = self.canonicalize_scene_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id().to_owned();
        self.content.scene_bindings.insert(id.clone(), b);
        let meshes = self.content.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target, previous])
    }

    /// Atomically replace an existing Part binding and its Offscreen appearance.
    /// The caller supplies every new keyform and mapping explicitly; no implicit
    /// interpolation or slot copying takes place. Mapping slots use canonical axis order.
    pub fn replace_part_binding_with_offscreen(
        &mut self,
        binding: SceneBinding,
        offscreen: Offscreen,
    ) -> EditResult {
        if let Err(status) = self.authoring_unlocked(&binding.id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &binding.id));
        }
        let Some(old_binding) = self.get_scene_binding(&binding.id) else {
            return self.failed(Status::error("MISSING_BINDING", &binding.id));
        };
        let Some(old_offscreen) = self.get_offscreen(&offscreen.id) else {
            return self.failed(Status::error("MISSING_OBJECT", &offscreen.id));
        };
        if binding.target_id() != old_binding.target_id()
            || offscreen.part_id != old_offscreen.part_id
            || binding.target_id() != offscreen.part_id
        {
            return self.failed(Status::error(
                "INVALID_BINDING",
                "Preserve the Part and Offscreen owner",
            ));
        }
        let objects = vec![
            binding.id.clone(),
            binding.target_id().to_owned(),
            offscreen.id.clone(),
        ];
        let mut candidate = self.clone();
        candidate
            .content
            .offscreens
            .insert(offscreen.id.clone(), offscreen.clone());
        let edit = candidate.replace_scene_binding(binding);
        if !edit.status.is_ok() {
            return self.failed(edit.status);
        }
        let edit = candidate.replace_offscreen(offscreen);
        if !edit.status.is_ok() {
            return self.failed(edit.status);
        }
        self.content.scene_bindings = candidate.content.scene_bindings;
        self.content.offscreens = candidate.content.offscreens;
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            objects,
        )
    }

    pub fn set_scene_keyform(&mut self, id: &str, mut f: SceneKeyform) -> EditResult {
        if let Err(status) = self.authoring_unlocked(id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", id));
        }
        let mut b = match self.get_scene_binding(id) {
            Some(old) => old.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", id)),
        };
        let (keys, samples) = match &mut f {
            SceneKeyform::Warp(f) => (&mut f.keys, &mut f.samples),
            SceneKeyform::Rotation(f) => (&mut f.keys, &mut f.samples),
            SceneKeyform::Part(f) => (&mut f.keys, &mut f.samples),
        };
        if let Err(s) = super::binding_authoring::identity::coordinates(&b.axes, keys, samples) {
            return self.failed(s);
        }
        let status = b.track.replace(f);
        if !status.is_ok() {
            return self.failed(status);
        }
        let status = self.canonicalize_scene_binding(&mut b);
        if !status.is_ok() {
            return self.failed(status);
        }
        // Replacing an existing pose preserves target, axes and key coordinates.
        // Part keyforms can change render order and still require preparation.
        let kind = if matches!(b.track, SceneTrack::Part { .. }) {
            ChangeKind::Structure
        } else {
            ChangeKind::Positions
        };
        let target = b.target_id().to_owned();
        self.content.scene_bindings.insert(id.to_owned(), b);
        self.changed(
            kind,
            self.content.mesh_order.clone(),
            vec![id.to_owned(), target],
        )
    }
}
