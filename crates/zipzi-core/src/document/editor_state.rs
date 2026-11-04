//! Persistent authoring preferences. Never read by runtime evaluation or export.
use super::*;

pub use zipzi_contract::document::{EditorState, ObjectEditorState};

/// Initial display length in model pixels when descendant bounds cannot supply one.
pub const DEFAULT_ROTATION_BONE_LENGTH: f32 = 128.;

impl Document {
    /// Display baseline only; runtime evaluation never reads this value.
    /// Creation and loading freeze the fallback in EditorState before a later
    /// original-shape capture can change it.
    pub fn rotation_bone_reference_scale(&self, id: &str) -> Option<f32> {
        let transform = self.get_transform(id)?;
        transform.rotation()?;
        if let Some(scale) = self.editor_state().rotation_bone_reference_scales.get(id) {
            return Some(*scale);
        }
        if let Some(crate::DeformerOriginalShape::Rotation { pose }) = transform
            .original_shape
            .as_ref()
            .and_then(|original| original.snapshot.as_ref())
            .map(|snapshot| &snapshot.shape)
        {
            if pose.scale.is_finite() && pose.scale > 0. {
                return Some(pose.scale);
            }
        }
        // Imported rotations may use reciprocal pixel/unit scales. Capture their
        // authored baseline once, rather than treating that conversion as UI size.
        let mut scale = 1_f64;
        let mut current = Some(transform);
        let mut remaining = self.transform_order().len();
        while let Some(transform) = current {
            if remaining == 0 {
                return None;
            }
            remaining -= 1;
            if let Some(rotation) = transform.rotation() {
                scale *= f64::from(rotation.pose.scale);
            }
            current = self.get_transform(transform.parent());
        }
        let scale = scale as f32;
        (scale.is_finite() && scale > 0.).then_some(scale)
    }

    /// Missing display lengths derive from descendant Mesh bounds at the frozen
    /// authoring inputs. Evaluate once for the entire set, including hidden Meshes.
    pub(super) fn initial_rotation_bone_lengths(
        &self,
        ids: &[String],
        state: &EditorState,
    ) -> std::collections::BTreeMap<String, f32> {
        let targets: std::collections::HashSet<_> = ids
            .iter()
            .filter(|id| {
                !state.rotation_bone_lengths.contains_key(*id)
                    && self
                        .get_transform(id)
                        .is_some_and(|t| t.rotation().is_some())
            })
            .map(String::as_str)
            .collect();
        if targets.is_empty() {
            return Default::default();
        }
        // A fallback is still a captured preference. Leaving it absent would
        // resize the bone on reopen after descendants have been added or moved.
        let mut lengths = targets
            .iter()
            .map(|id| ((*id).to_owned(), DEFAULT_ROTATION_BONE_LENGTH))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut frame = crate::DrawableFrame::default();
        if !crate::evaluation::evaluate_frame_including_hidden(
            self,
            &self.original_capture_context,
            &mut frame,
        )
        .is_ok()
        {
            return lengths;
        }
        let mut bounds: HashMap<&str, [f64; 4]> = HashMap::new();
        for drawable in &frame.drawables {
            let Some(mesh) = self.get_mesh(&drawable.id) else {
                continue;
            };
            let mut extent = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for p in &drawable.positions {
                extent[0] = extent[0].min(f64::from(p.x));
                extent[1] = extent[1].min(f64::from(p.y));
                extent[2] = extent[2].max(f64::from(p.x));
                extent[3] = extent[3].max(f64::from(p.y));
            }
            if !extent.iter().all(|v| v.is_finite()) {
                continue;
            }
            let mut parent = mesh.deformer_id.as_str();
            for _ in 0..self.transform_order().len() {
                let Some(transform) = self.get_transform(parent) else {
                    break;
                };
                if targets.contains(parent) {
                    let b = bounds.entry(parent).or_insert(extent);
                    b[0] = b[0].min(extent[0]);
                    b[1] = b[1].min(extent[1]);
                    b[2] = b[2].max(extent[2]);
                    b[3] = b[3].max(extent[3]);
                }
                parent = transform.parent();
            }
        }
        lengths.extend(frame.deformers.iter().filter_map(|control| {
            let b = bounds.get(control.id.as_str())?;
            let [x, _] = control.rotation_basis?;
            let evaluated_scale = f64::from(x.x).hypot(f64::from(x.y));
            let reference = f64::from(self.rotation_bone_reference_scale(&control.id)?);
            let origin = control.points.first()?;
            let axis = control.rotation_axis?;
            let angle = f64::from(
                state
                    .rotation_bone_angle_offsets
                    .get(&control.id)
                    .copied()
                    .unwrap_or(0.),
            )
            .to_radians();
            let (sin, cos) = angle.sin_cos();
            let direction = [
                cos * f64::from(axis.x) - sin * f64::from(axis.y),
                sin * f64::from(axis.x) + cos * f64::from(axis.y),
            ];
            let distance = rotation_bone_ray_length(
                [f64::from(origin.x), f64::from(origin.y)],
                direction,
                *b,
            )?;
            // Store at the display reference scale so presentation does not apply
            // authored/inherited scale twice to this already evaluated distance.
            let length = (distance * reference / evaluated_scale) as f32;
            (length.is_finite() && length > 0.).then(|| (control.id.clone(), length))
        }));
        lengths
    }

