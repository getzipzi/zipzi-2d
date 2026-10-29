//! Cross-domain patches are composed against the frozen typed removal set.
//! None of these known references are converted to unresolved imported values.
use super::*;

impl Document {
    pub(super) fn resolve_motion_registration_deletion(
        &self,
        removed: &mut BTreeMap<DeletionObject, DeletionReason>,
        report: &mut DeletionReport,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let mut by_clip: BTreeMap<&str, Vec<(&MotionRegistration, Option<&str>)>> = BTreeMap::new();
        for entry in self.motion_roots() {
            by_clip
                .entry(&entry.clip_id)
                .or_default()
                .push((entry, None));
        }
        for group in self.motion_groups() {
            for entry in &group.entries {
                by_clip
                    .entry(&entry.clip_id)
                    .or_default()
                    .push((entry, Some(&group.id)));
            }
        }
        let mut retained = BTreeSet::new();
        for (clip, entries) in by_clip {
            control.checkpoint()?;
            if removed.contains_key(&object(DeletionKind::Motion, clip))
                || !entries.iter().all(|(e, _)| {
                    removed.contains_key(&object(DeletionKind::MotionRegistration, &e.id))
                })
            {
                continue;
            }
            if let Some((entry, group)) = entries.iter().find(|(e, _)| {
                removed.get(&object(DeletionKind::MotionRegistration, &e.id))
                    == Some(&DeletionReason::Owned)
            }) {
                let key = object(DeletionKind::MotionRegistration, &entry.id);
                removed.remove(&key);
                retained.insert(key.clone());
                report.rewritten.push(DeletionRelation {
                    owner: key.clone(),
                    field: DeletionField::MotionRegistrationContainer,
                    target: object(DeletionKind::MotionGroup, group.unwrap()),
                });
                report
                    .effects
                    .push(DeletionEffect::MotionRegistrationMoved {
                        registration: key,
                        group: object(DeletionKind::MotionGroup, group.unwrap()),
                    });
            } else {
                report.decisions.push(DeletionDecision {
                    object: object(DeletionKind::Motion, clip),
                    reason: DeletionDecisionReason::InvalidContainer {
                        code: "LAST_MOTION_REGISTRATION".into(),
                    },
                    choices: vec![DeletionChoice::DeleteSubtree, DeletionChoice::Cancel],
                });
            }
        }
        report
            .relations
            .retain(|r| r.field != DeletionField::Owned || !retained.contains(&r.target));
        Ok(())
    }

