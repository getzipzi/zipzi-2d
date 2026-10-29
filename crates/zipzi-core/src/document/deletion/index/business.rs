//! Business ownership and typed uses share the same frozen scene relation index.
use super::*;
impl DeletionIndex {
    pub(super) fn business(
        &mut self,
        d: &Document,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        for id in d.parameter_order() {
            self.objects.insert(object(DeletionKind::Parameter, id));
        }
        for id in d.asset_order() {
            self.objects.insert(object(DeletionKind::Texture, id));
        }
        for a in d.package_attachments() {
            self.objects.insert(object(DeletionKind::Attachment, &a.id));
        }
        for g in d.display_info().parameter_groups.iter().flatten() {
            control.checkpoint()?;
            let key = object(DeletionKind::ParameterFolder, &g.id);
            self.objects.insert(key.clone());
            if let Some(parent) = &g.parent_id {
                self.folders
                    .entry(parent.clone())
                    .or_default()
                    .push(key.clone());
                self.edge(
                    key,
                    DeletionField::ParameterFolder,
                    object(DeletionKind::ParameterFolder, parent),
                );
            }
        }
        for (slot, e) in d.display_info().parameters.iter().flatten().enumerate() {
            control.checkpoint()?;
            if let CdiParameterEntry::Resolved {
                parameter_id,
                group_id,
                ..
            } = e
            {
                let key = object(DeletionKind::Parameter, parameter_id);
                self.edge(
                    object(DeletionKind::DisplayInfo, d.id()),
                    DeletionField::CdiParameter { slot },
                    key.clone(),
                );
                if let Some(group) = group_id {
                    self.folders
                        .entry(group.clone())
                        .or_default()
                        .push(key.clone());
                    self.edge(
                        key,
                        DeletionField::ParameterFolder,
                        object(DeletionKind::ParameterFolder, group),
                    );
                }
            } else if let Some(group) = e.group_id() {
                self.edge(
                    object(DeletionKind::DisplayInfo, d.id()),
                    DeletionField::CdiParameter { slot },
                    object(DeletionKind::ParameterFolder, group),
                );
            }
        }
        for id in d.mesh_order() {
            control.checkpoint()?;
            if let Some(asset) = d.get_mesh(id).unwrap().texture_asset_id.resolved_id() {
                self.edge(
                    object(DeletionKind::Mesh, id),
                    DeletionField::Texture,
                    object(DeletionKind::Texture, asset),
                );
            }
        }
        for id in d.blend_key_table_order() {
            control.checkpoint()?;
            let table = d.get_blend_key_table(id).unwrap();
            let key = object(DeletionKind::BlendKeyTable, id);
            self.edge(
                key.clone(),
                DeletionField::BlendTableParameter,
                object(DeletionKind::Parameter, &table.parameter_id),
            );
            for sample in &table.samples {
                self.own(key.clone(), object(DeletionKind::Sample, &sample.id));
            }
        }
        for id in d.blend_constraint_order() {
            control.checkpoint()?;
            let c = d.get_blend_constraint(id).unwrap();
            let key = object(DeletionKind::BlendConstraint, id);
            self.edge(
                key.clone(),
                DeletionField::BlendConstraintParameter,
                object(DeletionKind::Parameter, &c.parameter_id),
            );
            for sample in &c.samples {
                self.own(key.clone(), object(DeletionKind::Sample, &sample.id));
            }
        }
        for id in d.blend_binding_order() {
            control.checkpoint()?;
            let b = d.get_blend_binding(id).unwrap();
            let key = object(DeletionKind::BlendBinding, id);
            self.edge(
                key.clone(),
                DeletionField::BlendTable,
                object(DeletionKind::BlendKeyTable, &b.key_table_id),
            );
            for (slot, id) in b.constraint_ids.iter().enumerate() {
                self.edge(
                    key.clone(),
                    DeletionField::BlendConstraint { slot },
                    object(DeletionKind::BlendConstraint, id),
                );
            }
        }
        for id in d.motion_order() {
            control.checkpoint()?;
            let clip = d.get_motion(id).unwrap();
            let key = object(DeletionKind::Motion, id);
            self.objects.insert(key.clone());
            for track in &clip.tracks {
                self.own(key.clone(), object(DeletionKind::MotionTrack, &track.id));
                self.track(track, None, control)?;
            }
            for event in &clip.events {
                self.own(key.clone(), object(DeletionKind::MotionEvent, &event.id));
            }
        }
        for group in d.motion_groups() {
            let key = object(DeletionKind::MotionGroup, &group.id);
            self.objects.insert(key.clone());
            for entry in &group.entries {
                self.own(
                    key.clone(),
                    object(DeletionKind::MotionRegistration, &entry.id),
                );
            }
        }
        for entry in d.motion_registrations() {
            control.checkpoint()?;
            let key = object(DeletionKind::MotionRegistration, &entry.id);
            self.edge(
                key.clone(),
                DeletionField::MotionClip,
                object(DeletionKind::Motion, &entry.clip_id),
            );
            if let Some(AttachmentRef::Resolved { attachment_id }) = &entry.sound {
                self.edge(
                    key,
                    DeletionField::AttachmentSound,
                    object(DeletionKind::Attachment, attachment_id),
                );
            }
        }
        if let Some(AttachmentRef::Resolved { attachment_id }) = &d.model3_settings().user_data {
            self.edge(
                object(DeletionKind::ModelSettings, d.id()),
                DeletionField::AttachmentUserData,
                object(DeletionKind::Attachment, attachment_id),
            );
        }
        for id in d.expression_order() {
            control.checkpoint()?;
            let expression = d.get_expression(id).unwrap();
            let key = object(DeletionKind::Expression, id);
            self.objects.insert(key.clone());
            for entry in &expression.entries {
                let child = object(DeletionKind::ExpressionEntry, &entry.id);
                self.own(key.clone(), child.clone());
                if let ExpressionTarget::Resolved { parameter_id } = &entry.target {
                    self.edge(
                        child,
                        DeletionField::ExpressionParameter,
                        object(DeletionKind::Parameter, parameter_id),
                    );
                }
            }
            if let Some(scene) = expression.source_scene() {
                for track in &scene.tracks {
                    self.own(
                        key.clone(),
                        DeletionObject {
                            kind: DeletionKind::ExpressionTrack,
                            id: track.id.clone(),
                            owner_id: Some(id.clone()),
                        },
                    );
                    self.track(track, Some(id), control)?;
                }
                for event in &scene.events {
                    self.own(
                        key.clone(),
                        DeletionObject {
                            kind: DeletionKind::MotionEvent,
                            id: event.id.clone(),
                            owner_id: Some(id.clone()),
                        },
                    );
                }
            }
        }
        if let Some(asset) = d.physics() {
            let key = object(DeletionKind::Physics, &asset.id);
            self.objects.insert(key.clone());
            for group in &asset.data.settings {
                control.checkpoint()?;
                let owner = object(DeletionKind::PhysicsGroup, &group.id);
                self.own(key.clone(), owner.clone());
                for input in &group.inputs {
                    let child = object(DeletionKind::PhysicsInput, &input.id);
                    self.own(owner.clone(), child.clone());
                    if let PhysicsTarget::Resolved { parameter_id } = &input.source {
                        self.edge(
                            child,
                            DeletionField::PhysicsParameter,
                            object(DeletionKind::Parameter, parameter_id),
                        );
                    }
                }
                for output in &group.outputs {
                    let child = object(DeletionKind::PhysicsOutput, &output.id);
                    self.own(owner.clone(), child.clone());
                    if let PhysicsTarget::Resolved { parameter_id } = &output.destination {
                        self.edge(
                            child.clone(),
                            DeletionField::PhysicsParameter,
                            object(DeletionKind::Parameter, parameter_id),
                        );
                    }
                    self.edge(
                        child,
                        DeletionField::PhysicsNode,
                        object(DeletionKind::PhysicsNode, &output.node_id),
                    );
                }
                for node in &group.vertices {
                    self.own(owner.clone(), object(DeletionKind::PhysicsNode, &node.id));
                }
            }
        }
        for group in d.model3_settings().groups.iter().flatten() {
            control.checkpoint()?;
            let owner = object(DeletionKind::ModelParameterGroup, &group.id);
            self.objects.insert(owner.clone());
            for member in &group.parameters {
                let child = object(DeletionKind::ModelParameterMember, &member.id);
                self.own(owner.clone(), child.clone());
                if let Some(parameter) = member.target.object_id() {
                    self.edge(
                        child,
                        DeletionField::ModelParameter,
                        object(DeletionKind::Parameter, parameter),
                    );
                }
            }
        }
        for set in d.display_info().combined_parameters.iter().flatten() {
            control.checkpoint()?;
            let owner = object(DeletionKind::CdiCombinedSet, &set.id);
            self.objects.insert(owner.clone());
            for member in &set.members {
                let child = object(DeletionKind::CdiCombinedMember, &member.id);
                self.own(owner.clone(), child.clone());
                if let CdiParameterRef::Resolved { parameter_id } = &member.target {
                    self.edge(
                        child,
                        DeletionField::CdiCombinedParameter,
                        object(DeletionKind::Parameter, parameter_id),
                    );
                }
            }
        }
        Ok(())
    }
}
