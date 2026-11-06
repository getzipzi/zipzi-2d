use super::*;
impl Model<'_> {
    /// Display baseline only; runtime evaluation never reads this value.
    /// Creation and loading freeze the fallback in EditorState before a later
    /// original-shape capture can change it.
    pub fn rotation_bone_reference_scale(&self, id: &str) -> Option<f32> {
        let transform = self.get_transform(id)?;
        transform.rotation()?;
        if let Some(scale) = self.editor_state().rotation_bone_reference_scales.get(id) {
            return Some(*scale);
        }
        self.default_rotation_bone_scales().get(id).copied()
    }

    fn default_rotation_bone_scales(&self) -> std::collections::BTreeMap<String, f32> {
        let mut frame = crate::DrawableFrame::default();
        if !crate::evaluation::evaluate_frame_including_hidden(
            self,
            &Default::default(),
            &mut frame,
        )
        .is_ok()
        {
            return Default::default();
        }
        frame
            .deformers
            .into_iter()
            .filter_map(|control| {
                let [x, _] = control.rotation_basis?;
                let scale = x.x.hypot(x.y);
                (scale.is_finite() && scale > 0.).then_some((control.id, scale))
            })
            .collect()
    }

    pub(crate) fn initial_rotation_bone_scales(
        &self,
        ids: &[String],
        state: &EditorState,
    ) -> std::collections::BTreeMap<String, f32> {
        let missing: Vec<_> = ids
            .iter()
            .filter(|id| {
                !state.rotation_bone_reference_scales.contains_key(*id)
                    && self
                        .get_transform(id)
                        .is_some_and(|t| t.rotation().is_some())
            })
            .collect();
        if missing.is_empty() {
            return Default::default();
        }
        let evaluated = self.default_rotation_bone_scales();
        missing
            .into_iter()
            .map(|id| (id.clone(), evaluated.get(id).copied().unwrap_or(1.)))
            .collect()
    }

    /// Missing display lengths derive from descendant Mesh bounds at the frozen
    /// authoring inputs. Evaluate once for the entire set, including hidden Meshes.
    pub(crate) fn initial_rotation_bone_lengths(
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
            &Default::default(),
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
            let reference = f64::from(*state.rotation_bone_reference_scales.get(&control.id)?);
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
}
pub(crate) fn rotation_bone_ray_length(
    origin: [f64; 2],
    axis: [f64; 2],
    bounds: [f64; 4],
) -> Option<f64> {
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
