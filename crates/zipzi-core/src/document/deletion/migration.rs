use super::*;

impl Document {
    fn deletion_reparent_edit(
        &self,
        request: &DeletionRequest,
        object: &DeletionObject,
        parent_id: String,
        strategy: ReparentStrategy,
    ) -> ReparentEdit {
        let mut required = BTreeSet::new();
        if strategy != ReparentStrategy::KeepLocalData {
            let mut pending = vec![object.id.as_str(), parent_id.as_str()];
            let mut seen = BTreeSet::new();
            while let Some(id) = pending.pop() {
                if id.is_empty() || !seen.insert(id) {
                    continue;
                }
                required.extend(self.reparent_drivers(id));
                if let Some(parent) = self.scene_deformer_parent(id) {
                    pending.push(parent);
                }
            }
        }
        let context = if strategy == ReparentStrategy::KeepLocalData {
            None
        } else if let Some(context) = &request.conversion_context {
            let mut context = context.clone();
            context
                .parameter_values
                .retain(|id, _| required.contains(id));
            context
                .parameter_domain
                .retain(|d| required.contains(&d.parameter_id));
            if strategy == ReparentStrategy::PreserveEditState {
                context.parameter_domain.clear();
            }
            Some(context)
        } else if strategy == ReparentStrategy::PreserveParameterDomain {
            Some(ReparentContext {
                parameter_values: required
                    .iter()
                    .map(|id| (id.clone(), self.get_parameter(id).unwrap().default_value))
                    .collect(),
                parameter_domain: required
                    .iter()
                    .map(|id| {
                        let p = self.get_parameter(id).unwrap();
                        ConversionDomain {
                            parameter_id: id.clone(),
                            minimum: p.minimum,
                            maximum: p.maximum,
                        }
                    })
                    .collect(),
                edit_time_seconds: 0.,
            })
        } else {
            None
        };
        ReparentEdit {
            sources: vec![object.id.clone()],
            parent_id,
            strategy,
            placement: None,
            tolerance: request.tolerance.absolute,
            relative_tolerance: request.tolerance.relative,
            context,
        }
    }

