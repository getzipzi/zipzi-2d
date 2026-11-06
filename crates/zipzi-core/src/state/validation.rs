use super::*;
use crate::model_validation::{AssetIds, ImageAvailability, Model, ModelContent, RecordView};
impl Content {
    pub(super) fn model(&self) -> Model<'_> {
        Model::new(ModelContent {
            source_canvas: self.setup.source_canvas,
            id: &self.id,
            image_availability: ImageAvailability::Shared(self.assets.shared_map()),
            assets: AssetIds(self.assets.order()),
            parameter_order: self.parameters.order_vec(),
            part_order: self.parts.order_vec(),
            transform_order: self.transforms.order_vec(),
            offscreen_order: self.offscreens.order_vec(),
            blend_binding_order: self.blend_bindings.order_vec(),
            hierarchy_order: &self.hierarchy_order,
            parameters: RecordView::Shared(self.parameters.shared_map()),
            parts: RecordView::Shared(self.parts.shared_map()),
            transforms: RecordView::Shared(self.transforms.shared_map()),
            meshes: RecordView::Shared(self.meshes.shared_map()),
            mesh_order: self.meshes.order_vec(),
            bindings: RecordView::Shared(self.bindings.shared_map()),
            binding_order: self.bindings.order_vec(),
            scene_bindings: RecordView::Shared(self.scene_bindings.shared_map()),
            scene_binding_order: self.scene_bindings.order_vec(),
            blend_key_tables: RecordView::Shared(self.blend_key_tables.shared_map()),
            blend_key_table_order: self.blend_key_tables.order_vec(),
            blend_constraints: RecordView::Shared(self.blend_constraints.shared_map()),
            blend_constraint_order: self.blend_constraints.order_vec(),
            blend_bindings: RecordView::Shared(self.blend_bindings.shared_map()),
            glues: RecordView::Shared(self.glues.shared_map()),
            glue_order: self.glues.order_vec(),
            offscreens: RecordView::Shared(self.offscreens.shared_map()),
            expression_order: self.expressions.order_vec(),
            draw_order_groups: &self.draw_order_groups,
            expressions: RecordView::Shared(self.expressions.shared_map()),
            motion_order: self.motions.order_vec(),
            motions: RecordView::Shared(self.motions.shared_map()),
            display_info: &self.display_info,
            editor_state: &self.editor_state,
            motion_groups: &self.motion_groups,
            motion_roots: &self.motion_roots,
            pose: &self.pose,
            physics: &self.physics,
            model3_settings: &self.model3_settings,
            package_attachments: &self.package_attachments,
        })
    }
    pub(super) fn validate(&self) -> Result<(), Status> {
        let model = self.model();
        model.validate_attachment_records()?;
        let mut ids = std::collections::HashSet::new();
        let mut admit = |id: &str| {
            if !valid_uuid(id) {
                Err(Status::error("INVALID_ID", id))
            } else if !ids.insert(id.to_owned()) {
                Err(Status::error("DUPLICATE_ID", id))
            } else {
                Ok(())
            }
        };
        admit(&self.id)?;
        for id in self.assets.order() {
            admit(id)?;
            validate_asset(self.assets.get(id).unwrap())?;
        }
        for (id, record) in self.parameters.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.parts.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.transforms.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.meshes.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.bindings.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.scene_bindings.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.blend_key_tables.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.blend_constraints.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.blend_bindings.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.glues.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.offscreens.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.expressions.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }
        for (id, record) in self.motions.shared_map() {
            admit(id)?;
            if id != &record.id {
                return Err(Status::error("INVALID_ID", id));
            }
        }

        for identity in model.binding_identities() {
            admit(&identity.id)?;
        }
        for identity in model.reference_identities() {
            admit(&identity.id)?;
        }
        for group in self.display_info.parameter_groups.iter().flatten() {
            admit(&group.id)?;
        }
        for group in self.motion_groups.iter() {
            admit(&group.id)?;
        }
        for entry in model.motion_registrations() {
            admit(&entry.id)?;
        }
        for expression in self.expressions.shared_map().values() {
            for entry in &expression.entries {
                admit(&entry.id)?;
            }
        }
        for clip in self.motions.shared_map().values() {
            for track in &clip.tracks {
                admit(&track.id)?;
                for key in track.keys() {
                    admit(&key.id)?;
                }
            }
            for event in &clip.events {
                admit(&event.id)?;
            }
        }
        for pose in self.pose.iter() {
            for id in pose.entity_ids() {
                admit(id)?;
            }
        }
        for physics in self.physics.iter() {
            for id in physics.entity_ids() {
                admit(id)?;
            }
        }
        for glue in self.glues.shared_map().values() {
            for pair in &glue.pairs {
                admit(&pair.id)?;
            }
        }
        let mut paths = std::collections::HashSet::new();
        for attachment in self.package_attachments.iter() {
            admit(&attachment.id)?;
            if !valid_attachment_path(&attachment.path) || !paths.insert(&attachment.path) {
                return Err(Status::error("INVALID_ATTACHMENT_PATH", &attachment.path));
            }
        }
        for path in self.missing_attachments.iter() {
            if !valid_attachment_path(path) {
                return Err(Status::error("INVALID_ATTACHMENT_PATH", path));
            }
        }
        for value in self.parts.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_part(value))?;
        }
        for value in self.transforms.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_transform(value))?;
        }
        for value in self.meshes.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_mesh_snapshot(value, false))?;
        }
        for value in self.parameters.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_parameter(value))?;
        }
        for value in self.bindings.shared_map().values().map(Arc::as_ref) {
            checked(model.canonicalize_binding(&mut value.clone()))?;
        }
        for value in self.scene_bindings.shared_map().values().map(Arc::as_ref) {
            checked(model.canonicalize_scene_binding(&mut value.clone()))?;
        }
        for value in self.blend_key_tables.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_blend_key_table(value))?;
        }
        for value in self
            .blend_constraints
            .shared_map()
            .values()
            .map(Arc::as_ref)
        {
            checked(model.validate_blend_constraint(value))?;
        }
        for value in self.blend_bindings.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_blend_binding(value))?;
        }
        for value in self.glues.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_glue(value))?;
        }
        for value in self.offscreens.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_offscreen(value))?;
        }
        for value in self.expressions.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_expression(value))?;
        }
        for value in self.motions.shared_map().values().map(Arc::as_ref) {
            checked(model.validate_motion(value))?;
        }
        let mut runtime = std::collections::HashSet::new();
        for value in self.glues.shared_map().values() {
            if value.runtime_id.is_empty() || !runtime.insert(&value.runtime_id) {
                return Err(Status::error("DUPLICATE_RUNTIME_ID", &value.id));
            }
        }
        let mut runtime = std::collections::HashSet::new();
        for value in self.offscreens.shared_map().values() {
            if value.runtime_id.is_empty() || !runtime.insert(&value.runtime_id) {
                return Err(Status::error("DUPLICATE_RUNTIME_ID", &value.id));
            }
        }

        for issues in [
            model.validate_display_info(),
            model.hierarchy_order_issues(&self.hierarchy_order),
            model.editor_state_issues(&self.editor_state),
        ] {
            if let Some(issue) = issues.into_iter().next() {
                return Err(issue.status);
            }
        }
        checked(model.validate_model3_settings(&self.model3_settings))?;
        checked(model.validate_motion_groups(&self.motion_groups))?;
        if let Some(pose) = self.pose.as_ref() {
            checked(model.validate_pose(pose))?;
        }
        if let Some(physics) = self.physics.as_ref() {
            checked(model.validate_physics(physics, false))?;
        }
        if let Some(groups) = self.draw_order_groups.as_ref() {
            crate::draw_order::validate_groups(&model, groups)?;
        }
        Ok(())
    }
}
fn checked(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}
impl DocumentBuilder {
    pub(super) fn normalize_touched(&mut self) -> Result<(), Status> {
        if let Some(ids) = self.touched.get(&RecordDomain::BlendKeyTable) {
            let mut normalized = Vec::new();
            {
                let model = self.content.model();
                for id in ids {
                    if let Some(old) = self.content.blend_key_tables.get(id) {
                        let mut value = old.clone();
                        model.normalize_table(&mut value)?;
                        if &value != old {
                            normalized.push(value);
                        }
                    }
                }
            }
            for value in normalized {
                self.content
                    .blend_key_tables
                    .insert(value.id.clone(), value);
            }
        }
        if let Some(ids) = self.touched.get(&RecordDomain::BlendConstraint) {
            let mut normalized = Vec::new();
            {
                let model = self.content.model();
                for id in ids {
                    if let Some(old) = self.content.blend_constraints.get(id) {
                        let mut value = old.clone();
                        model.normalize_constraint(&mut value)?;
                        if &value != old {
                            normalized.push(value);
                        }
                    }
                }
            }
            for value in normalized {
                self.content
                    .blend_constraints
                    .insert(value.id.clone(), value);
            }
        }
        if let Some(ids) = self.touched.get(&RecordDomain::Binding) {
            let mut normalized = Vec::new();
            {
                let model = self.content.model();
                for id in ids {
                    if let Some(old) = self.content.bindings.get(id) {
                        let mut value = old.clone();
                        checked(model.canonicalize_binding(&mut value))?;
                        if &value != old {
                            normalized.push(value);
                        }
                    }
                }
            }
            for value in normalized {
                self.content.bindings.insert(value.id.clone(), value);
            }
        }
        if let Some(ids) = self.touched.get(&RecordDomain::SceneBinding) {
            let mut normalized = Vec::new();
            {
                let model = self.content.model();
                for id in ids {
                    if let Some(old) = self.content.scene_bindings.get(id) {
                        let mut value = old.clone();
                        checked(model.canonicalize_scene_binding(&mut value))?;
                        if &value != old {
                            normalized.push(value);
                        }
                    }
                }
            }
            for value in normalized {
                self.content.scene_bindings.insert(value.id.clone(), value);
            }
        }
        if let Some(ids) = self.touched.get(&RecordDomain::Glue) {
            let mut normalized = Vec::new();
            {
                let model = self.content.model();
                for id in ids {
                    if let Some(old) = self.content.glues.get(id) {
                        let mut value = old.clone();
                        if let Some(binding) = &mut value.binding {
                            model.normalize_owner_axes(&value.id, &mut binding.axes)?;
                        }
                        model.normalize_glue_forms(&mut value)?;
                        if &value != old {
                            normalized.push(value);
                        }
                    }
                }
            }
            for value in normalized {
                self.content.glues.insert(value.id.clone(), value);
            }
        }
        if let Some(ids) = self.touched.get(&RecordDomain::BlendBinding) {
            let mut normalized = Vec::new();
            {
                let model = self.content.model();
                for id in ids {
                    if let Some(old) = self.content.blend_bindings.get(id) {
                        let mut value = old.clone();
                        model.normalize_blend_forms(&mut value)?;
                        if &value != old {
                            normalized.push(value);
                        }
                    }
                }
            }
            for value in normalized {
                self.content.blend_bindings.insert(value.id.clone(), value);
            }
        }
        Ok(())
    }
}