    /// Freeze native display preferences after the final creation geometry and
    /// parent relations have been installed, before original-shape updates.
    pub(super) fn initialize_rotation_bone_display(&mut self, ids: &[String]) {
        for id in ids {
            if self
                .get_transform(id)
                .is_some_and(|t| t.rotation().is_some())
                && !self
                    .editor_state()
                    .rotation_bone_reference_scales
                    .contains_key(id)
            {
                let scale = self.rotation_bone_reference_scale(id).unwrap_or(1.);
                self.content
                    .editor_state
                    .rotation_bone_reference_scales
                    .insert(id.clone(), scale);
            }
        }
        let lengths = self.initial_rotation_bone_lengths(ids, self.editor_state());
        self.content
            .editor_state
            .rotation_bone_lengths
            .extend(lengths);
    }

    /// Freeze missing display preferences before publishing an imported or loaded document.
    pub fn initialize_rotation_bone_reference_scales(&mut self) -> EditResult {
        let mut state = self.editor_state().clone();
        for id in self.transform_order() {
            let transform = self.get_transform(id).unwrap();
            if transform.rotation().is_some() && transform.original_shape.is_none() {
                state
                    .rotation_bone_angle_offsets
                    .entry(id.clone())
                    .or_insert(90.);
            }
            if transform.rotation().is_some()
                && !state.rotation_bone_reference_scales.contains_key(id)
            {
                state.rotation_bone_reference_scales.insert(
                    id.clone(),
                    self.rotation_bone_reference_scale(id).unwrap_or(1.),
                );
            }
        }
        let lengths = self.initial_rotation_bone_lengths(self.transform_order(), &state);
        state.rotation_bone_lengths.extend(lengths);
        self.replace_editor_state(state)
    }

    pub fn editor_state(&self) -> &EditorState {
        &self.content.editor_state
    }

    pub fn replace_editor_state(&mut self, mut state: EditorState) -> EditResult {
        #[cfg(feature = "bench-instrumentation")]
        let _span = crate::benchmark::span("core.replace_editor_state");
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Some(issue) = self.editor_state_issues(&state).into_iter().next() {
            return self.failed(issue.status);
        }
        state
            .objects
            .retain(|_, flags| flags.hidden_in_editor || flags.locked);
        if self.content.editor_state == state {
            return self.failed(Status::ok());
        }
        let mut changed: Vec<_> = self
            .content
            .editor_state
            .objects
            .keys()
            .chain(state.objects.keys())
            .chain(self.content.editor_state.rotation_bone_lengths.keys())
            .chain(state.rotation_bone_lengths.keys())
            .chain(
                self.content
                    .editor_state
                    .rotation_bone_reference_scales
                    .keys(),
            )
            .chain(state.rotation_bone_reference_scales.keys())
            .chain(self.content.editor_state.rotation_bone_angle_offsets.keys())
            .chain(state.rotation_bone_angle_offsets.keys())
            .cloned()
            .collect();
        changed.sort();
        changed.dedup();
        // Unchanged bone display preferences are not touched objects. Including
        // them makes every flag toggle fan out through the publication summary.
        let before = &self.content.editor_state;
        changed.retain(|id| {
            before.get(id) != state.get(id)
                || before.rotation_bone_lengths.get(id) != state.rotation_bone_lengths.get(id)
                || before.rotation_bone_reference_scales.get(id)
                    != state.rotation_bone_reference_scales.get(id)
                || before.rotation_bone_angle_offsets.get(id)
                    != state.rotation_bone_angle_offsets.get(id)
        });
        self.content.editor_state = state;
        self.changed_editor_state(changed)
    }

