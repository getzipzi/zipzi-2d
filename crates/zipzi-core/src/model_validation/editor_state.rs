use super::*;
impl Model<'_> {
    pub(crate) fn editor_state_issues(&self, state: &EditorState) -> Vec<StructureIssue> {
        let mut breath_targets = std::collections::HashSet::new();
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
            .chain(state.cursor_preview_bindings.iter().flatten().filter_map(|(id, binding)| {
                (!self.content.parameters.contains_key(id)
                    || !(0. ..=100.).contains(&binding.influence_percent))
                    .then(|| StructureIssue {
                        object_id: id.clone(),
                        status: Status::error(
                            "INVALID_EDITOR_STATE",
                            "Cursor preview requires an existing parameter and an influence from 0 to 100",
                        ),
                    })
            }))
            .chain(state.breath_preview_parameters.iter().flatten().filter_map(|id| {
                (!self.content.parameters.contains_key(id) || !breath_targets.insert(id))
                    .then(|| StructureIssue {
                        object_id: id.clone(),
                        status: Status::error(
                            "INVALID_EDITOR_STATE",
                            "Breath preview requires unique targets that reference existing parameters",
                        ),
                    })
            }))
            .collect()
    }
}
