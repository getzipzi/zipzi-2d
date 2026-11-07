//! Dependency analysis and atomic Builder deletion with natural tracked writes.
use super::*;
use crate::draw_order::DrawOrderGroup;
use crate::model_validation::organization;
use crate::model_validation::Model;
mod business;
mod dependents;
pub(crate) mod index;
mod leaves;
mod migration;
mod parameters;
mod scope;
fn object(kind: DeletionKind, id: &str) -> DeletionObject {
    DeletionObject {
        kind,
        id: id.into(),
        owner_id: None,
    }
}
impl Model<'_> {
    pub fn analyze_deletion(
        &self,
        request: &DeletionRequest,
        control: &ConversionControl,
    ) -> Result<DeletionReport, Status> {
        control.checkpoint()?;
        if request.roots.is_empty() {
            return Err(Status::error("INVALID_SELECTION", "Select deletion roots"));
        }
        let (mut removed, mut report) = self.deletion_scope(request, control)?;
        self.indexed_deletion_dependencies(request, &mut removed, &mut report, control)?;
        self.resolve_motion_registration_deletion(&mut removed, &mut report, control)?;
        self.analyze_parameter_deletion(request, &removed, &mut report, control)?;
        self.analyze_business_deletion(request, &removed, &mut report, control)?;
        self.deletion_migrations(request, &removed, &mut report, control)?;
        report.removed = removed
            .into_iter()
            .map(|(object, reason)| DeletionRemoval { object, reason })
            .collect();
        let writes: BTreeSet<_> = report
            .removed
            .iter()
            .map(|r| r.object.clone())
            .chain(report.rewritten.iter().map(|r| r.owner.clone()))
            .collect();
        let mut lock_checks = BTreeMap::new();
        for key in writes {
            control.checkpoint()?;
            // Whole binding writes protect every owned sample; reuse that check
            // rather than scanning the entire axis grid once for every sample.
            let check = if matches!(key.kind, DeletionKind::Axis | DeletionKind::Sample) {
                self.binding_identity(&key.id)
                    .map(|i| {
                        object(
                            if self.get_binding(&i.owner).is_some() {
                                DeletionKind::MeshBinding
                            } else if self.get_scene_binding(&i.owner).is_some() {
                                DeletionKind::SceneBinding
                            } else if self.get_blend_key_table(&i.owner).is_some() {
                                DeletionKind::BlendKeyTable
                            } else if self.get_blend_constraint(&i.owner).is_some() {
                                DeletionKind::BlendConstraint
                            } else {
                                DeletionKind::GlueBinding
                            },
                            &i.owner,
                        )
                    })
                    .unwrap_or_else(|| key.clone())
            } else {
                key.clone()
            };
            if let Err(status) = lock_checks
                .entry(check.clone())
                .or_insert_with(|| self.deletion_unlocked(&check))
            {
                report.blockers.push(DeletionLock {
                    object: key,
                    source_id: status.message.clone(),
                });
            }
        }
        let mask_owners: BTreeSet<_> = report
            .rewritten
            .iter()
            .filter(|r| {
                matches!(
                    r.field,
                    DeletionField::MeshMask { .. } | DeletionField::OffscreenMask { .. }
                )
            })
            .map(|r| r.owner.clone())
            .collect();
        report
            .effects
            .extend(mask_owners.into_iter().map(DeletionEffect::MaskChanged));
        for removal in &report.removed {
            let effect = match removal.object.kind {
                DeletionKind::Glue => Some(DeletionEffect::GlueRemoved(removal.object.clone())),
                DeletionKind::HitArea => {
                    Some(DeletionEffect::HitAreaRemoved(removal.object.clone()))
                }
                DeletionKind::Offscreen => {
                    Some(DeletionEffect::CompositionChanged(removal.object.clone()))
                }
                _ => None,
            };
            report.effects.extend(effect);
        }
        Ok(report)
    }
}
fn add_owned(
    removed: &mut BTreeMap<DeletionObject, DeletionReason>,
    report: &mut DeletionReport,
    owner: DeletionObject,
    child: DeletionObject,
) {
    let reason = removed
        .entry(child.clone())
        .or_insert(DeletionReason::Owned);
    if *reason == DeletionReason::Dependency {
        *reason = DeletionReason::Owned;
    }
    report.relations.push(DeletionRelation {
        owner,
        field: DeletionField::Owned,
        target: child,
    });
}