    pub(super) fn editor_state_issues(&self, state: &EditorState) -> Vec<StructureIssue> {
        state
            .objects
            .keys()
            .filter_map(|id| {
                // Resolve top-level identities before building the global
                // reference/binding indexes for owned metadata identities.
                let valid = self.content.assets.contains_key(id)
                    || self.content.meshes.contains_key(id)
                    || self.content.transforms.contains_key(id)
                    || self.content.parts.contains_key(id)
                    || self.content.offscreens.contains_key(id)
                    || self.content.glues.contains_key(id)
                    || self.content.parameters.contains_key(id)
                    || self.get_parameter_group(id).is_some()
                    || self.content.bindings.contains_key(id)
                    || self.content.scene_bindings.contains_key(id)
                    || self.content.blend_key_tables.contains_key(id)
                    || self.content.blend_constraints.contains_key(id)
                    || self.content.blend_bindings.contains_key(id)
                    || self.content.expressions.contains_key(id)
                    || self.get_expression_entry(id).is_some()
                    || self.motion_identity_exists(id)
                    || self.get_attachment(id).is_some()
                    || self.physics_owner(id).is_some()
                    || self.physics().is_some_and(|a| a.id == *id)
                    || self.pose().is_some_and(|a| a.entity_ids().any(|v| v == id))
                    || self.reference_identity(id).is_some()
                    || self.binding_identity(id).is_some()
                    || self.get_glue_pair(id).is_some();
                (!valid).then(|| StructureIssue {
                    object_id: id.clone(),
                    status: Status::error(
                        "INVALID_EDITOR_STATE",
                        "Editor state references a missing or incompatible object",
                    ),
                })
            })
            .chain(
                state
                    .rotation_bone_lengths
                    .iter()
                    .chain(state.rotation_bone_reference_scales.iter())
                    .map(|(id, value)| (id, value, true))
                    .chain(state.rotation_bone_angle_offsets.iter().map(|(id, value)| (id, value, false)))
                    .filter_map(|(id, length, positive)| {
                        (!length.is_finite()
                            || (positive && *length <= 0.)
                            || self
                                .get_transform(id)
                                .is_none_or(|t| t.rotation().is_none()))
                        .then(|| StructureIssue {
                            object_id: id.clone(),
                            status: Status::error(
                                "INVALID_EDITOR_STATE",
                                "Bone display values must be finite, lengths and reference scales positive, and reference a Rotation",
                            ),
                        })
                    }),
            )
            .collect()
    }
}

/// Exit the descendant bounds along the displayed forward axis. Keep the tip
/// 15% inside the forward segment so it does not sit directly on the Mesh edge.
fn rotation_bone_ray_length(origin: [f64; 2], axis: [f64; 2], bounds: [f64; 4]) -> Option<f64> {
    let magnitude = axis[0].hypot(axis[1]);
    if !origin
        .iter()
        .chain(axis.iter())
        .chain(bounds.iter())
        .all(|v| v.is_finite())
        || magnitude <= 0.
    {
        return None;
    }
    let mut enter = 0_f64;
    let mut exit = f64::INFINITY;
    for i in 0..2 {
        let direction = axis[i] / magnitude;
        if bounds[i] > bounds[i + 2] {
            return None;
        }
        if direction.abs() < 1e-12 {
            if origin[i] < bounds[i] || origin[i] > bounds[i + 2] {
                return None;
            }
        } else {
            let a = (bounds[i] - origin[i]) / direction;
            let b = (bounds[i + 2] - origin[i]) / direction;
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    if !exit.is_finite() || exit <= enter {
        return None;
    }
    Some(exit - (exit - enter) * 0.15)
}

#[cfg(test)]
mod bone_length_tests {
    use super::rotation_bone_ray_length as length;
    #[test]
    fn ray_length_uses_direction_and_off_center_origin() {
        let bounds = [-100., -20., 100., 20.];
        for (origin, direction, expected) in [
            ([0., 0.], [1., 0.], 85.),
            ([0., 0.], [0., 1.], 17.),
            ([80., 10.], [1., 0.], 17.),
            ([80., 10.], [-1., 0.], 153.),
            ([80., 10.], [0., -1.], 25.5),
            ([0., 0.], [1., 1.], 17. * 2_f64.sqrt()),
            ([200., 0.], [-1., 0.], 270.),
        ] {
            assert!((length(origin, direction, bounds).unwrap() - expected).abs() < 1e-9);
        }
    }
    #[test]
    fn ray_length_rejects_misses_and_invalid_or_degenerate_inputs() {
        let bounds = [-100., -20., 100., 20.];
        assert_eq!(length([200., 0.], [1., 0.], bounds), None);
        assert_eq!(length([200., 0.], [0., 1.], bounds), None);
        assert_eq!(length([0., 0.], [0., 0.], bounds), None);
        assert_eq!(length([f64::NAN, 0.], [1., 0.], bounds), None);
        assert_eq!(length([0., 0.], [1., 0.], [0.; 4]), None);
    }
}