    pub(super) fn analyze_business_deletion(
        &self,
        request: &DeletionRequest,
        removed: &BTreeMap<DeletionObject, DeletionReason>,
        report: &mut DeletionReport,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let deleted = |kind, id: &str| removed.contains_key(&object(kind, id));
        for (id, &value) in &request.parameter_slices {
            let p = self
                .get_parameter(id)
                .ok_or_else(|| Status::error("MISSING_PARAMETER", id))?;
            if !value.is_finite() || value < p.minimum || value > p.maximum {
                return Err(Status::error("INVALID_PARAMETER_SLICE", id));
            }
        }
        let mut textures: BTreeMap<DeletionObject, Vec<&DeletionRelation>> = BTreeMap::new();
        for relation in report
            .rewritten
            .iter()
            .filter(|r| r.field == DeletionField::Texture)
        {
            textures
                .entry(relation.target.clone())
                .or_default()
                .push(relation);
        }
        for (texture, relations) in textures {
            let replacement = request.texture_replacements.get(&texture.id);
            let valid = replacement.is_some_and(|v| {
                v.as_ref().is_none_or(|id| {
                    self.get_asset(id).is_some() && !deleted(DeletionKind::Texture, id)
                })
            });
            if !valid {
                report.decisions.push(DeletionDecision {
                    object: texture,
                    reason: DeletionDecisionReason::TextureReplacementRequired,
                    choices: vec![
                        DeletionChoice::ChooseTexture,
                        DeletionChoice::ClearTexture,
                        DeletionChoice::Cancel,
                    ],
                });
            } else {
                for relation in relations {
                    report.effects.push(DeletionEffect::ResourceReplaced {
                        object: relation.owner.clone(),
                        source: texture.clone(),
                        replacement: replacement
                            .unwrap()
                            .as_ref()
                            .map(|id| object(DeletionKind::Texture, id)),
                    });
                }
            }
        }
        for relation in &report.rewritten {
            if matches!(
                relation.field,
                DeletionField::AttachmentSound | DeletionField::AttachmentUserData
            ) {
                report.effects.push(DeletionEffect::ResourceReplaced {
                    object: relation.owner.clone(),
                    source: relation.target.clone(),
                    replacement: None,
                });
            }
        }
        for relation in &report.relations {
            if matches!(
                relation.field,
                DeletionField::MotionParameter
                    | DeletionField::ExpressionParameter
                    | DeletionField::ExpressionSourceParameter
            ) {
                report
                    .effects
                    .push(DeletionEffect::ParameterAnimationRemoved(
                        relation.owner.clone(),
                    ));
            }
        }
        for removal in removed.keys().filter(|o| {
            matches!(
                o.kind,
                DeletionKind::BlendBinding
                    | DeletionKind::MeshBinding
                    | DeletionKind::SceneBinding
                    | DeletionKind::GlueBinding
            )
        }) {
            let host = self
                .get_blend_binding(&removal.id)
                .map(|b| b.target_id.as_str())
                .or_else(|| self.get_binding(&removal.id).map(|b| b.mesh_id.as_str()))
                .or_else(|| self.get_scene_binding(&removal.id).map(|b| b.target_id()))
                .or_else(|| {
                    (removal.kind == DeletionKind::GlueBinding).then_some(removal.id.as_str())
                });
            if let Some(host) = host
                .and_then(|id| self.deletion_scene_object(id))
                .filter(|o| !removed.contains_key(o))
            {
                report
                    .effects
                    .push(DeletionEffect::BindingContributionRemoved {
                        object: host,
                        binding: removal.clone(),
                    });
            }
        }
        for binding in removed
            .keys()
            .filter(|k| k.kind == DeletionKind::SceneBinding)
        {
            let b = self.get_scene_binding(&binding.id).unwrap();
            if deleted(DeletionKind::Part, b.target_id()) {
                continue;
            }
            if let Some(os) = self
                .offscreen_for_part(b.target_id())
                .filter(|o| !deleted(DeletionKind::Offscreen, &o.id))
            {
                let mapping = request
                    .offscreen_placements
                    .get(&os.id)
                    .filter(|p| p.part_id == os.part_id)
                    .and_then(|p| p.mapping.as_ref());
                let valid = mapping.is_some_and(|m| {
                    (m.is_empty() || m.len() == 1)
                        && m.iter().all(|&i| i < 0 || (i as usize) < os.keyforms.len())
                });
                if !valid {
                    report.decisions.push(DeletionDecision {
                        object: object(DeletionKind::Offscreen, &os.id),
                        reason: DeletionDecisionReason::OffscreenMappingRequired,
                        choices: vec![
                            DeletionChoice::SupplyOffscreenMapping,
                            DeletionChoice::DeleteSubtree,
                            DeletionChoice::Cancel,
                        ],
                    });
                } else {
                    let key = object(DeletionKind::Offscreen, &os.id);
                    report.rewritten.push(DeletionRelation {
                        owner: key.clone(),
                        field: DeletionField::ConvertedData,
                        target: binding.clone(),
                    });
                    report.effects.push(DeletionEffect::CompositionChanged(key));
                }
            }
        }
        let physics_ids: BTreeSet<_> = removed
            .keys()
            .filter(|k| {
                matches!(
                    k.kind,
                    DeletionKind::PhysicsInput
                        | DeletionKind::PhysicsOutput
                        | DeletionKind::PhysicsNode
                )
            })
            .map(|k| k.id.as_str())
            .collect();
        if let Some(asset) = self
            .physics()
            .filter(|p| !deleted(DeletionKind::Physics, &p.id))
        {
            for group in &asset.data.settings {
                control.checkpoint()?;
                if deleted(DeletionKind::PhysicsGroup, &group.id) {
                    continue;
                }
                if !group.entity_ids().any(|id| physics_ids.contains(id)) {
                    continue;
                }
                let mut proposed = group.as_ref().clone();
                proposed
                    .inputs
                    .retain(|c| !deleted(DeletionKind::PhysicsInput, &c.id));
                proposed
                    .outputs
                    .retain(|c| !deleted(DeletionKind::PhysicsOutput, &c.id));
                proposed
                    .vertices
                    .retain(|c| !deleted(DeletionKind::PhysicsNode, &c.id));
                let retained_nodes: BTreeSet<_> =
                    proposed.vertices.iter().map(|v| v.id.as_str()).collect();
                for output in &mut proposed.outputs {
                    if deleted(DeletionKind::PhysicsNode, &output.node_id) {
                        if let Some(replacement) = request
                            .physics_node_replacements
                            .get(&output.node_id)
                            .filter(|id| retained_nodes.contains(id.as_str()))
                        {
                            output.node_id = replacement.clone();
                        } else {
                            report.decisions.push(DeletionDecision {
                                object: object(DeletionKind::PhysicsNode, &output.node_id),
                                reason: DeletionDecisionReason::PhysicsNodeReplacementRequired,
                                choices: vec![
                                    DeletionChoice::ChoosePhysicsNode,
                                    DeletionChoice::Cancel,
                                ],
                            });
                        }
                    }
                }
                if proposed != **group {
                    for input in &group.inputs {
                        if deleted(DeletionKind::PhysicsInput, &input.id) {
                            report.effects.push(DeletionEffect::PhysicsChannelRemoved {
                                object: object(DeletionKind::PhysicsInput, &input.id),
                                group: object(DeletionKind::PhysicsGroup, &group.id),
                            });
                        }
                    }
                    for output in &group.outputs {
                        if deleted(DeletionKind::PhysicsOutput, &output.id) {
                            report.effects.push(DeletionEffect::PhysicsChannelRemoved {
                                object: object(DeletionKind::PhysicsOutput, &output.id),
                                group: object(DeletionKind::PhysicsGroup, &group.id),
                            });
                        }
                    }
                    let validity = if group
                        .vertices
                        .first()
                        .is_some_and(|v| deleted(DeletionKind::PhysicsNode, &v.id))
                    {
                        Err(Status::error("PHYSICS_ROOT_NODE", &group.id))
                    } else if proposed.vertices.len() < group.vertices.len()
                        && proposed.vertices.len() < 2
                    {
                        Err(Status::error("PHYSICS_CHAIN_TOO_SHORT", &group.id))
                    } else {
                        self.validate_native_physics_rig(&proposed)
                    };
                    if let Err(status) = validity {
                        report.decisions.push(DeletionDecision {
                            object: object(DeletionKind::PhysicsGroup, &group.id),
                            reason: DeletionDecisionReason::InvalidContainer { code: status.code },
                            choices: vec![DeletionChoice::DeleteSubtree, DeletionChoice::Cancel],
                        });
                    }
                }
            }
        }
        // Removing a drawing role never removes its Part or animation records.
        for key in removed
            .keys()
            .filter(|k| k.kind == DeletionKind::DrawingGroup && !deleted(DeletionKind::Part, &k.id))
        {
            if let Some(os) = self
                .offscreen_for_part(&key.id)
                .filter(|o| !deleted(DeletionKind::Offscreen, &o.id))
            {
                report.decisions.push(DeletionDecision {
                    object: object(DeletionKind::Offscreen, &os.id),
                    reason: DeletionDecisionReason::OffscreenOwnerUnavailable,
                    choices: vec![DeletionChoice::DeleteSubtree, DeletionChoice::Cancel],
                });
            }
        }
        Ok(())
    }

