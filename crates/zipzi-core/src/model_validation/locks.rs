use super::*;
impl Model<'_> {
    /// Locks inherit through organizational owners. Deformation parents protect
    /// their own fields; reparent/conversion operations check affected parents explicitly.
    pub(crate) fn authoring_unlocked(&self, id: &str) -> Result<(), Status> {
        if !self.editor_state().objects.values().any(|flags| flags.locked) {
            return Ok(());
        }
        let mut pending = vec![id];
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = pending.pop() {
            if id.is_empty() || !seen.insert(id) {
                continue;
            }
            if self.editor_state().get(id).locked {
                return Err(Status::error("OBJECT_LOCKED", id));
            }
            if let Some(owner) = self.reference_identity(id).and_then(|i| i.owner.as_deref()) {
                pending.push(owner);
            }
            if let Some(owner) = self.glue_pair_owner(id) {
                pending.push(owner);
            }
            if let Some(glue) = self.get_glue(id) {
                for pair in &glue.pairs {
                    if self.editor_state().get(&pair.id).locked {
                        return Err(Status::error("OBJECT_LOCKED", &pair.id));
                    }
                }
            }
            // Whole-owner replacements and shared conversions must honor locks
            // on their owned axes and samples as well as the resource itself.
            let axes = self
                .get_binding(id)
                .map(|b| b.axes.as_slice())
                .or_else(|| self.get_scene_binding(id).map(|b| b.axes.as_slice()))
                .or_else(|| {
                    self.get_glue(id)
                        .and_then(|g| g.binding.as_ref())
                        .map(|b| b.axes.as_slice())
                });
            if let Some(axes) = axes {
                for axis in axes {
                    for child in std::iter::once(axis.id.as_str())
                        .chain(axis.samples.iter().map(|s| s.id.as_str()))
                    {
                        if self.editor_state().get(child).locked {
                            return Err(Status::error("OBJECT_LOCKED", child));
                        }
                    }
                }
            }
            let samples = self
                .get_blend_key_table(id)
                .map(|t| t.samples.iter().map(|s| s.id.as_str()).collect::<Vec<_>>())
                .or_else(|| {
                    self.get_blend_constraint(id)
                        .map(|c| c.samples.iter().map(|s| s.id.as_str()).collect())
                });
            for child in samples.into_iter().flatten() {
                if self.editor_state().get(child).locked {
                    return Err(Status::error("OBJECT_LOCKED", child));
                }
            }
            if let Some(child) = self.binding_identity(id) {
                return self.authoring_unlocked(&child.owner);
            } else if let Some(m) = self.get_mesh(id) {
                pending.push(m.part_id.as_str());
            } else if let Some(t) = self.get_transform(id) {
                pending.push(t.part());
            } else if self.get_parameter(id).is_some() {
                if let Some(group) = self.parameter_group_for(id) {
                    pending.push(group);
                }
            } else if let Some(group) = self.get_parameter_group(id) {
                if let Some(parent) = group.parent_id.as_deref() {
                    pending.push(parent);
                }
            } else if let Some(p) = self.get_part(id) {
                pending.push(&p.parent_id);
            } else if let Some(o) = self.get_offscreen(id) {
                pending.push(&o.part_id);
            } else if let Some(b) = self.get_binding(id) {
                pending.push(&b.mesh_id);
            } else if let Some(b) = self.get_scene_binding(id) {
                pending.push(b.target_id());
            } else if let Some(b) = self.get_blend_binding(id) {
                pending.push(&b.target_id);
            } else if self.get_blend_key_table(id).is_some() {
                pending.extend(
                    self.content
                        .blend_bindings
                        .values()
                        .filter(|b| b.key_table_id == id)
                        .map(|b| b.id.as_str()),
                );
            } else if self.get_blend_constraint(id).is_some() {
                pending.extend(
                    self.content
                        .blend_bindings
                        .values()
                        .filter(|b| b.constraint_ids.iter().any(|c| c == id))
                        .map(|b| b.id.as_str()),
                );
            }
        }
        Ok(())
    }
}
