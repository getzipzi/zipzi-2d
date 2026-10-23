//! Scene lifecycle and collections. Organization, deformation, drawing and
//! connection management have independent, explicit commands.
use super::organization::lifecycle::{copy_axes, mapped, new_id, unique};
use super::organization::Delta;
use super::*;
use crate::{
    Appearance, BlendMode, RotationTransform, SceneReference, TransformData, Vec2, WarpTransform,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
mod projection;
pub use projection::*;
mod drawing;
mod fields;
mod lifecycle;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneCollection {
    Organization { part_id: Option<String> },
    Deformation { parent_id: Option<String> },
    Glues,
    DrawingGroups,
    Drawing { owner: String },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneCopyScope {
    #[default]
    Selected,
    DeformationSubtree,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneCreate {
    Mesh {
        #[serde(default)]
        texture: SceneReference,
    },
    Warp {
        #[serde(default)]
        bounds: Option<[f32; 4]>,
    },
    Rotation,
    Offscreen,
    Glue {
        #[serde(default)]
        a: SceneReference,
        #[serde(default)]
        b: SceneReference,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneField {
    Label,
    RuntimeId,
    Enabled,
    Appearance,
    Texture,
    Masks,
    MeshDrawing,
    Warp,
    Rotation,
    GlueEndpoints,
    GlueIntensity,
    Offscreen,
    Context,
}
impl SceneField {
    pub fn is_reference(self) -> bool {
        matches!(
            self,
            Self::Texture | Self::Masks | Self::GlueEndpoints | Self::Context
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "field",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SceneFieldValue {
    Label(String),
    RuntimeId(String),
    Enabled(bool),
    Appearance(Appearance),
    Texture(SceneReference),
    Masks(Vec<String>),
    MeshDrawing {
        draw_order: Option<f32>,
        blend_mode: BlendMode,
        raw_blend_mode: Option<u32>,
        double_sided: bool,
        inverted_mask: bool,
    },
    Warp(WarpTransform),
    Rotation(RotationTransform),
    GlueEndpoints {
        a: SceneReference,
        b: SceneReference,
    },
    GlueIntensity(f32),
    Offscreen {
        blend_mode: u32,
        flags: u8,
        keyforms: Vec<crate::OffscreenKeyform>,
        part_keyform_indices: Vec<i32>,
    },
    Context {
        part_id: Option<String>,
        parent_id: Option<String>,
    },
}
impl SceneFieldValue {
    /// Native reference payload used by publication summaries. Context exposes
    /// organization/deformation targets; topology changes are reported separately.
    pub fn into_references(self) -> Option<Vec<SceneReference>> {
        match self {
            Self::Texture(target) => Some(vec![target]),
            Self::Masks(targets) => Some(targets.into_iter().map(Into::into).collect()),
            Self::GlueEndpoints { a, b } => Some(vec![a, b]),
            Self::Context { part_id, parent_id } => Some(vec![
                part_id.unwrap_or_default().into(),
                parent_id.unwrap_or_default().into(),
            ]),
            _ => None,
        }
    }

    pub fn field(&self) -> SceneField {
        match self {
            Self::Label(_) => SceneField::Label,
            Self::RuntimeId(_) => SceneField::RuntimeId,
            Self::Enabled(_) => SceneField::Enabled,
            Self::Appearance(_) => SceneField::Appearance,
            Self::Texture(_) => SceneField::Texture,
            Self::Masks(_) => SceneField::Masks,
            Self::MeshDrawing { .. } => SceneField::MeshDrawing,
            Self::Warp(_) => SceneField::Warp,
            Self::Rotation(_) => SceneField::Rotation,
            Self::GlueEndpoints { .. } => SceneField::GlueEndpoints,
            Self::GlueIntensity(_) => SceneField::GlueIntensity,
            Self::Offscreen { .. } => SceneField::Offscreen,
            Self::Context { .. } => SceneField::Context,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneEdit {
    Create {
        id: String,
        name: String,
        object: SceneCreate,
        #[serde(default)]
        part_id: Option<String>,
        #[serde(default)]
        parent_id: Option<String>,
        #[serde(default)]
        before: Option<String>,
    },
    Patch {
        id: String,
        expected: SceneFieldValue,
        value: SceneFieldValue,
        #[serde(default)]
        clear_glue_pairs: bool,
    },
    Move {
        ids: Vec<String>,
        destination: SceneCollection,
        before: Option<String>,
    },
    Reparent {
        edit: ReparentEdit,
    },
    Duplicate {
        ids: Vec<String>,
        #[serde(default)]
        scope: SceneCopyScope,
        #[serde(default)]
        destination: Option<OrganizationCollection>,
        #[serde(default)]
        before: Option<String>,
    },
    Delete {
        ids: Vec<String>,
        #[serde(default)]
        scope: SceneCopyScope,
    },
    TransferOffscreen {
        id: String,
        part_id: String,
        part_keyform_indices: Vec<i32>,
    },
    EnsureDrawingGroup {
        owner: String,
        parent: String,
        before: Option<String>,
    },
    SetDrawingRange {
        owner: String,
        expected: [i32; 2],
        range: [i32; 2],
    },
    DissolveDrawingGroup {
        owner: String,
    },
    DuplicateDrawingGroup {
        source: String,
        target: String,
    },
}
#[derive(Debug, Clone)]
pub struct ScenePlan {
    pub(in crate::document) domain: OrganizationPlan,
    patches: Vec<(String, SceneFieldValue, bool)>,
    reparent: Option<ReparentPlan>,
    metadata_only: bool,
    preserves_identities: bool,
    cost: Option<ConversionCost>,
}
impl ScenePlan {
    /// A Core-proven identity guarantee for the validated plan. Unknown or
    /// structural/conversion effects conservatively require identity tracking.
    pub fn preserves_identities(&self) -> bool {
        self.preserves_identities
    }
    pub fn is_noop(&self) -> bool {
        self.domain.no_op
            && self.patches.is_empty()
            && self
                .reparent
                .as_ref()
                .is_none_or(|p| p.affected_ids().is_empty())
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.domain.affected
    }
    pub fn copied_ids(&self) -> &BTreeMap<String, String> {
        &self.domain.copied
    }
    pub fn effects(&self) -> &OrganizationEffects {
        &self.domain.effects
    }
    pub fn conversion_report(&self) -> Option<&ConversionReport> {
        self.reparent.as_ref().map(ReparentPlan::report)
    }
    pub fn cost(&self) -> Option<ConversionCost> {
        self.cost
            .or_else(|| self.reparent.as_ref().map(ReparentPlan::cost))
    }
}
impl Document {
    fn empty_scene_plan(&self) -> ScenePlan {
        ScenePlan {
            domain: OrganizationPlan {
                baseline: self.snapshot_identity.clone(),
                revision: self.revision(),
                delta: Delta::default(),
                affected: vec![],
                copied: BTreeMap::new(),
                effects: OrganizationEffects {
                    drawing_preserved: true,
                    deformation_preserved: true,
                    ..Default::default()
                },
                no_op: true,
            },
            patches: vec![],
            reparent: None,
            metadata_only: false,
            preserves_identities: false,
            cost: None,
        }
    }
    pub fn scene_collection(&self, c: &SceneCollection) -> Result<Vec<String>, Status> {
        Ok(match c {
            SceneCollection::Organization { part_id } => {
                return self.organization_collection(&OrganizationCollection::PartMembers {
                    part_id: part_id.clone(),
                })
            }
            SceneCollection::Deformation { parent_id } => {
                if parent_id
                    .as_ref()
                    .is_some_and(|id| self.get_transform(id).is_none())
                {
                    return Err(Status::error(
                        "MISSING_TRANSFORM",
                        parent_id.as_ref().unwrap(),
                    ));
                }
                let parent = parent_id.as_deref().unwrap_or("");
                let mut seen = HashSet::new();
                self.hierarchy_order()
                    .deformation
                    .iter()
                    .chain(self.transform_order())
                    .chain(self.mesh_order())
                    .filter(|id| {
                        seen.insert(id.as_str()) && self.scene_deformer_parent(id) == Some(parent)
                    })
                    .cloned()
                    .collect()
            }
            SceneCollection::DrawingGroups => self.drawing_group_owners(),
            SceneCollection::Glues => {
                let mut seen = HashSet::new();
                self.hierarchy_order()
                    .glues
                    .iter()
                    .chain(self.glue_order())
                    .filter(|id| seen.insert(id.as_str()))
                    .cloned()
                    .collect()
            }
            SceneCollection::Drawing { owner } => {
                crate::draw_order::resolved_groups(self)
                    .into_iter()
                    .find(|g| g.owner == *owner)
                    .ok_or_else(|| Status::error("MISSING_DRAWING_GROUP", owner))?
                    .items
            }
        })
    }
    pub fn drawing_group_owners(&self) -> Vec<String> {
        match self.draw_order_groups() {
            Some(groups) => groups.iter().map(|g| g.owner.clone()).collect(),
            None => std::iter::once(String::new())
                .chain(self.part_order().iter().cloned())
                .collect(),
        }
    }
    pub fn scene_lifecycle_unlocked(
        &self,
        ids: &[String],
        scope: SceneCopyScope,
    ) -> Result<(), Status> {
        let mut closure = self.scene_selection(ids, scope)?;
        self.include_owned_scene_bindings(&mut closure);
        for id in closure {
            self.authoring_unlocked(&id)?;
        }
        Ok(())
    }
    pub fn scene_deformer_parent(&self, id: &str) -> Option<&str> {
        self.get_mesh(id)
            .map(|m| m.deformer_id.as_str())
            .or_else(|| self.get_transform(id).map(Transform::parent))
    }
    pub fn plan_scene_edit(&self, edit: SceneEdit) -> Result<ScenePlan, Status> {
        self.plan_scene_edit_with_control(
            edit,
            AuthoringLimits::default(),
            &ConversionControl::default(),
        )
    }
    pub fn plan_scene_edit_with_control(
        &self,
        edit: SceneEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ScenePlan, Status> {
        if !self.initialized() {
            return Err(Status::error(
                "NOT_INITIALIZED",
                "Initialize the document first",
            ));
        }
        control.check()?;
        let mut plan = self.empty_scene_plan();
        match edit {
            SceneEdit::Create {
                id,
                name,
                object,
                part_id,
                parent_id,
                before,
            } => self.plan_scene_create(&mut plan, id, name, object, part_id, parent_id, before)?,
            SceneEdit::Patch {
                id,
                expected,
                value,
                clear_glue_pairs,
            } => {
                self.authoring_unlocked(&id)?;
                if expected.field() != value.field() {
                    return Err(Status::error("WRONG_FIELD", "Field patch types differ"));
                }
                if self.scene_field(&id, expected.field())? != expected {
                    return Err(Status::error("STALE_FIELD", &id));
                }
                if clear_glue_pairs && value.field() != SceneField::GlueEndpoints {
                    return Err(Status::error(
                        "WRONG_FIELD",
                        "Pair clearing requires an endpoint patch",
                    ));
                }
                if expected != value
                    || (clear_glue_pairs && self.get_glue(&id).is_some_and(|g| !g.pairs.is_empty()))
                {
                    self.validate_scene_field(&id, &value, clear_glue_pairs)?;
                    plan.metadata_only = matches!(value, SceneFieldValue::Label(_));
                    plan.domain.effects.visibility_may_change = matches!(
                        value,
                        SceneFieldValue::Enabled(_)
                            | SceneFieldValue::Appearance(_)
                            | SceneFieldValue::Texture(_)
                            | SceneFieldValue::Masks(_)
                    );
                    plan.domain.effects.composition_may_change = self.get_offscreen(&id).is_some()
                        && !plan.metadata_only
                        || matches!(
                            value,
                            SceneFieldValue::Masks(_) | SceneFieldValue::Texture(_)
                        );
                    plan.domain.effects.deformation_preserved = !matches!(
                        value,
                        SceneFieldValue::Warp(_)
                            | SceneFieldValue::Rotation(_)
                            | SceneFieldValue::GlueEndpoints { .. }
                            | SceneFieldValue::GlueIntensity(_)
                    );
                    plan.domain.effects.drawing_preserved =
                        !matches!(value, SceneFieldValue::MeshDrawing { .. });
                    plan.domain.affected.push(id.clone());
                    plan.patches.push((id, value, clear_glue_pairs));
                }
            }
            SceneEdit::Move {
                ids,
                destination,
                before,
            } => match destination {
                SceneCollection::Organization { part_id } => {
                    plan.domain = self.plan_organization_edit(OrganizationEdit::Move {
                        ids,
                        destination: OrganizationCollection::PartMembers { part_id },
                        before,
                    })?;
                    plan.metadata_only = !plan.domain.delta.runtime_changed(self);
                }
                SceneCollection::Drawing { owner } => {
                    self.plan_drawing_move(&mut plan, &ids, &owner, &before)?
                }
                SceneCollection::DrawingGroups => {
                    return Err(Status::error(
                        "DRAWING_GROUP_ORDER_DERIVED",
                        "Move a Part drawing member to change group nesting",
                    ))
                }
                other => {
                    let members = self.scene_collection(&other)?;
                    let ordered = ordered_selection(&members, &ids)?;
                    for id in &ordered {
                        self.authoring_unlocked(id)?;
                    }
                    if let SceneCollection::Deformation { parent_id } = &other {
                        if let Some(parent) = parent_id {
                            self.authoring_unlocked(parent)?;
                        }
                    }
                    let next = relocate_members(&members, &ordered, &before)?;
                    if next != members {
                        let mut order = self.hierarchy_order().clone();
                        match other {
                            SceneCollection::Glues => {
                                order.glues = replace_collection_order(
                                    &order.glues,
                                    &members,
                                    &next,
                                    self.glue_order(),
                                )
                            }
                            SceneCollection::Deformation { .. } => {
                                order.deformation = replace_collection_order(
                                    &order.deformation,
                                    &members,
                                    &next,
                                    &self.scene_deformation_order(),
                                )
                            }
                            _ => unreachable!(),
                        };
                        plan.domain.delta.hierarchy = Some(order);
                        plan.domain.affected = ordered;
                    }
                    plan.metadata_only = true;
                }
            },
            SceneEdit::Reparent { edit } => {
                let reparent = self.plan_reparent_with_control(edit, limits, control)?;
                plan.domain.affected = reparent.affected_ids().to_vec();
                plan.domain.effects.deformation_preserved =
                    reparent.guarantee().strategy != ReparentStrategy::KeepLocalData;
                plan.domain.effects.visibility_may_change = true;
                plan.reparent = Some(reparent);
            }
            SceneEdit::Duplicate {
                ids,
                scope,
                destination,
                before,
            } => {
                plan.cost = Some(limits.check_conversion(
                    0,
                    self.estimated_content_bytes(),
                    self.estimated_content_bytes(),
                )?);
                self.plan_scene_duplicate(&mut plan, &ids, scope, destination.as_ref(), &before)?;
            }
            SceneEdit::Delete { ids, scope } => self.plan_scene_delete(&mut plan, &ids, scope)?,
            SceneEdit::TransferOffscreen {
                id,
                part_id,
                part_keyform_indices,
            } => {
                self.authoring_unlocked(&id)?;
                self.authoring_unlocked(&part_id)?;
                let mut os = self
                    .get_offscreen(&id)
                    .ok_or_else(|| Status::error("MISSING_OFFSCREEN", &id))?
                    .clone();
                os.part_id = part_id;
                os.part_keyform_indices = part_keyform_indices;
                check(self.validate_offscreen(&os))?;
                if self.get_offscreen(&id) != Some(&os) {
                    plan.domain.affected = vec![id];
                    plan.domain.delta.offscreens.push(os);
                    plan.domain.effects.composition_may_change = true;
                }
            }
            SceneEdit::EnsureDrawingGroup {
                owner,
                parent,
                before,
            } => self.plan_ensure_drawing(&mut plan, &owner, &parent, &before)?,
            SceneEdit::SetDrawingRange {
                owner,
                expected,
                range,
            } => {
                self.drawing_group_unlocked(&owner)?;
                let mut groups = self
                    .draw_order_groups()
                    .map(|g| g.to_vec())
                    .unwrap_or_else(|| crate::draw_order::resolved_groups(self));
                let group = groups
                    .iter_mut()
                    .find(|g| g.owner == owner)
                    .ok_or_else(|| Status::error("MISSING_DRAWING_GROUP", &owner))?;
                if [group.min_order, group.max_order] != expected {
                    return Err(Status::error("STALE_FIELD", &owner));
                }
                if range != expected {
                    for member in &group.items {
                        self.authoring_unlocked(member)?;
                    }
                    group.min_order = range[0];
                    group.max_order = range[1];
                    self.finish_drawing_plan(&mut plan, groups)?;
                }
            }
            SceneEdit::DissolveDrawingGroup { owner } => {
                self.plan_drawing_dissolve(&mut plan, &owner)?
            }
            SceneEdit::DuplicateDrawingGroup { source, target } => {
                plan.cost = Some(limits.check_conversion(
                    0,
                    self.estimated_content_bytes(),
                    self.estimated_content_bytes(),
                )?);
                self.plan_duplicate_drawing(&mut plan, &source, &target)?;
            }
        }
        plan.domain.affected.sort();
        plan.domain.affected.dedup();
        plan.domain.no_op = !plan.domain.delta.changed(self);
        // Field installation changes values/references, never owned IDs except
        // explicitly clearing Glue pairs. Nonempty domain deltas and reparent
        // conversions keep the conservative path, including role lifecycles.
        plan.preserves_identities = plan.domain.no_op
            && plan.reparent.is_none()
            && plan.patches.iter().all(|(_, value, clear)| match value {
                SceneFieldValue::GlueEndpoints { .. } => !clear,
                SceneFieldValue::Label(_)
                | SceneFieldValue::RuntimeId(_)
                | SceneFieldValue::Enabled(_)
                | SceneFieldValue::Appearance(_)
                | SceneFieldValue::Texture(_)
                | SceneFieldValue::Masks(_)
                | SceneFieldValue::MeshDrawing { .. }
                | SceneFieldValue::Warp(_)
                | SceneFieldValue::Rotation(_)
                | SceneFieldValue::GlueIntensity(_)
                | SceneFieldValue::Offscreen { .. } => true,
                SceneFieldValue::Context { .. } => false,
            });
        control.check()?;
        Ok(plan)
    }
    pub fn apply_scene_plan(&mut self, plan: &ScenePlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish or cancel the current edit",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.domain.baseline)
            || self.revision() != plan.domain.revision
        {
            return self.failed(Status::error("STALE_PLAN", "Replan scene edit"));
        }
        if let Some(reparent) = &plan.reparent {
            return self.apply_reparent_plan(reparent);
        }
        if plan.is_noop() {
            return self.failed(Status::ok());
        }
        // An identity-preserving plan may still write relationships or order.
        // Retain coverage only for the audited Mesh fields with no domain delta.
        let tracked = (plan.domain.delta.is_empty()
            && plan.patches.iter().all(|(id, value, clear)| {
                !clear
                    && self.get_mesh(id).is_some()
                    && matches!(
                        value,
                        SceneFieldValue::Label(_)
                            | SceneFieldValue::Appearance(_)
                            | SceneFieldValue::Enabled(_)
                    )
            }))
        .then(|| self.track_mesh_write(plan.patches.iter().map(|(id, _, _)| id.clone()).collect()));
        if tracked.is_none() {
            plan.domain.delta.install(self);
        }
        for (id, value, clear) in &plan.patches {
            self.install_scene_field(id, value, *clear);
        }
        let kind = if plan.metadata_only {
            ChangeKind::Metadata
        } else {
            ChangeKind::Structure
        };
        let mesh_ids = if plan.metadata_only {
            vec![]
        } else {
            self.mesh_order().to_vec()
        };
        if let Some(write) = tracked {
            self.changed_tracked(write, kind, mesh_ids, plan.domain.affected.clone())
        } else {
            self.changed(kind, mesh_ids, plan.domain.affected.clone())
        }
    }
    fn scene_deformation_order(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.hierarchy_order()
            .deformation
            .iter()
            .chain(self.transform_order())
            .chain(self.mesh_order())
            .filter(|id| seen.insert(id.as_str()))
            .cloned()
            .collect()
    }
}
fn check(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}
fn ordered_selection(members: &[String], ids: &[String]) -> Result<Vec<String>, Status> {
    let selected: HashSet<_> = ids.iter().collect();
    if ids.is_empty() || selected.len() != ids.len() {
        return Err(Status::error(
            "INVALID_SELECTION",
            "Select distinct members",
        ));
    }
    if ids.iter().any(|id| !members.contains(id)) {
        return Err(Status::error("WRONG_SCENE_COLLECTION","Source is not a member of this collection; deformation parent changes require a reparent plan"));
    }
    Ok(members
        .iter()
        .filter(|id| selected.contains(id))
        .cloned()
        .collect())
}
fn relocate_members(
    members: &[String],
    ids: &[String],
    before: &Option<String>,
) -> Result<Vec<String>, Status> {
    if before.as_ref().is_some_and(|id| ids.contains(id)) {
        if ids.len() == 1 {
            return Ok(members.to_vec());
        }
        return Err(Status::error("INVALID_PLACEMENT", "Anchor is selected"));
    }
    let mut next: Vec<_> = members
        .iter()
        .filter(|id| !ids.contains(id))
        .cloned()
        .collect();
    let at = match before {
        Some(id) => next
            .iter()
            .position(|v| v == id)
            .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))?,
        None => next.len(),
    };
    next.splice(at..at, ids.iter().cloned());
    Ok(next)
}
fn replace_collection_order(
    original: &[String],
    members: &[String],
    next: &[String],
    fallback: &[String],
) -> Vec<String> {
    let mut seen = HashSet::new();
    let all: Vec<_> = original
        .iter()
        .chain(fallback)
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect();
    let mut next = next.iter();
    all.into_iter()
        .map(|id| {
            if members.contains(&id) {
                next.next().unwrap().clone()
            } else {
                id
            }
        })
        .collect()
}