    pub(super) fn deletion_migrations(
        &self,
        request: &DeletionRequest,
        removed: &BTreeMap<DeletionObject, DeletionReason>,
        report: &mut DeletionReport,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let removed_ids: BTreeSet<_> = removed.keys().map(|o| o.id.as_str()).collect();
        let mut offscreen_destinations = BTreeSet::new();
        for id in self.transform_order().iter().chain(self.mesh_order()) {
            control.checkpoint()?;
            let owner = self.deletion_scene_object(id).unwrap();
            if removed.contains_key(&owner) {
                continue;
            }
            let parent = self.scene_deformer_parent(id).unwrap_or("");
            if parent.is_empty() || !removed_ids.contains(parent) {
                continue;
            }
            let mut destination = parent;
            while !destination.is_empty() && removed_ids.contains(destination) {
                destination = self.get_transform(destination).unwrap().parent();
            }
            let strategy = request
                .reparent
                .get(id)
                .copied()
                .unwrap_or(ReparentStrategy::PreserveParameterDomain);
            let edit = self.deletion_reparent_edit(request, &owner, destination.into(), strategy);
            if let Err(status) = self.deletion_reparent_supported(&edit) {
                let mut choices = vec![];
                if request.conversion_context.is_some() {
                    let state = self.deletion_reparent_edit(
                        request,
                        &owner,
                        destination.into(),
                        ReparentStrategy::PreserveEditState,
                    );
                    if self.deletion_reparent_supported(&state).is_ok() {
                        choices.push(DeletionChoice::PreserveEditState);
                    }
                }
                choices.extend([
                    DeletionChoice::KeepLocalData,
                    DeletionChoice::DeleteSubtree,
                    DeletionChoice::Cancel,
                ]);
                report.decisions.push(DeletionDecision {
                    object: owner.clone(),
                    reason: DeletionDecisionReason::ConversionUnproven { code: status.code },
                    choices,
                });
            }
            report.rewritten.push(DeletionRelation {
                owner: owner.clone(),
                field: DeletionField::DeformationParent,
                target: object(DeletionKind::Deformer, parent),
            });
            report.migrations.push(DeletionMigration {
                object: owner,
                parent_id: destination.into(),
                strategy,
                guarantee: None,
                limited_by: vec![],
            });
            report.effects.push(DeletionEffect::UnpreservedChannels {
                object: self.deletion_scene_object(id).unwrap(),
                channels: vec![
                    DeletionChannel::Appearance,
                    DeletionChannel::Enabled,
                    DeletionChannel::GluePostprocessing,
                    DeletionChannel::DrawingOrder,
                ],
            });
        }
        for id in self.scene_organization_order() {
            control.checkpoint()?;
            let owner = self.deletion_scene_object(&id).unwrap();
            if removed.contains_key(&owner) {
                continue;
            }
            let parent = self.scene_organization_owner(&id).unwrap_or("");
            if parent.is_empty() || !removed_ids.contains(parent) {
                continue;
            }
            let mut destination = parent;
            while !destination.is_empty() && removed_ids.contains(destination) {
                destination = &self.get_part(destination).unwrap().parent_id;
            }
            report.rewritten.push(DeletionRelation {
                owner: owner.clone(),
                field: DeletionField::OrganizationParent,
                target: object(DeletionKind::Part, parent),
            });
            report
                .effects
                .push(DeletionEffect::OrganizationChanged(owner.clone()));
            if let Some(os) = self.get_offscreen(&id) {
                let placement = request.offscreen_placements.get(&id);
                let destination = placement.map_or(destination, |p| p.part_id.as_str());
                let valid = !destination.is_empty()
                    && self.get_part(destination).is_some()
                    && !removed_ids.contains(destination)
                    && self.offscreen_for_part(destination).is_none_or(|other| {
                        other.id == os.id || removed_ids.contains(other.id.as_str())
                    })
                    && offscreen_destinations.insert(destination.to_owned());
                if !valid {
                    report.decisions.push(DeletionDecision {
                        object: owner.clone(),
                        reason: DeletionDecisionReason::OffscreenOwnerUnavailable,
                        choices: vec![
                            DeletionChoice::ChoosePart,
                            DeletionChoice::DeleteSubtree,
                            DeletionChoice::Cancel,
                        ],
                    });
                } else {
                    let mapping = placement.and_then(|p| p.mapping.as_ref());
                    let count = self
                        .binding_for_scene(destination)
                        .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                        .map_or(1, |b| b.track.len());
                    let needs_mapping = !os.part_keyform_indices.is_empty() && mapping.is_none();
                    let invalid_mapping = mapping.is_some_and(|m| {
                        (!m.is_empty() && m.len() != count)
                            || m.iter().any(|&i| i >= 0 && i as usize >= os.keyforms.len())
                    });
                    if needs_mapping || invalid_mapping {
                        report.decisions.push(DeletionDecision {
                            object: owner.clone(),
                            reason: DeletionDecisionReason::OffscreenMappingRequired,
                            choices: vec![
                                DeletionChoice::SupplyOffscreenMapping,
                                DeletionChoice::DeleteSubtree,
                                DeletionChoice::Cancel,
                            ],
                        });
                    }
                }
                report
                    .organization_migrations
                    .push(DeletionOrganizationMigration {
                        object: owner.clone(),
                        parent_id: destination.into(),
                        offscreen_mapping: placement.and_then(|p| p.mapping.clone()),
                    });
                report
                    .effects
                    .push(DeletionEffect::CompositionChanged(owner));
            } else {
                report
                    .organization_migrations
                    .push(DeletionOrganizationMigration {
                        object: owner,
                        parent_id: destination.into(),
                        offscreen_mapping: None,
                    });
            }
        }
        for root in &request.roots {
            if let DeletionTarget::DissolvePart(id) = root {
                if report
                    .rewritten
                    .iter()
                    .any(|r| r.field == DeletionField::OrganizationParent && r.target.id == *id)
                {
                    report.effects.push(DeletionEffect::PartStateRemoved {
                        part: root.object(),
                        enabled: self.get_part(id).unwrap().enabled,
                        animated_order: self
                            .binding_for_scene(id)
                            .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                            || self
                                .iter_active_blend_bindings_for_target(id)
                                .next()
                                .is_some(),
                    });
                }
                if report
                    .rewritten
                    .iter()
                    .any(|r| r.field == DeletionField::OrganizationParent && r.target.id == *id)
                    && !self.part_dissolve_state_representable(id)
                    && request.reparent.get(id) != Some(&ReparentStrategy::KeepLocalData)
                {
                    report.decisions.push(DeletionDecision {
                        object: root.object(),
                        reason: DeletionDecisionReason::PartStateUnpreserved,
                        choices: vec![
                            DeletionChoice::KeepLocalData,
                            DeletionChoice::DeleteSubtree,
                            DeletionChoice::Cancel,
                        ],
                    });
                }
            }
        }
        for group in self.draw_order_groups().into_iter().flatten() {
            if removed_ids.contains(group.owner.as_str()) {
                let key = object(DeletionKind::DrawingGroup, &group.owner);
                let groups = self.draw_order_groups().unwrap();
                let mut destination = groups.iter().find(|g| g.items.contains(&group.owner));
                while let Some(parent) =
                    destination.filter(|g| removed_ids.contains(g.owner.as_str()))
                {
                    destination = groups.iter().find(|g| g.items.contains(&parent.owner));
                }
                if let Some(destination) = destination {
                    report.effects.push(DeletionEffect::DrawingGroupFlattened {
                        group: key,
                        destination: object(
                            DeletionKind::DrawingGroup,
                            if destination.owner.is_empty() {
                                self.id()
                            } else {
                                &destination.owner
                            },
                        ),
                        source_range: [group.min_order, group.max_order],
                        destination_range: [destination.min_order, destination.max_order],
                        dynamic_order: self
                            .binding_for_scene(&group.owner)
                            .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                            || self
                                .iter_active_blend_bindings_for_target(&group.owner)
                                .next()
                                .is_some(),
                        offscreen: self
                            .offscreen_for_part(&group.owner)
                            .map(|os| object(DeletionKind::Offscreen, &os.id)),
                    });
                }
                for item in &group.items {
                    if !removed_ids.contains(item.as_str()) {
                        if let Some(owner) = self.deletion_scene_object(item) {
                            report.rewritten.push(DeletionRelation {
                                owner,
                                field: DeletionField::DrawingItem {
                                    slot: group.items.iter().position(|v| v == item).unwrap(),
                                },
                                target: object(DeletionKind::DrawingGroup, &group.owner),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn install_deletion_migrations(
        &self,
        candidate: &mut Document,
        request: &DeletionRequest,
        report: &mut DeletionReport,
        limits: AuthoringLimits,
        control: &ConversionControl,
        cost: &mut ConversionCost,
    ) -> Result<bool, Status> {
        // Descendants migrate before ancestors, so each conversion sees the
        // original parent context while collapsing chains of removed nodes.
        report.migrations.sort_by_key(|m| {
            let mut depth = 0;
            let mut parent = self.scene_deformer_parent(&m.object.id).unwrap_or("");
            while !parent.is_empty() {
                depth += 1;
                parent = self.get_transform(parent).unwrap().parent();
            }
            (std::cmp::Reverse(depth), m.object.clone())
        });
        for migration in &mut report.migrations {
            control.checkpoint()?;
            let edit = self.deletion_reparent_edit(
                request,
                &migration.object,
                migration.parent_id.clone(),
                migration.strategy,
            );
            let conversion = match candidate.plan_deletion_reparent(edit, limits, control) {
                Ok(plan) => plan,
                Err(status) if status.code.starts_with("AUTHORING_") => return Err(status),
                Err(status) => {
                    report.decisions.push(DeletionDecision {
                        object: migration.object.clone(),
                        reason: DeletionDecisionReason::ConversionUnproven { code: status.code },
                        choices: vec![
                            DeletionChoice::KeepLocalData,
                            DeletionChoice::DeleteSubtree,
                            DeletionChoice::Cancel,
                        ],
                    });
                    return Ok(false);
                }
            };
            migration.guarantee = Some(conversion.guarantee().clone());
            let payload = cost
                .payload_bytes
                .checked_add(conversion.cost().payload_bytes)
                .ok_or_else(|| {
                    Status::error("AUTHORING_CAPACITY_OVERFLOW", "Deletion conversions")
                })?;
            *cost = limits.check_conversion(
                cost.sample_combinations
                    .max(conversion.cost().sample_combinations),
                payload,
                self.estimated_content_bytes(),
            )?;
            let result = candidate.apply_reparent_plan(&conversion);
            if !result.status.is_ok() {
                return Err(result.status);
            }
            if migration.strategy != ReparentStrategy::PreserveParameterDomain {
                report
                    .effects
                    .push(DeletionEffect::DeformationChanged(migration.object.clone()));
            }
        }
        let local: BTreeMap<_, _> = report
            .migrations
            .iter()
            .filter(|m| m.strategy == ReparentStrategy::KeepLocalData)
            .map(|m| (m.object.id.clone(), m.object.clone()))
            .collect();
        for migration in &mut report.migrations {
            let mut parent = candidate
                .scene_deformer_parent(&migration.object.id)
                .unwrap_or("");
            while !parent.is_empty() {
                if let Some(object) = local.get(parent) {
                    migration.limited_by.push(object.clone());
                }
                parent = candidate.get_transform(parent).unwrap().parent();
            }
            if !migration.limited_by.is_empty() {
                migration.guarantee = None;
                report
                    .effects
                    .push(DeletionEffect::DeformationChanged(migration.object.clone()));
            }
        }
        let removed_ids: BTreeSet<_> = report
            .removed
            .iter()
            .map(|r| r.object.id.as_str())
            .collect();
        for migration in &report.organization_migrations {
            let destination = migration.parent_id.as_str();
            if let Some(part) = candidate.content.parts.get_mut(&migration.object.id) {
                part.parent_id = destination.into();
            } else if let Some(t) = candidate.content.transforms.get_mut(&migration.object.id) {
                t.part_id = (!destination.is_empty()).then(|| destination.to_owned().into());
            } else if let Some(m) = candidate.content.meshes.get_mut(&migration.object.id) {
                m.part_id = destination.into();
            } else if let Some(os) = candidate.content.offscreens.get_mut(&migration.object.id) {
                os.part_id = destination.into();
                if let Some(mapping) = &migration.offscreen_mapping {
                    os.part_keyform_indices = mapping.clone();
                }
            }
        }
        if !report.organization_migrations.is_empty() {
            candidate.content.hierarchy_order.organization = self.scene_organization_order();
        }
        // Converters validate geometry independently of admission. Collect the
        // exact changed records against the true source to honor every write lock.
        let mut converted = vec![];
        for id in self.mesh_order() {
            if !removed_ids.contains(id.as_str()) && self.get_mesh(id) != candidate.get_mesh(id) {
                converted.push(object(DeletionKind::Mesh, id));
            }
        }
        for id in self.transform_order() {
            if !removed_ids.contains(id.as_str())
                && self.get_transform(id) != candidate.get_transform(id)
            {
                converted.push(object(DeletionKind::Deformer, id));
            }
        }
        for id in self.binding_order() {
            if !removed_ids.contains(id.as_str())
                && self.get_binding(id) != candidate.get_binding(id)
            {
                converted.push(object(DeletionKind::MeshBinding, id));
            }
        }
        for id in self.blend_binding_order() {
            if !removed_ids.contains(id.as_str())
                && self.get_blend_binding(id) != candidate.get_blend_binding(id)
            {
                converted.push(object(DeletionKind::BlendBinding, id));
            }
        }
        for key in converted {
            if let Err(status) = self.deletion_unlocked(&key) {
                report.blockers.push(DeletionLock {
                    object: key,
                    source_id: status.message,
                });
                return Ok(false);
            }
            if !report.rewritten.iter().any(|r| r.owner == key) {
                report.rewritten.push(DeletionRelation {
                    owner: key.clone(),
                    field: DeletionField::ConvertedData,
                    target: key,
                });
            }
        }
        Ok(true)
    }

    pub(super) fn install_deletion_drawing(
        &self,
        candidate: &mut Document,
        removed: &BTreeSet<&str>,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let Some(source) = self.draw_order_groups() else {
            return Ok(());
        };
        let groups: BTreeMap<_, _> = source.iter().map(|g| (g.owner.as_str(), g)).collect();
        fn expand(
            id: &str,
            groups: &BTreeMap<&str, &DrawOrderGroup>,
            removed: &BTreeSet<&str>,
            out: &mut Vec<String>,
        ) {
            if !removed.contains(id) {
                out.push(id.into());
            } else if let Some(group) = groups.get(id) {
                for child in &group.items {
                    expand(child, groups, removed, out);
                }
            }
        }
        let mut result = vec![];
        for group in source {
            control.checkpoint()?;
            if removed.contains(group.owner.as_str()) {
                continue;
            }
            let mut next = group.clone();
            next.items.clear();
            for id in &group.items {
                expand(id, &groups, removed, &mut next.items);
            }
            if next != *group {
                self.authoring_unlocked(if group.owner.is_empty() {
                    self.id()
                } else {
                    &group.owner
                })?;
            }
            result.push(next);
        }
        candidate.content.draw_order_groups = Some(result);
        Ok(())
    }
}
