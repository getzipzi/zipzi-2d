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
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneCollection {
    Organization {
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
    },
    Deformation {
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
    },
    Glues,
    DrawingGroups,
    Drawing {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneCopyScope {
    #[default]
    Selected,
    DeformationSubtree,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneCreate {
    Mesh {
        #[serde(default)]
        /// Image-asset reference; resolved values use asset IDs and unresolved values retain
        /// external texture identifiers.
        texture: SceneReference,
    },
    Warp {
        #[serde(default)]
        /// Optional [left, bottom, right, top] warp bounds in parent coordinates; None requests
        /// native defaults.
        bounds: Option<[f32; 4]>,
    },
    Rotation,
    Offscreen,
    Glue {
        #[serde(default)]
        /// Mesh A endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        a: SceneReference,
        #[serde(default)]
        /// Mesh B endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        b: SceneReference,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape)]
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
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneFieldValue {
    Label(#[doc = "Replacement display label."] String),
    RuntimeId(
        /// Replacement external runtime identifier; an empty string explicitly clears it.
        String,
    ),
    Enabled(#[doc = "Replacement model evaluation enabled flag."] bool),
    Appearance(#[doc = "Replacement opacity and RGB color factors."] Appearance),
    Texture(
        /// Resolved image-asset reference or explicitly unresolved texture runtime identifier.
        SceneReference,
    ),
    Masks(#[doc = "Complete ordered clipping-mask mesh selection."] Vec<String>),
    MeshDrawing {
        /// Drawing-order coordinate; distinct from the final evaluated render_order. None
        /// leaves this channel unspecified for native evaluation.
        draw_order: Option<f32>,
        /// Compositing mode used when drawing this object.
        blend_mode: BlendMode,
        /// Original format-specific blend-mode code; None means no preserved raw code.
        raw_blend_mode: Option<u32>,
        /// Whether both triangle faces contribute to drawing.
        double_sided: bool,
        /// Whether to invert the combined clipping mask for this drawable.
        inverted_mask: bool,
    },
    Warp(#[doc = "Complete replacement warp lattice and controller definition."] WarpTransform),
    Rotation(#[doc = "Complete replacement rotation definition."] RotationTransform),
    GlueEndpoints {
        /// Mesh A endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        a: SceneReference,
        /// Mesh B endpoint of the Glue; resolved references use stable mesh IDs and unresolved
        /// references retain runtime identifiers.
        b: SceneReference,
    },
    GlueIntensity(#[doc = "Replacement Glue influence factor."] f32),
    Offscreen {
        /// Compositing mode used when drawing this object.
        blend_mode: u32,
        /// Preserved format-specific Offscreen compositing flags.
        flags: u8,
        /// Shape samples for this binding; coordinate/sample identities must match its axes or
        /// key table.
        keyforms: Vec<crate::OffscreenKeyform>,
        /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
        part_keyform_indices: Vec<i32>,
    },
    Context {
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
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
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneEdit {
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Native scene object kind and its creation options; not a persisted full object record.
        object: SceneCreate,
        #[serde(default)]
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
        #[serde(default)]
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
        /// Stable anchor in the organization collection (`part_id`), or the
        /// connection collection for Glue. None appends to that collection.
        #[serde(default)]
        before: Option<String>,
        /// Independent stable anchor in the deformation collection (`parent_id`).
        /// Only Mesh/Warp/Rotation support this position; None appends.
        #[serde(default)]
        deformation_before: Option<String>,
    },
    Patch {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: SceneFieldValue,
        /// Scalar value in the referenced parameter domain.
        value: SceneFieldValue,
        #[serde(default)]
        /// Explicitly clear existing Glue vertex pairs when changing Glue endpoints.
        clear_glue_pairs: bool,
    },
    Move {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: SceneCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Reparent {
        /// Edit candidate handle; None selects formal document state.
        edit: ReparentEdit,
    },
    Duplicate {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        #[serde(default)]
        /// Native copy/delete scope specifying whether dependent deformation descendants are
        /// included.
        scope: SceneCopyScope,
        #[serde(default)]
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: Option<OrganizationCollection>,
        #[serde(default)]
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        #[serde(default)]
        /// Native copy/delete scope specifying whether dependent deformation descendants are
        /// included.
        scope: SceneCopyScope,
    },
    TransferOffscreen {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Destination Part object ID; it must name an existing Part.
        part_id: String,
        /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
        part_keyform_indices: Vec<i32>,
    },
    EnsureDrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Drawing-group parent owner; an empty string selects the root drawing group.
        parent: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    SetDrawingRange {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: [i32; 2],
        /// Inclusive lower and upper drawing-order bounds, in that order.
        range: [i32; 2],
    },
    DissolveDrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
    DuplicateDrawingGroup {
        /// Exact formal or candidate source captured for this result; later host changes do not
        /// rewrite it.
        source: String,
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
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
    /// Validate a collection without materializing deformation members. Other
    /// collection kinds retain their native collection validation path.
    pub fn validate_scene_collection(&self, c: &SceneCollection) -> Result<(), Status> {
        if let SceneCollection::Deformation { parent_id } = c {
            if let Some(id) = parent_id
                .as_ref()
                .filter(|id| self.get_transform(id).is_none())
            {
                return Err(Status::error("MISSING_TRANSFORM", id));
            }
            Ok(())
        } else {
            self.scene_collection(c).map(|_| ())
        }
    }

    pub fn scene_collection(&self, c: &SceneCollection) -> Result<Vec<String>, Status> {
        Ok(match c {
            SceneCollection::Organization { part_id } => {
                return self.organization_collection(&OrganizationCollection::PartMembers {
                    part_id: part_id.clone(),
                });
            }
            SceneCollection::Deformation { parent_id } => {
                self.validate_scene_collection(c)?;
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
                deformation_before,
            } => self.plan_scene_create(
                &mut plan,
                id,
                name,
                object,
                part_id,
                parent_id,
                before,
                deformation_before,
            )?,
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
                    ));
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
    pub(in crate::document) fn reparent_placement(
        &self,
        sources: &[String],
        edit: &super::ReparentEdit,
    ) -> Result<Option<crate::document::HierarchyOrder>, Status> {
        let Some(placement) = &edit.placement else {
            return Ok(None);
        };
        let mut members = self.scene_collection(&SceneCollection::Deformation {
            parent_id: (!edit.parent_id.is_empty()).then(|| edit.parent_id.clone()),
        })?;
        if placement
            .before
            .as_ref()
            .is_some_and(|id| !members.contains(id))
        {
            return Err(Status::error(
                "INVALID_PLACEMENT",
                "Anchor is not in the destination",
            ));
        }
        for id in sources {
            if !members.contains(id) {
                members.push(id.clone());
            }
        }
        let next = relocate_members(&members, sources, &placement.before)?;
        let mut order = self.hierarchy_order().clone();
        order.deformation = replace_collection_order(
            &order.deformation,
            &members,
            &next,
            &self.scene_deformation_order(),
        );
        Ok((order.deformation != self.scene_deformation_order()).then_some(order))
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
        return Err(Status::error(
            "WRONG_SCENE_COLLECTION",
            "Source is not a member of this collection; deformation parent changes require a reparent plan",
        ));
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
