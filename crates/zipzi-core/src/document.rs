//! Shared authoring values and validation helpers. Persistent ownership lives in state.
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Component};
pub use zipzi_contract::document::*;
pub use zipzi_contract::physics_document::*;
mod validation;
pub use validation::*;
mod parameters;
pub use parameters::*;
mod motions;
pub use motions::*;
mod reference_entry;
pub use reference_entry::*;
pub(crate) mod physics;
pub use physics::model::*;
mod model3;
pub use crate::model_validation::conversion::report::{
    ConversionAddress, ConversionElement, ConversionLocation, ConversionOwnerDomain,
    ConversionReconstruction, ConversionRemapping, ConversionReport, ParameterRangeConversion,
};
pub use crate::model_validation::organization::OrganizationEffects;
pub use model3::*;
pub const DEFAULT_ROTATION_BONE_LENGTH: f32 = 128.;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureIssue {
    pub object_id: String,
    pub status: Status,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingIdentityKind {
    Axis,
    Sample,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingIdentity {
    pub kind: BindingIdentityKind,
    pub id: String,
    pub owner: String,
    pub axis: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindingOmission {
    pub object_id: String,
    pub code: String,
    pub dependency: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReparentSupport {
    Available,
    RequiresConversionPlan,
    NoChange,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReparentGuarantee {
    pub strategy: ReparentStrategy,
    pub position_scope: String,
    pub tolerance: f64,
    pub relative_tolerance: f64,
    pub reference_scale: f64,
    pub context: Option<ReparentContext>,
    pub measured_error: f64,
    pub error_bound: f64,
    /// Channels outside the position proof must be presented to the caller.
    pub effects: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneUse {
    Geometry,
    Drawing,
    Glue,
    GluePair,
    Composite,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SceneDisabledReason {
    UnspecifiedReference,
    UnresolvedReference { runtime_id: String },
    EmptyGeometry,
    MissingImageContent { asset_id: String },
    DependencyUnavailable { object_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisabledSceneItem {
    pub id: String,
    pub usage: SceneUse,
    pub reason: SceneDisabledReason,
    pub chain: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SceneProjection {
    pub geometry: Vec<String>,
    pub drawing: Vec<String>,
    pub glues: Vec<String>,
    pub pairs: Vec<String>,
    /// Stable PairId -> original, uncompressed evaluation coordinates.
    pub pair_slots: HashMap<String, ScenePairSlot>,
    pub composites: Vec<String>,
    pub geometry_slots: HashMap<String, usize>,
    pub drawing_slots: HashMap<String, usize>,
    pub glue_slots: HashMap<String, usize>,
    pub composite_slots: HashMap<String, usize>,
    pub disabled: Vec<DisabledSceneItem>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenePairSlot {
    pub glue_id: String,
    pub source_slot: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    ModelParameterGroup,
    ModelParameterMember,
    ModelHitArea,
    CdiCombinedSet,
    CdiCombinedMember,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceIdentity {
    pub id: String,
    pub kind: ReferenceKind,
    pub owner: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReferenceEffects {
    pub metadata_only: bool,
    pub behavior_groups_changed: bool,
    pub hit_areas_changed: bool,
    pub combined_sets_changed: bool,
    pub drawing_changed: bool,
    pub deformation_changed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReferenceDisabledReason {
    Unspecified,
    Unresolved { runtime_id: String },
    EmptyGeometry { mesh_id: String },
    EmptyMembers,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisabledReferenceItem {
    pub id: String,
    pub owner: Option<String>,
    pub reason: ReferenceDisabledReason,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedParameterReferences {
    pub name: Option<String>,
    pub id: String,
    pub parameters: Vec<String>,
    pub members: Vec<String>,
    pub member_slots: HashMap<String, usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedHitArea {
    pub name: String,
    pub id: String,
    pub mesh_id: String,
    pub source_slot: usize,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceProjection {
    pub group_slots: HashMap<String, usize>,
    pub combined_set_slots: HashMap<String, usize>,
    pub groups: Vec<ProjectedParameterReferences>,
    pub combined_sets: Vec<ProjectedParameterReferences>,
    pub hit_areas: Vec<ProjectedHitArea>,
    pub hit_area_slots: HashMap<String, usize>,
    pub disabled: Vec<DisabledReferenceItem>,
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

pub fn valid_attachment_path(path: &str) -> bool {
    !path.is_empty()
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !path.contains('\\')
        && !path.contains('\0')
        && std::path::Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}
