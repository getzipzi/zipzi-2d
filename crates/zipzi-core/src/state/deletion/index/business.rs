//! Business ownership and typed uses share the same frozen scene relation index.
use super::*;
use std::collections::HashSet;

#[derive(Default)]
struct CurveScope<'a> {
    motions: HashSet<&'a str>,
    expressions: HashSet<&'a str>,
    parameters: HashSet<&'a str>,
    parts: HashSet<&'a str>,
    tracks: HashSet<(Option<&'a str>, &'a str)>,
    keys: HashSet<(Option<&'a str>, &'a str)>,
}
impl IndexBuilder {
    pub(crate) fn business(
        &mut self,
        d: &Model<'_>,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        for id in d.parameter_order() {
            self.checkpoint(control)?;
            self.record(object(DeletionKind::Parameter, id));
        }
        for id in d.asset_order() {
            self.checkpoint(control)?;
            self.record(object(DeletionKind::Texture, id));
        }
        for a in d.package_attachments() {
            self.checkpoint(control)?;
            self.record(object(DeletionKind::Attachment, &a.id));
        }
        for g in d.display_info().parameter_groups.iter().flatten() {
            self.checkpoint(control)?;
            let key = object(DeletionKind::ParameterFolder, &g.id);
            self.record(key.clone());
            if let Some(parent) = &g.parent_id {
                self.group(|i| &mut i.folders, parent.clone(), key.clone());
                self.edge(
                    key,
                    DeletionField::ParameterFolder,
                    object(DeletionKind::ParameterFolder, parent),
                );
            }
        }
        for (slot, e) in d.display_info().parameters.iter().flatten().enumerate() {
            self.checkpoint(control)?;
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
                    self.group(|i| &mut i.folders, group.clone(), key.clone());
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
            self.checkpoint(control)?;
            if let Some(asset) = d.get_mesh(id).unwrap().texture_asset_id.resolved_id() {
                self.edge(
                    object(DeletionKind::Mesh, id),
                    DeletionField::Texture,
                    object(DeletionKind::Texture, asset),
                );
            }
        }
        for id in d.blend_key_table_order() {
            self.checkpoint(control)?;
            let table = d.get_blend_key_table(id).unwrap();
            let key = object(DeletionKind::BlendKeyTable, id);
            self.edge(
                key.clone(),
                DeletionField::BlendTableParameter,
                object(DeletionKind::Parameter, &table.parameter_id),
            );
            for sample in &table.samples {
                self.checkpoint(control)?;
                self.own(key.clone(), object(DeletionKind::Sample, &sample.id));
            }
        }
        for id in d.blend_constraint_order() {
            self.checkpoint(control)?;
            let c = d.get_blend_constraint(id).unwrap();
            let key = object(DeletionKind::BlendConstraint, id);
            self.edge(
                key.clone(),
                DeletionField::BlendConstraintParameter,
                object(DeletionKind::Parameter, &c.parameter_id),
            );
            for sample in &c.samples {
                self.checkpoint(control)?;
                self.own(key.clone(), object(DeletionKind::Sample, &sample.id));
            }
        }
        for id in d.blend_binding_order() {
            self.checkpoint(control)?;
            let b = d.get_blend_binding(id).unwrap();
            let key = object(DeletionKind::BlendBinding, id);
            // An unconfigured binding has no table relationship. Registering
            // the empty sentinel would make it a selectable deletion root and
            // incorrectly retire every unconfigured consumer as a dependency.
            if !b.key_table_id.is_empty() {
                self.edge(
                    key.clone(),
                    DeletionField::BlendTable,
                    object(DeletionKind::BlendKeyTable, &b.key_table_id),
                );
            }
            for (slot, id) in b.constraint_ids.iter().enumerate() {
                self.checkpoint(control)?;
                self.edge(
                    key.clone(),
                    DeletionField::BlendConstraint { slot },
                    object(DeletionKind::BlendConstraint, id),
                );
            }
        }
        for id in d.motion_order() {
            self.checkpoint(control)?;
            let clip = d.get_motion(id).unwrap();
            let key = object(DeletionKind::Motion, id);
            self.record(key.clone());
            for event in &clip.events {
                self.checkpoint(control)?;
                self.own(key.clone(), object(DeletionKind::MotionEvent, &event.id));
            }
        }
        for group in d.motion_groups() {
            self.checkpoint(control)?;
            let key = object(DeletionKind::MotionGroup, &group.id);
            self.record(key.clone());
            for entry in &group.entries {
                self.checkpoint(control)?;
                self.own(
                    key.clone(),
                    object(DeletionKind::MotionRegistration, &entry.id),
                );
            }
        }
        for entry in d.motion_registrations() {
            self.checkpoint(control)?;
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
            self.checkpoint(control)?;
            let expression = d.get_expression(id).unwrap();
            let key = object(DeletionKind::Expression, id);
            self.record(key.clone());
            for entry in &expression.entries {
                self.checkpoint(control)?;
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
                for event in &scene.events {
                    self.checkpoint(control)?;
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
            self.record(key.clone());
            for group in &asset.data.settings {
                self.checkpoint(control)?;
                let owner = object(DeletionKind::PhysicsGroup, &group.id);
                self.own(key.clone(), owner.clone());
                for input in &group.inputs {
                    self.checkpoint(control)?;
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
                    self.checkpoint(control)?;
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
                    self.checkpoint(control)?;
                    self.own(owner.clone(), object(DeletionKind::PhysicsNode, &node.id));
                }
            }
        }
        for group in d.model3_settings().groups.iter().flatten() {
            self.checkpoint(control)?;
            let owner = object(DeletionKind::ModelParameterGroup, &group.id);
            self.record(owner.clone());
            for member in &group.parameters {
                self.checkpoint(control)?;
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
            self.checkpoint(control)?;
            let owner = object(DeletionKind::CdiCombinedSet, &set.id);
            self.record(owner.clone());
            for member in &set.members {
                self.checkpoint(control)?;
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

    pub(in crate::state::deletion) fn curves<'a>(
        &mut self,
        model: &Model<'_>,
        removed: impl IntoIterator<Item = &'a DeletionObject>,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let mut scope = CurveScope::default();
        for object in removed {
            self.checkpoint(control)?;
            let id = object.id.as_str();
            let owner = object.owner_id.as_deref();
            match (object.kind, owner) {
                (DeletionKind::Motion, _) => {
                    scope.motions.insert(id);
                }
                (DeletionKind::Expression, _) => {
                    scope.expressions.insert(id);
                }
                (DeletionKind::Parameter, _) => {
                    scope.parameters.insert(id);
                }
                (DeletionKind::Part, _) => {
                    scope.parts.insert(id);
                }
                (DeletionKind::MotionTrack, None) | (DeletionKind::ExpressionTrack, Some(_)) => {
                    scope.tracks.insert((owner, id));
                }
                (DeletionKind::MotionKey, None) | (DeletionKind::ExpressionKey, Some(_)) => {
                    scope.keys.insert((owner, id));
                }
                _ => {}
            }
        }
        if scope.motions.is_empty()
            && scope.expressions.is_empty()
            && scope.parameters.is_empty()
            && scope.parts.is_empty()
            && scope.tracks.is_empty()
            && scope.keys.is_empty()
        {
            return Ok(());
        }
        for id in model.motion_order() {
            self.checkpoint(control)?;
            let clip = model.get_motion(id).unwrap();
            self.clip_tracks(
                &scope,
                object(DeletionKind::Motion, id),
                &clip.tracks,
                control,
            )?;
        }
        for id in model.expression_order() {
            self.checkpoint(control)?;
            if let Some(scene) = model.get_expression(id).unwrap().source_scene() {
                self.clip_tracks(
                    &scope,
                    object(DeletionKind::Expression, id),
                    &scene.tracks,
                    control,
                )?;
            }
        }
        Ok(())
    }

    fn clip_tracks(
        &mut self,
        scope: &CurveScope<'_>,
        owner: DeletionObject,
        tracks: &[MotionTrack],
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let expression = (owner.kind == DeletionKind::Expression).then_some(owner.id.as_str());
        let (kind, all) = if let Some(id) = expression {
            (
                DeletionKind::ExpressionTrack,
                scope.expressions.contains(id),
            )
        } else {
            (
                DeletionKind::MotionTrack,
                scope.motions.contains(owner.id.as_str()),
            )
        };
        for track in tracks {
            self.checkpoint(control)?;
            let target = match &track.target {
                MotionTrackTarget::Parameter { parameter_id } => {
                    scope.parameters.contains(parameter_id.as_str())
                }
                MotionTrackTarget::PartOpacity { part_id } => {
                    scope.parts.contains(part_id.as_str())
                }
                _ => false,
            };
            if !all && !target && !scope.tracks.contains(&(expression, track.id.as_str())) {
                let mut selected_key = false;
                if !scope.keys.is_empty() {
                    for key in track.keys() {
                        control.checkpoint()?;
                        if scope.keys.contains(&(expression, key.id.as_str())) {
                            selected_key = true;
                            break;
                        }
                    }
                }
                if !selected_key {
                    continue;
                }
            }
            self.own(
                owner.clone(),
                DeletionObject {
                    kind,
                    id: track.id.clone(),
                    owner_id: expression.map(str::to_owned),
                },
            );
            self.track(track, expression, control)?;
        }
        Ok(())
    }
}
