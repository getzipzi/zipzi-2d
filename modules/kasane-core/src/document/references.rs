use super::*;
use crate::types::{ChangeKind, EditResult, Status};

impl Document {
    pub fn references_to(&self, id: &str) -> Vec<String> {
        let mut refs = Vec::new();
        for m in &self.content.mesh_order {
            if self.content.meshes[m].texture_asset_id == id {
                refs.push(m.clone());
            }
        }
        for b in &self.content.binding_order {
            let binding = &self.content.bindings[b];
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
                if set.members.iter().any(|member| matches!(member, CdiParameterRef::Resolved { parameter_id } if parameter_id == id)) {
                    refs.push(set.id.clone());
                }
            }
        }
        for expression in self.content.expressions.values() {
            for (index, entry) in expression.entries.iter().enumerate() {
                if matches!(&entry.target, ExpressionTarget::Resolved { parameter_id } if parameter_id == id)
                {
                    refs.push(format!("expression {} entry {index}", expression.id));
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
        for group in &self.content.motion_groups {
            if group.entries.iter().any(|entry| entry.clip_id == id) {
                refs.push(format!("motion group {}", group.name));
            }
        }
        if let Some(pose) = &self.content.pose {
            for (group_index, group) in pose.groups.iter().enumerate() {
                for (entry_index, entry) in group.iter().enumerate() {
                    if matches!(&entry.part, PosePartRef::Resolved { part_id } if part_id == id)
                        || entry.links.as_ref().is_some_and(|links| links.iter().any(|target| matches!(target, PosePartRef::Resolved { part_id } if part_id == id))) {
                        refs.push(format!("pose {} group {group_index} entry {entry_index}", pose.id));
                    }
                }
            }
        }
        if let Some(physics) = &self.content.physics {
            for (runtime_id, parameter_id) in &physics.parameter_bindings {
                if parameter_id == id {
                    refs.push(format!("physics {} parameter {runtime_id}", physics.id));
                }
            }
        }
        if self.content.model3_settings.references_to(id) {
            refs.push("model3 settings".into());
        }
        refs.sort();
        refs.dedup();
        refs
    }

    pub fn erase_object(&mut self, id: &str) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if id == self.content.id || !self.contains_id(id) {
            return self.failed(Status::error("MISSING_OBJECT", id));
        }
        let metadata_only = self.content.expressions.contains_key(id)
            || self.content.motions.contains_key(id)
            || self.content.pose.as_ref().is_some_and(|pose| pose.id == id)
            || self
                .content
                .physics
                .as_ref()
                .is_some_and(|physics| physics.id == id)
            || self
                .content
                .display_info
                .parameter_groups
                .as_ref()
                .is_some_and(|groups| groups.iter().any(|group| group.id == id))
            || self
                .content
                .display_info
                .combined_parameters
                .as_ref()
                .is_some_and(|sets| sets.iter().any(|set| set.id == id));
        let refs = self.references_to(id);
        if !refs.is_empty() {
            let mut e = self.failed(Status::error(
                "OBJECT_REFERENCED",
                format!("{}: explicitly unbind references first", id),
            ));
            e.referrers = refs;
            return e;
        }
        let mut meshes = Vec::new();
        if let Some(b) = self.get_binding(id) {
            meshes.push(b.mesh_id.clone());
        }
        if self.get_mesh(id).is_some() {
            meshes.push(id.to_string());
        }
        if self.get_blend_binding(id).is_some() {
            // Deformers/Parts affect descendants, and Glue can propagate mesh changes.
            meshes = self.content.mesh_order.clone();
        }
        if let Some(g) = self.get_glue(id) {
            meshes.push(g.mesh_a_id.clone());
            meshes.push(g.mesh_b_id.clone());
        }
        if self.get_scene_binding(id).is_some()
            || self.get_transform(id).is_some()
            || self.get_part(id).is_some()
        {
            meshes = self.content.mesh_order.clone();
        }
        if let Some(groups) = &mut self.content.draw_order_groups {
            groups.retain(|group| group.owner != id);
            for group in groups {
                group.items.retain(|item| item != id);
            }
        }
        self.content.editor_state.objects.remove(id);
        self.content
            .hierarchy_order
            .organization
            .retain(|item| item != id);
        self.content
            .hierarchy_order
            .deformation
            .retain(|item| item != id);
        self.content.transforms.remove(id);
        self.content.parts.remove(id);
        self.content.scene_bindings.remove(id);
        self.content.transform_order.retain(|k| k != id);
        self.content.part_order.retain(|k| k != id);
        self.content.scene_binding_order.retain(|k| k != id);
        self.content.assets.remove(id);
        if let Some(mesh) = self.content.meshes.remove(id) {
            self.mesh_runtime_ids.remove(&mesh.runtime_id);
        }
        self.vertex_slots.remove(id);
        self.content.parameters.remove(id);
        self.content.bindings.remove(id);
        self.content.asset_order.retain(|k| k != id);
        self.content.mesh_order.retain(|k| k != id);
        self.content.parameter_order.retain(|k| k != id);
        self.content.binding_order.retain(|k| k != id);
        self.content.blend_key_tables.remove(id);
        self.content.blend_key_table_order.retain(|k| k != id);
        self.content.blend_constraints.remove(id);
        self.content.blend_constraint_order.retain(|k| k != id);
        self.content.blend_bindings.remove(id);
        self.content.blend_binding_order.retain(|k| k != id);
        self.content.glues.remove(id);
        self.content.glue_order.retain(|k| k != id);
        self.content.offscreens.remove(id);
        self.content.offscreen_order.retain(|k| k != id);
        self.content.expressions.remove(id);
        self.content.expression_order.retain(|key| key != id);
        self.content.motions.remove(id);
        self.content.motion_order.retain(|key| key != id);
        if self.content.pose.as_ref().is_some_and(|pose| pose.id == id) {
            self.content.pose = None;
        }
        if self
            .content
            .physics
            .as_ref()
            .is_some_and(|physics| physics.id == id)
        {
            self.content.physics = None;
        }
        if let Some(groups) = &mut self.content.display_info.parameter_groups {
            groups.retain(|group| group.id != id);
        }
        if let Some(sets) = &mut self.content.display_info.combined_parameters {
            sets.retain(|set| set.id != id);
        }

        meshes.sort();
        meshes.dedup();
        self.changed(
            if metadata_only {
                ChangeKind::Metadata
            } else {
                ChangeKind::Structure
            },
            meshes,
            vec![id.to_string()],
        )
    }
}
