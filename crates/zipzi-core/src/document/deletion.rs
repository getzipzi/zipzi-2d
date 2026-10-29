//! Dependency-cleaning deletion. Plans own a validated private candidate; strict
//! scene/resource deletion remains available independently.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod dependents;
mod migration;
mod scope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DeletionKind {
    Mesh,
    Glue,
    Offscreen,
    MeshBinding,
    BlendBinding,
    Axis,
    Sample,
    GluePair,
    HitArea,
    DrawingGroup,
    Part,
    Deformer,
    SceneBinding,
    Motion,
    MotionTrack,
    MotionKey,
    Expression,
    ExpressionTrack,
    ExpressionKey,
    PoseEntry,
    PoseLink,
    CdiPart,
    DisplayInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeletionObject {
    pub kind: DeletionKind,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
}

/// Only these root types are implemented. Other domains retain their strict APIs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DeletionTarget {
    Mesh(String),
    Glue(String),
    Offscreen(String),
    Part(String),
    Deformer(String),
    DissolvePart(String),
    RemoveDeformer(String),
}
impl DeletionTarget {
    pub fn object(&self) -> DeletionObject {
        let (kind, id) = match self {
            Self::Mesh(id) => (DeletionKind::Mesh, id),
            Self::Glue(id) => (DeletionKind::Glue, id),
            Self::Offscreen(id) => (DeletionKind::Offscreen, id),
            Self::Part(id) | Self::DissolvePart(id) => (DeletionKind::Part, id),
            Self::Deformer(id) | Self::RemoveDeformer(id) => (DeletionKind::Deformer, id),
        };
        DeletionObject {
            kind,
            id: id.clone(),
            owner_id: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeletionRequest {
    pub roots: Vec<DeletionTarget>,
    #[serde(default)]
    pub reparent: BTreeMap<String, ReparentStrategy>,
    #[serde(default)]
    pub conversion_context: Option<ReparentContext>,
    #[serde(default)]
    pub offscreen_placements: BTreeMap<String, DeletionOffscreenPlacement>,
    #[serde(default)]
    pub tolerance: DeletionTolerance,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeletionTolerance {
    pub absolute: f64,
    pub relative: f64,
}
impl Default for DeletionTolerance {
    fn default() -> Self {
        Self {
            absolute: 1e-3,
            relative: 1e-6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionOffscreenPlacement {
    pub part_id: String,
    /// Exact mapping in the destination Part's keyform order, if rebinding is required.
    pub mapping: Option<Vec<i32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletionChoice {
    PreserveEditState,
    KeepLocalData,
    ChoosePart,
    SupplyOffscreenMapping,
    DeleteSubtree,
    Cancel,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletionDecisionReason {
    ConversionUnproven { code: String },
    OffscreenOwnerUnavailable,
    OffscreenMappingRequired,
    PartStateUnpreserved,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionDecision {
    pub object: DeletionObject,
    pub reason: DeletionDecisionReason,
    pub choices: Vec<DeletionChoice>,
}
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletionField {
    Owned,
    MeshMask { slot: usize },
    MeshInvertedMask,
    OffscreenMask { slot: usize },
    GlueMeshA,
    GlueMeshB,
    HitAreaMesh,
    DrawingItem { slot: usize },
    OrganizationParent,
    DeformationParent,
    ConvertedData,
    PosePart,
    PoseLinkPart,
    MotionPart,
    ExpressionSourcePart,
    CdiPart { slot: usize },
    RequestedScope,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionRelation {
    pub owner: DeletionObject,
    pub field: DeletionField,
    pub target: DeletionObject,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletionReason {
    Requested,
    Owned,
    Dependency,
    Scope,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionRemoval {
    pub object: DeletionObject,
    pub reason: DeletionReason,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionLock {
    pub object: DeletionObject,
    pub source_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionStructuralBlocker {
    pub object_id: String,
    pub code: String,
    pub reason: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletionChannel {
    Appearance,
    Enabled,
    GluePostprocessing,
    DrawingOrder,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletionEffect {
    MaskChanged(DeletionObject),
    GlueRemoved(DeletionObject),
    HitAreaRemoved(DeletionObject),
    CompositionChanged(DeletionObject),
    PartAnimationRemoved(DeletionObject),
    DrawingGroupFlattened {
        group: DeletionObject,
        destination: DeletionObject,
        source_range: [i32; 2],
        destination_range: [i32; 2],
        dynamic_order: bool,
        offscreen: Option<DeletionObject>,
    },
    OrganizationChanged(DeletionObject),
    DeformationChanged(DeletionObject),
    UnpreservedChannels {
        object: DeletionObject,
        channels: Vec<DeletionChannel>,
    },
    PartStateRemoved {
        part: DeletionObject,
        enabled: bool,
        animated_order: bool,
    },
}
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
    pub fn requires_confirmation(&self) -> bool {
        !self.rewritten.is_empty()
            || self
                .removed
                .iter()
                .any(|r| matches!(r.reason, DeletionReason::Dependency | DeletionReason::Scope))
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
    /// Read-only native analysis. Currently a cancellable full scan;
    /// hosts must not call it from per-frame command-state queries.
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
        // Endpoints delete the connection, never its other endpoint. Inspect the
        // original source once so root order and repeated dependency paths agree.
        for id in self.glue_order() {
            control.checkpoint()?;
            let glue = self.get_glue(id).unwrap();
            for (endpoint, field) in [
                (&glue.mesh_a_id, DeletionField::GlueMeshA),
                (&glue.mesh_b_id, DeletionField::GlueMeshB),
            ] {
                let Some(mesh) = endpoint.resolved_id() else {
                    continue;
                };
                let target = object(DeletionKind::Mesh, mesh);
                if removed.contains_key(&target) {
                    let owner = object(DeletionKind::Glue, id);
                    removed
                        .entry(owner.clone())
                        .or_insert(DeletionReason::Dependency);
                    report.relations.push(DeletionRelation {
                        owner,
                        field,
                        target,
                    });
                }
            }
        }
        for id in self.binding_order() {
            control.checkpoint()?;
            let b = self.get_binding(id).unwrap();
            let owner = object(DeletionKind::Mesh, &b.mesh_id);
            if removed.contains_key(&owner) {
                add_owned(
                    &mut removed,
                    &mut report,
                    owner,
                    object(DeletionKind::MeshBinding, id),
                );
            }
        }
        for id in self.scene_binding_order() {
            control.checkpoint()?;
            let b = self.get_scene_binding(id).unwrap();
            if let Some(owner) = [DeletionKind::Part, DeletionKind::Deformer]
                .into_iter()
                .map(|kind| object(kind, b.target_id()))
                .find(|key| removed.contains_key(key))
            {
                add_owned(
                    &mut removed,
                    &mut report,
                    owner,
                    object(DeletionKind::SceneBinding, id),
                );
            }
        }
        for id in self.blend_binding_order() {
            control.checkpoint()?;
            let b = self.get_blend_binding(id).unwrap();
            let owner = [
                DeletionKind::Mesh,
                DeletionKind::Glue,
                DeletionKind::Offscreen,
                DeletionKind::Part,
                DeletionKind::Deformer,
            ]
            .into_iter()
            .map(|kind| object(kind, &b.target_id))
            .find(|key| removed.contains_key(key));
            if let Some(owner) = owner {
                add_owned(
                    &mut removed,
                    &mut report,
                    owner,
                    object(DeletionKind::BlendBinding, id),
                );
            }
        }
        let owners: Vec<_> = removed.keys().cloned().collect();
        for owner in owners {
            control.checkpoint()?;
            let axes = self
                .get_binding(&owner.id)
                .map(|b| &b.axes)
                .or_else(|| self.get_scene_binding(&owner.id).map(|b| &b.axes))
                .or_else(|| {
                    self.get_glue(&owner.id)
                        .and_then(|g| g.binding.as_ref())
                        .map(|b| &b.axes)
                });
            if let Some(axes) = axes {
                for axis in axes {
                    let key = object(DeletionKind::Axis, &axis.id);
                    add_owned(&mut removed, &mut report, owner.clone(), key.clone());
                    for sample in &axis.samples {
                        control.checkpoint()?;
                        add_owned(
                            &mut removed,
                            &mut report,
                            key.clone(),
                            object(DeletionKind::Sample, &sample.id),
                        );
                    }
                }
            }
            if let Some(glue) = self.get_glue(&owner.id) {
                for pair in &glue.pairs {
                    control.checkpoint()?;
                    add_owned(
                        &mut removed,
                        &mut report,
                        owner.clone(),
                        object(DeletionKind::GluePair, &pair.id),
                    );
                }
            }
        }
        for id in self.mesh_order() {
            control.checkpoint()?;
            let owner = object(DeletionKind::Mesh, id);
            if removed.contains_key(&owner) {
                continue;
            }
            let mesh = self.get_mesh(id).unwrap();
            for (slot, mask) in mesh.masks.iter().enumerate() {
                control.checkpoint()?;
                let target = object(DeletionKind::Mesh, mask);
                if removed.contains_key(&target) {
                    report.rewritten.push(DeletionRelation {
                        owner: owner.clone(),
                        field: DeletionField::MeshMask { slot },
                        target,
                    });
                }
            }
            if mesh.inverted_mask
                && !mesh.masks.is_empty()
                && mesh
                    .masks
                    .iter()
                    .all(|mask| removed.contains_key(&object(DeletionKind::Mesh, mask)))
            {
                report.rewritten.push(DeletionRelation {
                    owner,
                    field: DeletionField::MeshInvertedMask,
                    target: object(DeletionKind::Mesh, &mesh.masks[0]),
                });
            }
        }
        for id in self.offscreen_order() {
            control.checkpoint()?;
            let owner = object(DeletionKind::Offscreen, id);
            if removed.contains_key(&owner) {
                continue;
            }
            for (slot, mask) in self.get_offscreen(id).unwrap().masks.iter().enumerate() {
                control.checkpoint()?;
                let target = object(DeletionKind::Mesh, mask);
                if removed.contains_key(&target) {
                    report.rewritten.push(DeletionRelation {
                        owner: owner.clone(),
                        field: DeletionField::OffscreenMask { slot },
                        target,
                    });
                }
            }
        }
        for area in self.model3_settings().hit_areas.iter().flatten() {
            control.checkpoint()?;
            let Some(mesh) = area.mesh.object_id() else {
                continue;
            };
            let target = object(DeletionKind::Mesh, mesh);
            if removed.contains_key(&target) {
                let owner = object(DeletionKind::HitArea, &area.id);
                removed.insert(owner.clone(), DeletionReason::Dependency);
                report.relations.push(DeletionRelation {
                    owner,
                    field: DeletionField::HitAreaMesh,
                    target,
                });
            }
        }
        for group in self.draw_order_groups().into_iter().flatten() {
            control.checkpoint()?;
            for (slot, id) in group.items.iter().enumerate() {
                control.checkpoint()?;
                if let Some(target) = [
                    DeletionKind::Mesh,
                    DeletionKind::Offscreen,
                    DeletionKind::Part,
                ]
                .into_iter()
                .map(|kind| object(kind, id))
                .find(|key| removed.contains_key(key))
                {
                    report.rewritten.push(DeletionRelation {
                        owner: object(
                            DeletionKind::DrawingGroup,
                            if group.owner.is_empty() {
                                self.id()
                            } else {
                                &group.owner
                            },
                        ),
                        field: DeletionField::DrawingItem { slot },
                        target,
                    });
                }
            }
        }
        for group in self.draw_order_groups().into_iter().flatten() {
            if removed.contains_key(&object(DeletionKind::Part, &group.owner)) {
                add_owned(
                    &mut removed,
                    &mut report,
                    object(DeletionKind::Part, &group.owner),
                    object(DeletionKind::DrawingGroup, &group.owner),
                );
            }
        }
        self.deletion_dependents(&mut removed, &mut report, control)?;
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
        for key in writes {
            control.checkpoint()?;
            if let Err(status) = self.deletion_unlocked(&key) {
                report.blockers.push(DeletionLock {
                    object: key,
                    source_id: status.message,
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
        let ids: BTreeSet<_> = report
            .removed
            .iter()
            .filter(|r| r.object.owner_id.is_none())
            .map(|r| r.object.id.as_str())
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
        self.install_deletion_dependents(&mut candidate, &ids, control)?;
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
        self.install_deletion_drawing(&mut candidate, &ids, control)?;
        candidate
            .content
            .hierarchy_order
            .organization
            .retain(|id| !ids.contains(id.as_str()));
        candidate
            .content
            .hierarchy_order
            .deformation
            .retain(|id| !ids.contains(id.as_str()));
        candidate
            .content
            .hierarchy_order
            .glues
            .retain(|id| !ids.contains(id.as_str()));
        candidate.changed(
            ChangeKind::Structure,
            vec![],
            ids.iter().map(|id| (*id).to_owned()).collect(),
        );
        control.checkpoint()?;
        for issue in candidate.validate_structure() {
            report.structural_blockers.push(DeletionStructuralBlocker {
                object_id: issue.object_id,
                code: issue.status.code,
                reason: issue.status.message,
            });
        }
        if !report.structural_blockers.is_empty() {
            return Ok(DeletionPreparation { report, plan: None });
        }
        control.checkpoint()?;
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
    removed
        .entry(child.clone())
        .or_insert(DeletionReason::Owned);
    report.relations.push(DeletionRelation {
        owner,
        field: DeletionField::Owned,
        target: child,
    });
}
