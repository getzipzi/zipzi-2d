use super::*;
use crate::types::{ChangeKind, EditResult, Status};

impl Document {
    pub fn references_to(&self, id: &str) -> Vec<String> {
        let mut refs = Vec::new();
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
            if matches!(&entry.sound,Some(super::AttachmentRef::Resolved{attachment_id}) if attachment_id==id)
            {
                refs.push(format!("motion registration {} sound", entry.id));
            }
        }
        if matches!(&self.model3_settings().user_data,Some(super::AttachmentRef::Resolved{attachment_id}) if attachment_id==id)
        {
            refs.push("model3 UserData".into());
        }
        for group in &self.content.motion_groups {
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
        if self.content.model3_settings.references_to(id) {
            refs.push("model3 settings".into());
        }
        refs.sort();
        refs.dedup();
        refs
    }

    pub fn erase_object(&mut self, id: &str) -> EditResult {
        if let Err(status) = self.authoring_unlocked(id) {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if let Some(child) = self.binding_identity(id) {
            let edit = if child.kind == super::BindingIdentityKind::Axis {
                super::BindingGridEdit::Edit {
                    owner: child.owner,
                    operation: super::GridOperation::RemoveAxis {
                        axis_id: child.id,
                        reduction: None,
                        keep_pending: false,
                    },
                }
            } else if child.axis.is_some() {
                super::BindingGridEdit::Edit {
                    owner: child.owner,
                    operation: super::GridOperation::DeleteSample {
                        sample_id: child.id,
                    },
                }
            } else {
                let plan = if self.get_blend_key_table(&child.owner).is_some() {
                    self.plan_blend_authoring(
                        super::BlendAuthoringEdit::Table {
                            id: child.owner,
                            operation: super::BlendTableOperation::Delete {
                                sample_id: child.id,
                                replacement_base: None,
                            },
                        },
                        super::AuthoringLimits::default(),
                    )
                } else {
                    let c = self.get_blend_constraint(&child.owner).unwrap();
                    self.plan_blend_authoring(
                        super::BlendAuthoringEdit::Constraint {
                            id: c.id.clone(),
                            parameter_id: c.parameter_id.clone(),
                            samples: c
                                .samples
                                .iter()
                                .filter(|s| s.id != child.id)
                                .cloned()
                                .collect(),
                        },
                        super::AuthoringLimits::default(),
                    )
                };
                return match plan {
                    Ok(p) => self.apply_blend_authoring_plan(&p),
                    Err(s) => self.failed(s),
                };
            };
            return match self.plan_binding_grid(edit, super::AuthoringLimits::default()) {
                Ok(p) => self.apply_binding_grid_plan(&p),
                Err(s) => self.failed(s),
            };
        }
        if id == self.content.id || !self.contains_id(id) {
            return self.failed(Status::error("MISSING_OBJECT", id));
        }
        if self.physics_owner(id).is_some() || self.physics().is_some_and(|a| a.id == id) {
            let plan = match self.plan_physics_edit(super::PhysicsEdit::Delete {
                ids: vec![id.into()],
                node_replacements: Default::default(),
            }) {
                Ok(p) => p,
                Err(s) => return self.failed(s),
            };
            return self.apply_physics_plan(&plan);
        }
        let motion_edit = if self.get_motion(id).is_some() {
            Some(super::MotionEdit::DeleteClip { id: id.into() })
        } else if self.get_motion_group(id).is_some() {
            Some(super::MotionEdit::DeleteGroup { id: id.into() })
        } else if self.get_motion_registration(id).is_some() {
            Some(super::MotionEdit::DeleteRegistrations {
                ids: vec![id.into()],
            })
        } else if self.get_motion_track(id).is_some() {
            Some(super::MotionEdit::DeleteTracks {
                ids: vec![id.into()],
            })
        } else if self.get_motion_event(id).is_some() {
            Some(super::MotionEdit::DeleteEvents {
                ids: vec![id.into()],
            })
        } else {
            None
        };
        if let Some(edit) = motion_edit {
            let plan = match self.plan_motion_edit(edit) {
                Ok(p) => p,
                Err(s) => return self.failed(s),
            };
            return self.apply_motion_edit(&plan);
        }
        if self.get_motion_key(id).is_some() {
            return self.failed(Status::error(
                "USE_MOTION_KEY_OPERATION",
                "Delete keys with the curve operation",
            ));
        }
        if self.get_attachment(id).is_some() {
            let plan =
                match self.plan_attachment_edit(super::AttachmentEdit::Delete { id: id.into() }) {
                    Ok(p) => p,
                    Err(s) => return self.failed(s),
                };
            return self.apply_attachment_edit(&plan);
        }
        if self.get_expression_entry(id).is_some() {
            let plan = match self.plan_expression_edit(ExpressionEdit::DeleteEntries {
                ids: vec![id.into()],
            }) {
                Ok(plan) => plan,
                Err(status) => return self.failed(status),
            };
            return self.apply_expression_edit(&plan);
        }
        if let Some(asset) = self.get_expression(id) {
            let plan = match self.plan_expression_edit(ExpressionEdit::Delete {
                id: asset.id.clone(),
            }) {
                Ok(plan) => plan,
                Err(status) => return self.failed(status),
            };
            return self.apply_expression_edit(&plan);
        }
        if self.pose().is_some_and(|p| p.entity_ids().any(|v| v == id)) {
            let plan = match self.plan_pose_edit(PoseEdit::Delete {
                ids: vec![id.into()],
            }) {
                Ok(p) => p,
                Err(s) => return self.failed(s),
            };
            return self.apply_pose_plan(&plan);
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
        self.organization_index.take();
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
            meshes.extend(g.mesh_a_id.resolved_id().map(str::to_owned));
            meshes.extend(g.mesh_b_id.resolved_id().map(str::to_owned));
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
        self.content.hierarchy_order.glues.retain(|item| item != id);
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
        self.content.motion_roots.retain(|e| e.clip_id != id);
        self.content.package_attachments.retain(|a| a.id != id);
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
