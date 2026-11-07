use super::*;
impl Model<'_> {
    pub fn references_to(&self, id: &str) -> Vec<String> {
        let mut refs = Vec::new();
        if let Some(owner) = self.reference_identity(id).and_then(|i| i.owner.as_ref()) {
            refs.push(owner.clone());
        }
        if let Some(owner) = self.glue_pair_owner(id) {
            refs.push(owner.into());
        }
        if let Some(child) = self.binding_identity(id) {
            refs.push(child.owner.clone());
            if self.get_blend_key_table(&child.owner).is_some() {
                refs.extend(
                    self.content
                        .blend_bindings
                        .values()
                        .filter(|b| b.key_table_id == child.owner)
                        .map(|b| b.id.clone()),
                );
            }
        }
        for m in self.content.mesh_order {
            if self.content.meshes.get(m).unwrap().texture_asset_id == id {
                refs.push(m.clone());
            }
        }
        for b in self.content.binding_order {
            let binding = self.content.bindings.get(b).unwrap();
            let mut refers = binding.mesh_id == id;
            for axis in &binding.axes {
                refers |= axis.parameter_id == id;
            }
            if refers {
                refs.push(b.clone());
            }
        }
        for (key, t) in &self.content.transforms {
            if t.parent() == id || t.part() == id {
                refs.push(key.clone());
            }
        }
        for (key, p) in &self.content.parts {
            if p.parent_id == id {
                refs.push(key.clone());
            }
        }
        for (key, m) in &self.content.meshes {
            if m.part_id == id || m.deformer_id == id || m.masks.iter().any(|mask| mask == id) {
                refs.push(key.clone());
            }
        }
        for (key, b) in &self.content.scene_bindings {
            let mut refers = b.target_id() == id;
            for a in &b.axes {
                refers |= a.parameter_id == id;
            }
            if refers {
                refs.push(key.clone());
            }
        }
        if let Some(groups) = &self.content.draw_order_groups {
            for group in groups {
                if group.owner == id {
                    refs.extend(group.items.iter().cloned());
                }
            }
        }
        for (key, kt) in &self.content.blend_key_tables {
            if kt.parameter_id == id {
                refs.push(key.clone());
            }
        }
        for (key, c) in &self.content.blend_constraints {
            if c.parameter_id == id {
                refs.push(key.clone());
            }
        }
        for (key, b) in &self.content.blend_bindings {
            if b.target_id == id
                || b.key_table_id == id
                || b.constraint_ids.iter().any(|cid| cid == id)
            {
                refs.push(key.clone());
            }
        }
        for (key, g) in &self.content.glues {
            if g.mesh_a_id == id
                || g.mesh_b_id == id
                || g.binding
                    .as_ref()
                    .is_some_and(|b| b.axes.iter().any(|a| a.parameter_id == id))
            {
                refs.push(key.clone());
            }
        }
        for (key, os) in &self.content.offscreens {
            if os.part_id == id
                || os.masks.iter().any(|mask| mask == id)
                || (!os.part_keyform_indices.is_empty()
                    && self
                        .get_scene_binding(id)
                        .is_some_and(|b| b.target_id() == os.part_id))
            {
                refs.push(key.clone());
            }
        }
        if let Some(groups) = &self.content.display_info.parameter_groups {
            for group in groups {
                if group.parent_id.as_deref() == Some(id) {
                    refs.push(group.id.clone());
                }
            }
        }
        if let Some(entries) = &self.content.display_info.parameters {
            for (index, entry) in entries.iter().enumerate() {
                if entry.group_id() == Some(id)
                    || matches!(entry, CdiParameterEntry::Resolved { parameter_id, .. } if parameter_id == id)
                {
                    refs.push(format!("display_info.Parameters[{index}]"));
                }
            }
        }
        if let Some(entries) = &self.content.display_info.parts {
            for (index, entry) in entries.iter().enumerate() {
                if matches!(entry, CdiPartEntry::Resolved { part_id, .. } if part_id == id) {
                    refs.push(format!("display_info.Parts[{index}]"));
                }
            }
        }
        if let Some(sets) = &self.content.display_info.combined_parameters {
            for set in sets {
                for member in &set.members {
                    if matches!(&member.target,CdiParameterRef::Resolved{parameter_id} if parameter_id==id)
                    {
                        refs.push(member.id.clone());
                    }
                }
            }
        }
        for expression in self.content.expressions.values() {
            for entry in &expression.entries {
                if matches!(&entry.target, ExpressionTarget::Resolved { parameter_id } if parameter_id == id)
                {
                    refs.push(entry.id.clone());
                }
            }
            if let Some(scene) = expression.source_scene() {
                for (index, track) in scene.tracks.iter().enumerate() {
                    if matches!(&track.target, MotionTrackTarget::Parameter { parameter_id } if parameter_id == id)
                        || matches!(&track.target, MotionTrackTarget::PartOpacity { part_id } if part_id == id)
                    {
                        refs.push(format!(
                            "expression {} source scene track {index}",
                            expression.id
                        ));
                    }
                }
            }
        }
        for motion in self.content.motions.values() {
            for (index, track) in motion.tracks.iter().enumerate() {
                if matches!(&track.target, MotionTrackTarget::Parameter { parameter_id } if parameter_id == id)
                    || matches!(&track.target, MotionTrackTarget::PartOpacity { part_id } if part_id == id)
                {
                    refs.push(format!("motion {} track {index}", motion.id));
                }
            }
        }
        for entry in self.motion_registrations() {
            if matches!(&entry.sound,Some(AttachmentRef::Resolved{attachment_id}) if attachment_id==id)
            {
                refs.push(format!("motion registration {} sound", entry.id));
            }
        }
        if matches!(&self.model3_settings().user_data,Some(AttachmentRef::Resolved{attachment_id}) if attachment_id==id)
        {
            refs.push("model3 UserData".into());
        }
        for group in self.content.motion_groups {
            if group.entries.iter().any(|entry| entry.clip_id == id) {
                refs.push(format!("motion group {}", group.name));
            }
        }
        if let Some(pose) = &self.content.pose {
            for group in &pose.groups {
                for entry in &group.entries {
                    if matches!(&entry.part,PosePartRef::Resolved { part_id } if part_id==id) {
                        refs.push(format!("pose {} entry {} Part", pose.id, entry.id));
                    }
                    for link in entry.links.iter().flatten() {
                        if matches!(&link.target,PosePartRef::Resolved { part_id } if part_id==id) {
                            refs.push(format!("pose {} link {} Part", pose.id, link.id));
                        }
                    }
                }
            }
        }
        if let Some(physics) = &self.content.physics {
            for output in self.physics_node_users(id) {
                refs.push(format!("physics {} output {output}", physics.id));
            }
            for g in &physics.data.settings {
                for (entry, target) in g
                    .inputs
                    .iter()
                    .map(|v| (&v.id, &v.source))
                    .chain(g.outputs.iter().map(|v| (&v.id, &v.destination)))
                {
                    if target.parameter_id() == Some(id) {
                        refs.push(format!("physics {} entry {entry}", physics.id));
                    }
                }
            }
        }
        for group in self.model3_settings().groups.iter().flatten() {
            for member in &group.parameters {
                if member.target.object_id() == Some(id) {
                    refs.push(member.id.clone());
                }
            }
        }
        for area in self.model3_settings().hit_areas.iter().flatten() {
            if area.mesh.object_id() == Some(id) {
                refs.push(area.id.clone());
            }
        }
        refs.sort();
        refs.dedup();
        refs
    }

    fn physics_node_users(&self, id: &str) -> Vec<String> {
        self.physics()
            .into_iter()
            .flat_map(|a| &a.data.settings)
            .flat_map(|g| &g.outputs)
            .filter(|v| v.node_id == id)
            .map(|v| v.id.clone())
            .collect()
    }
}
