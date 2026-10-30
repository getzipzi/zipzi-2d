//! Dependency-cleaning deletion. Plans own a validated private candidate; strict
//! scene/resource deletion remains available independently.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod business;
mod dependents;
pub(super) mod index;
mod leaves;
mod migration;
mod parameters;
mod scope;

pub use zipzi_contract::document::{
    DeletionChoice, DeletionDecision, DeletionDecisionReason, DeletionKind, DeletionObject,
    DeletionOffscreenPlacement, DeletionRequest, DeletionTarget, DeletionTolerance,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletionMigration {
    pub object: DeletionObject,
    pub parent_id: String,
    pub strategy: ReparentStrategy,
    pub guarantee: Option<ReparentGuarantee>,
    pub limited_by: Vec<DeletionObject>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionOrganizationMigration {
    pub object: DeletionObject,
    pub parent_id: String,
    pub offscreen_mapping: Option<Vec<i32>>,
}

/// Slots address the frozen source only; applying a plan never reuses them on a
/// changed document. Owned records use their stable identities instead.
pub use zipzi_contract::document::{
    DeletionChannel, DeletionEffect, DeletionField, DeletionLock, DeletionReason, DeletionRelation,
    DeletionRemoval, DeletionStructuralBlocker,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeletionReport {
    pub requested: Vec<DeletionObject>,
    pub requested_scopes: Vec<DeletionTarget>,
    pub removed: Vec<DeletionRemoval>,
    pub rewritten: Vec<DeletionRelation>,
    pub relations: Vec<DeletionRelation>,
    pub blockers: Vec<DeletionLock>,
    pub structural_blockers: Vec<DeletionStructuralBlocker>,
    pub effects: Vec<DeletionEffect>,
    pub migrations: Vec<DeletionMigration>,
    pub organization_migrations: Vec<DeletionOrganizationMigration>,
    pub decisions: Vec<DeletionDecision>,
}
impl DeletionReport {
    /// Conservative report payload estimate for bounded native plan storage.
    pub fn estimated_bytes(&self) -> usize {
        serde_json::to_vec(self)
            .expect("typed report serializes")
            .len()
            .saturating_mul(4)
    }

    pub fn requires_confirmation(&self) -> bool {
        self.rewritten.iter().any(|r| {
            !(matches!(r.field, DeletionField::DrawingItem { .. })
                || (matches!(r.field, DeletionField::CdiParameter { .. })
                    && r.target.kind == DeletionKind::Parameter))
        }) || self
            .removed
            .iter()
            .any(|r| matches!(r.reason, DeletionReason::Dependency | DeletionReason::Scope))
            || self
                .removed
                .iter()
                .any(|r| r.reason == DeletionReason::Owned && !r.object.kind.is_internal())
            || self.effects.iter().any(|e| {
                matches!(
                    e,
                    DeletionEffect::BindingContributionRemoved { .. }
                        | DeletionEffect::ParameterDimensionRemoved { .. }
                        | DeletionEffect::ResourceReplaced { .. }
                        | DeletionEffect::ContentChanged(_)
                        | DeletionEffect::DrawingGroupFlattened { .. }
                )
            })
            || self.requested.len() > 1
            || !self.migrations.is_empty()
            || !self.decisions.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct DeletionPlan {
    source: Arc<()>,
    candidate: Arc<Document>,
    report: DeletionReport,
    cost: ConversionCost,
    control: ConversionControl,
}
#[derive(Debug, Clone)]
pub struct DeletionPreparation {
    pub report: DeletionReport,
    pub plan: Option<DeletionPlan>,
}
impl DeletionPlan {
    pub fn report(&self) -> &DeletionReport {
        &self.report
    }
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
    /// Read-only access for SDK/Project export preflight. The result cannot be
    /// replaced or mutated by a caller.
    pub fn result(&self) -> &Document {
        &self.candidate
    }
}

fn object(kind: DeletionKind, id: &str) -> DeletionObject {
    DeletionObject {
        kind,
        id: id.into(),
        owner_id: None,
    }
}

impl Document {
    /// Read-only native analysis using the snapshot-bound typed relation index.
    /// Full conversion and candidate construction occur only during preparation.
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

    pub fn plan_deletion(
        &self,
        request: &DeletionRequest,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<DeletionPlan, Status> {
        let prepared = self.prepare_deletion(request, limits, control)?;
        if let Some(plan) = prepared.plan {
            return Ok(plan);
        }
        if let Some(blocker) = prepared.report.blockers.first() {
            return Err(Status::error("OBJECT_LOCKED", &blocker.source_id));
        }
        if let Some(blocker) = prepared.report.structural_blockers.first() {
            return Err(Status::error(&blocker.code, &blocker.reason));
        }
        Err(Status::error(
            "DELETION_DECISION_REQUIRED",
            "Choose a reported migration resolution",
        ))
    }

    pub fn prepare_deletion(
        &self,
        request: &DeletionRequest,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<DeletionPreparation, Status> {
        if self.mutation_blocked() {
            return Err(Status::error(
                "EDIT_ACTIVE",
                "Finish the active Core edit first",
            ));
        }
        control.checkpoint()?;
        let bytes = self.estimated_content_bytes();
        // Bound analysis inventories as well as the later candidate copy.
        limits.check_conversion(0, 0, bytes)?;
        let report = self.analyze_deletion(request, control)?;
        if !report.blockers.is_empty() || !report.decisions.is_empty() {
            return Ok(DeletionPreparation { report, plan: None });
        }
        // Admit source/candidate/report/history before cloning the candidate.
        let report_bytes = serde_json::to_vec(&report)
            .expect("typed deletion report")
            .len();
        let payload = bytes
            .checked_add(report_bytes)
            .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", "Deletion plan size"))?;
        let mut cost = limits.check_conversion(0, payload, bytes)?;
        control.checkpoint()?;
        let mut candidate = self.fork_candidate();
        let mut report = report;
        if !self.install_deletion_migrations(
            &mut candidate,
            request,
            &mut report,
            limits,
            control,
            &mut cost,
        )? {
            return Ok(DeletionPreparation { report, plan: None });
        }
        if !self.install_parameter_deletion(
            &mut candidate,
            request,
            &mut report,
            limits,
            control,
            &mut cost,
        )? {
            return Ok(DeletionPreparation { report, plan: None });
        }
        if !self.install_leaf_deletion(
            &mut candidate,
            request,
            &mut report,
            limits,
            control,
            &mut cost,
        )? {
            return Ok(DeletionPreparation { report, plan: None });
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
        let drawing_ids: BTreeSet<_> = scene_ids
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
        let mut delta = organization::Delta::default();
        delta.removed = report
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
            .collect();
        delta.install(&mut candidate);
        self.install_business_deletion(&mut candidate, request, &report, control)?;
        // Merge each field's cleanup on its original ordered list, never by
        // executing source slots after another slot has shifted.
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
            control.checkpoint()?;
            if let Some(mesh) = candidate.content.meshes.get_mut(&owner.id) {
                mesh.masks.retain(|mask| !ids.contains(mask.as_str()));
                if mesh.masks.is_empty() {
                    mesh.inverted_mask = false;
                }
            } else if let Some(os) = candidate.content.offscreens.get_mut(&owner.id) {
                os.masks.retain(|mask| !ids.contains(mask.as_str()));
            }
        }
        if let Some(areas) = &mut candidate.content.model3_settings.hit_areas {
            areas.retain(|a| !ids.contains(a.id.as_str()));
        }
        if !self.install_deletion_drawing(&mut candidate, &drawing_ids, &mut report, control)? {
            return Ok(DeletionPreparation { report, plan: None });
        }
        candidate
            .content
            .hierarchy_order
            .organization
            .retain(|id| !scene_ids.contains(id.as_str()));
        candidate
            .content
            .hierarchy_order
            .deformation
            .retain(|id| !scene_ids.contains(id.as_str()));
        candidate
            .content
            .hierarchy_order
            .glues
            .retain(|id| !scene_ids.contains(id.as_str()));
        candidate.changed(
            ChangeKind::Structure,
            vec![],
            ids.iter().map(|id| (*id).to_owned()).collect(),
        );
        control.checkpoint()?;
        if !self.validate_deletion_business_locks(&candidate, &mut report, control)? {
            return Ok(DeletionPreparation { report, plan: None });
        }
        for issue in candidate.validate_structure() {
            report.structural_blockers.push(DeletionStructuralBlocker {
                object_id: issue.object_id,
                code: issue.status.code,
                reason: issue.status.message,
            });
        }
        let retained_index = candidate.deletion_index(control)?;
        for removal in &report.removed {
            if retained_index.objects.contains(&removal.object) {
                report.structural_blockers.push(DeletionStructuralBlocker {
                    object_id: removal.object.id.clone(),
                    code: "DELETION_TARGET_RETAINED".into(),
                    reason: "The final candidate still resolves a retired typed identity".into(),
                });
            }
        }
        if !report.structural_blockers.is_empty() {
            return Ok(DeletionPreparation { report, plan: None });
        }
        control.checkpoint()?;
        cost = limits.check_conversion(
            cost.sample_combinations,
            cost.payload_bytes.saturating_add(report.estimated_bytes()),
            bytes,
        )?;
        let plan = DeletionPlan {
            source: self.snapshot_identity.clone(),
            candidate: Arc::new(candidate),
            report: report.clone(),
            cost,
            control: control.clone(),
        };
        Ok(DeletionPreparation {
            report,
            plan: Some(plan),
        })
    }

    pub fn apply_deletion_plan(&mut self, plan: &DeletionPlan) -> EditResult {
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.source) {
            return self.failed(Status::error(
                "STALE_PLAN",
                "Replan deletion against the exact source",
            ));
        }
        if let Err(status) = plan.control.check() {
            return self.failed(status);
        }
        let object_ids: BTreeSet<_> = plan
            .report
            .removed
            .iter()
            .map(|r| r.object.id.clone())
            .chain(plan.report.rewritten.iter().map(|r| r.owner.id.clone()))
            .collect();
        // Native consumers must invalidate retired meshes as well as survivors.
        let mesh_ids = self.mesh_order().to_vec();
        // Publication owns all cache invalidation and advances one revision.
        if let Err(status) =
            self.publish_candidate(plan.candidate.fork_candidate(), ChangeKind::Structure, true)
        {
            return self.failed(status);
        }
        EditResult {
            status: Status::ok(),
            changes: ChangeSet {
                kind: ChangeKind::Structure,
                mesh_ids,
                revision: self.revision,
                object_ids: object_ids.into_iter().collect(),
            },
            referrers: vec![],
        }
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