    pub(super) fn install_business_deletion(
        &self,
        candidate: &mut Document,
        request: &DeletionRequest,
        report: &DeletionReport,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let removed: BTreeSet<_> = report.removed.iter().map(|r| r.object.clone()).collect();
        let deleted = |kind, id: &str| removed.contains(&object(kind, id));
        let scoped_deleted = |kind, id: &str, owner: &str| {
            removed.contains(&DeletionObject {
                kind,
                id: id.into(),
                owner_id: Some(owner.into()),
            })
        };
        for binding in removed
            .iter()
            .filter(|k| k.kind == DeletionKind::SceneBinding)
        {
            let b = self.get_scene_binding(&binding.id).unwrap();
            if let Some(os) = self.offscreen_for_part(b.target_id()) {
                if let Some(mapping) = request
                    .offscreen_placements
                    .get(&os.id)
                    .filter(|p| p.part_id == os.part_id)
                    .and_then(|p| p.mapping.clone())
                {
                    if let Some(retained) = candidate.content.offscreens.get_mut(&os.id) {
                        retained.part_keyform_indices = mapping;
                    }
                }
            }
        }
        for glue in candidate.content.glues.values_mut() {
            if deleted(DeletionKind::GlueBinding, &glue.id) {
                glue.binding = None;
            }
        }
        candidate
            .content
            .parameters
            .retain(|id, _| !deleted(DeletionKind::Parameter, id));
        candidate
            .content
            .parameter_order
            .retain(|id| !deleted(DeletionKind::Parameter, id));
        candidate
            .content
            .assets
            .retain(|id, _| !deleted(DeletionKind::Texture, id));
        candidate
            .content
            .asset_order
            .retain(|id| !deleted(DeletionKind::Texture, id));
        for mesh in candidate.content.meshes.values_mut() {
            control.checkpoint()?;
            if let Some(source) = mesh
                .texture_asset_id
                .resolved_id()
                .filter(|id| deleted(DeletionKind::Texture, id))
            {
                mesh.texture_asset_id = match request.texture_replacements.get(source) {
                    Some(Some(id)) => crate::SceneReference::Resolved {
                        object_id: id.clone(),
                    },
                    Some(None) => crate::SceneReference::Unspecified,
                    None => return Err(Status::error("DELETION_DECISION_REQUIRED", source)),
                };
            }
        }
        candidate
            .content
            .blend_key_tables
            .retain(|id, _| !deleted(DeletionKind::BlendKeyTable, id));
        candidate
            .content
            .blend_key_table_order
            .retain(|id| !deleted(DeletionKind::BlendKeyTable, id));
        candidate
            .content
            .blend_constraints
            .retain(|id, _| !deleted(DeletionKind::BlendConstraint, id));
        candidate
            .content
            .blend_constraint_order
            .retain(|id| !deleted(DeletionKind::BlendConstraint, id));
        for b in candidate.content.blend_bindings.values_mut() {
            b.constraint_ids
                .retain(|id| !deleted(DeletionKind::BlendConstraint, id));
        }
        candidate
            .content
            .package_attachments
            .retain(|a| !deleted(DeletionKind::Attachment, &a.id));
        for relation in report
            .rewritten
            .iter()
            .filter(|r| r.field == DeletionField::MotionRegistrationContainer)
        {
            let (_, entry) = self.get_motion_registration(&relation.owner.id).unwrap();
            candidate.content.motion_roots.push(entry.clone());
        }
        candidate
            .content
            .motion_groups
            .retain(|g| !deleted(DeletionKind::MotionGroup, &g.id));
        for g in &mut candidate.content.motion_groups {
            g.entries
                .retain(|e| !deleted(DeletionKind::MotionRegistration, &e.id));
        }
        candidate
            .content
            .motion_roots
            .retain(|e| !deleted(DeletionKind::MotionRegistration, &e.id));
        for e in candidate
            .content
            .motion_groups
            .iter_mut()
            .flat_map(|g| &mut g.entries)
            .chain(&mut candidate.content.motion_roots)
        {
            if matches!(&e.sound,Some(AttachmentRef::Resolved {attachment_id}) if deleted(DeletionKind::Attachment,attachment_id))
            {
                e.sound = None;
            }
        }
        if matches!(&candidate.content.model3_settings.user_data,Some(AttachmentRef::Resolved {attachment_id}) if deleted(DeletionKind::Attachment,attachment_id))
        {
            candidate.content.model3_settings.user_data = None;
        }
        candidate
            .content
            .motions
            .retain(|id, _| !deleted(DeletionKind::Motion, id));
        candidate
            .content
            .motion_order
            .retain(|id| !deleted(DeletionKind::Motion, id));
        for clip in candidate.content.motions.values_mut() {
            control.checkpoint()?;
            if clip
                .tracks
                .iter()
                .any(|t| deleted(DeletionKind::MotionTrack, &t.id))
                || clip
                    .events
                    .iter()
                    .any(|e| deleted(DeletionKind::MotionEvent, &e.id))
            {
                let clip = Arc::make_mut(clip);
                clip.tracks
                    .retain(|t| !deleted(DeletionKind::MotionTrack, &t.id));
                clip.events
                    .retain(|e| !deleted(DeletionKind::MotionEvent, &e.id));
                let retained: BTreeSet<_> = clip.tracks.iter().map(|t| &t.id).collect();
                clip.track_order.retain(|id| retained.contains(id));
            }
        }
        candidate
            .content
            .expressions
            .retain(|id, _| !deleted(DeletionKind::Expression, id));
        candidate
            .content
            .expression_order
            .retain(|id| !deleted(DeletionKind::Expression, id));
        for expression in candidate.content.expressions.values_mut() {
            control.checkpoint()?;
            expression
                .entries
                .retain(|e| !deleted(DeletionKind::ExpressionEntry, &e.id));
            if let Some(mut scene) = expression.source_scene() {
                let before = (scene.tracks.len(), scene.events.len());
                scene.tracks.retain(|t| {
                    !scoped_deleted(DeletionKind::ExpressionTrack, &t.id, &expression.id)
                });
                scene
                    .events
                    .retain(|e| !scoped_deleted(DeletionKind::MotionEvent, &e.id, &expression.id));
                if before != (scene.tracks.len(), scene.events.len()) {
                    let retained: BTreeSet<_> = scene.tracks.iter().map(|t| &t.id).collect();
                    scene.track_order.retain(|id| retained.contains(id));
                    expression.preserve_source_scene(&scene);
                }
            }
        }
        let changed_ids: BTreeSet<_> = removed
            .iter()
            .filter(|k| k.owner_id.is_none())
            .map(|k| k.id.as_str())
            .collect();
        if candidate
            .pose()
            .is_some_and(|p| deleted(DeletionKind::Pose, &p.id))
        {
            candidate.content.pose = None;
        } else if let Some(pose) = &mut candidate.content.pose {
            if pose.entity_ids().any(|id| changed_ids.contains(id)) {
                let pose = Arc::make_mut(pose);
                pose.groups
                    .retain(|g| !deleted(DeletionKind::PoseGroup, &g.id));
                for group in &mut pose.groups {
                    group
                        .entries
                        .retain(|e| !deleted(DeletionKind::PoseEntry, &e.id));
                    for entry in &mut group.entries {
                        if let Some(links) = &mut entry.links {
                            links.retain(|l| !deleted(DeletionKind::PoseLink, &l.id));
                        }
                    }
                }
            }
        }
        if candidate
            .physics()
            .is_some_and(|p| deleted(DeletionKind::Physics, &p.id))
        {
            candidate.content.physics = None;
        } else if let Some(asset) = &mut candidate.content.physics {
            let changed = asset.entity_ids().any(|id| changed_ids.contains(id));
            if changed {
                let asset = Arc::make_mut(asset);
                asset
                    .data
                    .settings
                    .retain(|g| !deleted(DeletionKind::PhysicsGroup, &g.id));
                for group in &mut asset.data.settings {
                    if group.entity_ids().any(|id| changed_ids.contains(id)) {
                        let group = Arc::make_mut(group);
                        group
                            .inputs
                            .retain(|v| !deleted(DeletionKind::PhysicsInput, &v.id));
                        group
                            .outputs
                            .retain(|v| !deleted(DeletionKind::PhysicsOutput, &v.id));
                        group
                            .vertices
                            .retain(|v| !deleted(DeletionKind::PhysicsNode, &v.id));
                        for output in &mut group.outputs {
                            if let Some(replacement) = request
                                .physics_node_replacements
                                .get(&output.node_id)
                                .filter(|_| deleted(DeletionKind::PhysicsNode, &output.node_id))
                            {
                                output.node_id = replacement.clone();
                            }
                        }
                    }
                }
                asset
                    .data
                    .meta
                    .dictionary
                    .retain(|g| !deleted(DeletionKind::PhysicsGroup, &g.id));
                asset.reconcile_counts();
            }
        }
        let retained_folder = |parent: Option<&str>| {
            let mut parent = parent.map(str::to_owned);
            while let Some(id) = parent
                .as_ref()
                .filter(|id| deleted(DeletionKind::ParameterFolder, id))
            {
                parent = self
                    .get_parameter_group(id)
                    .and_then(|g| g.parent_id.clone());
            }
            parent
        };
        if let Some(groups) = &mut candidate.content.display_info.parameter_groups {
            groups.retain(|g| !deleted(DeletionKind::ParameterFolder, &g.id));
            for g in groups {
                g.parent_id = retained_folder(g.parent_id.as_deref());
            }
        }
        if let Some(entries) = &mut candidate.content.display_info.parts {
            entries.retain(|entry|!matches!(entry,CdiPartEntry::Resolved {part_id,..} if deleted(DeletionKind::Part,part_id)));
        }
        if let Some(entries) = &mut candidate.content.display_info.parameters {
            entries.retain(|e| !matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if deleted(DeletionKind::Parameter,parameter_id)));
            for entry in entries {
                match entry {
                    CdiParameterEntry::Resolved { group_id, .. }
                    | CdiParameterEntry::Unresolved { group_id, .. } => {
                        *group_id = retained_folder(group_id.as_deref());
                    }
                }
            }
        }
        if let Some(sets) = &mut candidate.content.display_info.combined_parameters {
            sets.retain(|s| !deleted(DeletionKind::CdiCombinedSet, &s.id));
            for set in sets {
                set.members
                    .retain(|m| !deleted(DeletionKind::CdiCombinedMember, &m.id));
            }
        }
        if let Some(groups) = &mut candidate.content.model3_settings.groups {
            groups.retain(|g| !deleted(DeletionKind::ModelParameterGroup, &g.id));
            for group in groups {
                group
                    .parameters
                    .retain(|m| !deleted(DeletionKind::ModelParameterMember, &m.id));
            }
        }
        for key in &removed {
            if key.owner_id.is_none()
                && !matches!(
                    key.kind,
                    DeletionKind::DrawingGroup | DeletionKind::GlueBinding
                )
            {
                candidate.content.editor_state.objects.remove(&key.id);
            }
        }
        candidate.binding_lookup.take();
        candidate.lookup.take();
        candidate.expression_entries.take();
        candidate.motion_lookup.take();
        candidate.reference_index.take();
        candidate.organization_index.take();
        Ok(())
    }
}
