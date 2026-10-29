//! Dependency-cleaning deletion. Plans own a validated private candidate; strict
//! scene/resource deletion remains available independently.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod dependents;
pub(super) mod index;
mod migration;
mod scope;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionObject {
    /// Native identity domain; Part and its DrawingGroup role remain distinct.
    pub kind: DeletionKind,
    /// Persistent native ID; embedded identities also require their owner namespace.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional owning Expression ID for embedded source-scene track/key identities.
    pub owner_id: Option<String>,
}

/// Only these root types are implemented. Other domains retain their strict APIs.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionTarget {
    Mesh(
        /// Delete this Mesh and its owned identities; shared resources remain.
        String,
    ),
    Glue(
        /// Delete this Glue connection, pairs and owned bindings; endpoint meshes remain.
        String,
    ),
    Offscreen(
        /// Delete this Offscreen composite and its own shapes/bindings; its Part remains.
        String,
    ),
    Part(
        /// Delete this Part organization subtree; external deformation members remain and migrate.
        String,
    ),
    Deformer(
        /// Delete this deformation subtree, including meshes across organization Parts.
        String,
    ),
    DissolvePart(
        /// Remove this Part while retaining and migrating its organization members.
        String,
    ),
    RemoveDeformer(
        /// Remove only this Deformer and retain/migrate its deformation children.
        String,
    ),
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

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionRequest {
    /// Explicit typed roots and individual scope choices; order does not change the result.
    pub roots: Vec<DeletionTarget>,
    #[serde(default)]
    /// Explicit migration strategy by surviving object ID; KeepLocalData on a dissolved Part accepts its state loss.
    pub reparent: BTreeMap<String, ReparentStrategy>,
    #[serde(default)]
    /// Frozen parameter inputs, finite domains and reference time; None uses document defaults and full declared domains.
    pub conversion_context: Option<ReparentContext>,
    #[serde(default)]
    /// Explicit retained Part owners and mapping resolutions for surviving Offscreens.
    pub offscreen_placements: BTreeMap<String, DeletionOffscreenPlacement>,
    #[serde(default)]
    /// Absolute and geometry-scale-relative position error allowed by conversion proofs.
    pub tolerance: DeletionTolerance,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionTolerance {
    /// Finite positive absolute error tolerance in model units.
    pub absolute: f64,
    /// Finite nonnegative tolerance times the original geometry scale.
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionOffscreenPlacement {
    /// Retained destination Part UUID; it must have an available Offscreen ownership role.
    pub part_id: String,
    /// Exact mapping in the destination Part's keyform order, if rebinding is required.
    pub mapping: Option<Vec<i32>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionChoice {
    PreserveEditState,
    KeepLocalData,
    ChoosePart,
    SupplyOffscreenMapping,
    DeleteSubtree,
    Cancel,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionDecisionReason {
    ConversionUnproven {
        /// Native conversion diagnostic code; available resolutions come from typed choices.
        code: String,
    },
    OffscreenOwnerUnavailable,
    OffscreenMappingRequired,
    PartStateUnpreserved,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionDecision {
    /// Affected surviving object that needs an explicit resolution.
    pub object: DeletionObject,
    /// Typed unresolved native condition; it is not an instruction to expand deletion.
    pub reason: DeletionDecisionReason,
    /// Actually available alternatives; selecting one requires a new source-bound plan.
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
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionField {
    Owned,
    MeshMask {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    MeshInvertedMask,
    OffscreenMask {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    GlueMeshA,
    GlueMeshB,
    HitAreaMesh,
    DrawingItem {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    OrganizationParent,
    DeformationParent,
    ConvertedData,
    PosePart,
    PoseLinkPart,
    MotionPart,
    ExpressionSourcePart,
    CdiPart {
        /// Zero-based address in the frozen source list, never a persistent object ID.
        slot: usize,
    },
    RequestedScope,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionRelation {
    /// Native object or role containing this field in the frozen source.
    pub owner: DeletionObject,
    /// Typed field/slot address valid only in the frozen source.
    pub field: DeletionField,
    /// Native target identity explaining this ownership, dependency or rewrite.
    pub target: DeletionObject,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionReason {
    Requested,
    Owned,
    Dependency,
    Scope,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionRemoval {
    /// Retired identity, including its native domain and embedded namespace.
    pub object: DeletionObject,
    /// Whether this is an explicit root, scoped descendant, owned identity or dependency record.
    pub reason: DeletionReason,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionLock {
    /// Actual removed or rewritten object protected by the lock.
    pub object: DeletionObject,
    /// Persistent ID of the direct or inherited lock source to locate and unlock.
    pub source_id: String,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct DeletionStructuralBlocker {
    /// Native result identity/address associated with the structural failure.
    pub object_id: String,
    /// Native structure-validation code; the candidate cannot be installed.
    pub code: String,
    /// Display diagnostic describing the invalid result; never parsed to decide execution.
    pub reason: String,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionChannel {
    Appearance,
    Enabled,
    GluePostprocessing,
    DrawingOrder,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum DeletionEffect {
    MaskChanged(
        /// Retained Mask consumer whose clipping effect changes.
        DeletionObject,
    ),
    GlueRemoved(
        /// Deleted connection whose seam or postprocessing contribution disappears.
        DeletionObject,
    ),
    HitAreaRemoved(
        /// Deleted model3 Hit Area registration.
        DeletionObject,
    ),
    CompositionChanged(
        /// Affected Offscreen or Part whose compositing can change.
        DeletionObject,
    ),
    PartAnimationRemoved(
        /// Removed Part animation/Pose record; its container remains.
        DeletionObject,
    ),
    DrawingGroupFlattened {
        /// Removed drawing role being flattened.
        group: DeletionObject,
        /// Nearest retained drawing role receiving expanded members.
        destination: DeletionObject,
        /// Original inclusive group order bounds.
        source_range: [i32; 2],
        /// Retained destination order bounds; clipping and relative order may differ.
        destination_range: [i32; 2],
        /// Whether parameter or Blend bindings drove the removed group order.
        dynamic_order: bool,
        /// Original composite whose group scope is lost or migrated.
        offscreen: Option<DeletionObject>,
    },
    OrganizationChanged(
        /// Surviving member whose organization owner changes.
        DeletionObject,
    ),
    DeformationChanged(
        /// Surviving object whose deformation is not fully preserved.
        DeletionObject,
    ),
    UnpreservedChannels {
        /// Surviving object affected by channels outside the position proof.
        object: DeletionObject,
        /// Explicit channels not covered by the converter position guarantee.
        channels: Vec<DeletionChannel>,
    },
    PartStateRemoved {
        /// Dissolved Part whose inherited state is removed.
        part: DeletionObject,
        /// Original Part enabled state, before dissolution.
        enabled: bool,
        /// Whether the removed Part had parameter-driven drawing order.
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
    /// Conservative report payload estimate for bounded native plan storage.
    pub fn estimated_bytes(&self) -> usize {
        serde_json::to_vec(self)
            .expect("typed report serializes")
            .len()
            .saturating_mul(4)
    }

    pub fn requires_confirmation(&self) -> bool {
        self.rewritten
            .iter()
            .any(|r| !matches!(r.field, DeletionField::DrawingItem { .. }))
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
        self.indexed_deletion_dependencies(&mut removed, &mut report, control)?;
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
                            } else {
                                DeletionKind::Glue
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