#[derive(Debug, Clone)]
pub struct DeletionOutcome {
    pub report: DeletionReport,
    pub work: ConversionWork,
}
fn ready(report: &DeletionReport) -> Result<(), Status> {
    if let Some(b) = report.blockers.first() {
        return Err(Status::error("OBJECT_LOCKED", &b.source_id));
    }
    if let Some(b) = report.structural_blockers.first() {
        return Err(Status::error(&b.code, &b.reason));
    }
    if !report.decisions.is_empty() {
        return Err(Status::error(
            "DELETION_DECISION_REQUIRED",
            "Choose a reported migration resolution",
        ));
    }
    Ok(())
}
impl DocumentState {
    pub fn analyze_deletion(
        &self,
        request: &DeletionRequest,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<DeletionReport, Status> {
        let model = self.content.model();
        index::DeletionIndex::admission_bytes(&model, control, limits.max_workspace_bytes / 2)?;
        model.analyze_deletion(request, control)
    }
}
impl DocumentBuilder {
    /// Prepare conversion-dependent review on a private candidate. A blocked
    /// review returns its full decisions without installing partial migrations.
    pub fn review_deletion(
        &mut self,
        request: &DeletionRequest,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<DeletionOutcome, Status> {
        let mut candidate = self.clone();
        let outcome = candidate.prepare_deletion(request, limits, control)?;
        if ready(&outcome.report).is_ok() {
            *self = candidate;
        }
        Ok(outcome)
    }
    pub fn edit_deletion(
        &mut self,
        request: &DeletionRequest,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<DeletionOutcome, Status> {
        let mut result = None;
        self.write(|this| {
            let outcome = this.review_deletion(request, limits, control)?;
            ready(&outcome.report)?;
            result = Some(outcome);
            Ok(())
        })?;
        Ok(result.unwrap())
    }
    fn prepare_deletion(
        &mut self,
        request: &DeletionRequest,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<DeletionOutcome, Status> {
        let mut outcome = None;
        self.write(|candidate| {
            // A source root retains immutable record blocks while this Builder writes;
            // it contains no saved baseline, session, history or publication machinery.
            let source = candidate.content.clone();
            let model = source.model();
            let index_bytes = index::DeletionIndex::admission_bytes(
                &model,
                control,
                limits.max_workspace_bytes / 2,
            )?;
            let mut report = model.analyze_deletion(request, control)?;
            if ready(&report).is_err() {
                outcome = Some(DeletionOutcome {
                    report,
                    work: Default::default(),
                });
                return Ok(());
            }
            let payload = super::cost::deletion_payload(&model, &report);
            let mut cost = limits.check_conversion(0, payload.saturating_add(index_bytes))?;
            if !model.install_deletion_migrations(
                candidate,
                request,
                &mut report,
                limits,
                control,
                &mut cost,
            )? {
                if ready(&report).is_err() {
                    outcome = Some(DeletionOutcome { report, work: cost });
                    return Ok(());
                }
            }
            if !model.install_parameter_deletion(
                candidate,
                request,
                &mut report,
                limits,
                control,
                &mut cost,
            )? {
                if ready(&report).is_err() {
                    outcome = Some(DeletionOutcome { report, work: cost });
                    return Ok(());
                }
            }
            if !model.install_leaf_deletion(
                candidate,
                request,
                &mut report,
                limits,
                control,
                &mut cost,
            )? {
                if ready(&report).is_err() {
                    outcome = Some(DeletionOutcome { report, work: cost });
                    return Ok(());
                }
            }
            let ids: BTreeSet<_> = report
                .removed
                .iter()
                .filter(|r| r.object.owner_id.is_none())
                .map(|r| r.object.id.clone())
                .collect();
            let scene_ids: BTreeSet<_> = report
                .removed
                .iter()
                .filter(|r| {
                    matches!(
                        r.object.kind,
                        DeletionKind::Mesh
                            | DeletionKind::Glue
                            | DeletionKind::Offscreen
                            | DeletionKind::Part
                            | DeletionKind::Deformer
                    )
                })
                .map(|r| r.object.id.clone())
                .collect();
            let drawing_ids = scene_ids
                .iter()
                .cloned()
                .chain(
                    report
                        .removed
                        .iter()
                        .filter(|r| r.object.kind == DeletionKind::DrawingGroup)
                        .map(|r| r.object.id.clone()),
                )
                .collect();
            let delta = organization::Delta {
                removed: report
                    .removed
                    .iter()
                    .filter(|r| {
                        matches!(
                            r.object.kind,
                            DeletionKind::Mesh
                                | DeletionKind::Glue
                                | DeletionKind::Offscreen
                                | DeletionKind::Part
                                | DeletionKind::Deformer
                                | DeletionKind::MeshBinding
                                | DeletionKind::SceneBinding
                                | DeletionKind::BlendBinding
                        )
                    })
                    .map(|r| r.object.id.clone())
                    .collect(),
                ..Default::default()
            };
            candidate.install_scene_delta(delta)?;
            model.install_business_deletion(candidate, request, &report, control)?;
            let mask_owners: BTreeSet<_> = report
                .rewritten
                .iter()
                .filter(|r| {
                    matches!(
                        r.field,
                        DeletionField::MeshMask { .. } | DeletionField::OffscreenMask { .. }
                    )
                })
                .map(|r| r.owner.clone())
                .collect();
            for owner in mask_owners {
                control.check()?;
                if let Some(mesh) = candidate.meshes_mut(&owner.id) {
                    mesh.masks.retain(|mask| !ids.contains(mask.as_str()));
                    if mesh.masks.is_empty() {
                        mesh.inverted_mask = false;
                    }
                } else if let Some(os) = candidate.offscreens_mut(&owner.id) {
                    os.masks.retain(|mask| !ids.contains(mask.as_str()));
                }
            }
            if report
                .removed
                .iter()
                .any(|r| r.object.kind == DeletionKind::HitArea)
            {
                if let Some(areas) = &mut candidate.model3_settings_mut().hit_areas {
                    areas.retain(|a| !ids.contains(a.id.as_str()));
                }
            }
            model.install_deletion_drawing(candidate, &drawing_ids, &mut report, control)?;
            if !scene_ids.is_empty() {
                let order = candidate.hierarchy_order_mut();
                order.organization.retain(|id| !scene_ids.contains(id));
                order.deformation.retain(|id| !scene_ids.contains(id));
                order.glues.retain(|id| !scene_ids.contains(id));
            }
            model.validate_deletion_business_locks(
                &candidate.content.model(),
                &mut report,
                control,
            )?;
            if ready(&report).is_err() {
                outcome = Some(DeletionOutcome { report, work: cost });
                return Ok(());
            }
            candidate.validate_candidate()?;
            let retained = candidate.content.model().deletion_index(control)?;
            for removal in &report.removed {
                if retained.objects.contains(&removal.object) {
                    return Err(Status::error(
                        "DELETION_TARGET_RETAINED",
                        &removal.object.id,
                    ));
                }
            }
            control.check()?;
            outcome = Some(DeletionOutcome { report, work: cost });
            Ok(())
        })?;
        Ok(outcome.unwrap())
    }
}
