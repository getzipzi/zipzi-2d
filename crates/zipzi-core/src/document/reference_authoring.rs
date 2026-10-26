//! Editable metadata references have native identities; targets remain shared.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
mod operations;
mod projection;
pub use projection::*;
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
#[derive(Debug, Clone, Default)]
pub(super) struct ReferenceIndex {
    entries: Vec<ReferenceIdentity>,
    slots: HashMap<String, usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceCollection {
    ModelParameterGroups,
    ModelHitAreas,
    CdiCombinedSets,
    ModelParameterMembers {
        /// Existing model3 parameter-group object ID; this collection has no root/unassigned group.
        group_id: String,
    },
    CdiCombinedMembers {
        /// Stable CDI combined-parameter set ID owning the selected members.
        set_id: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceSelectionField {
    MeshMasks,
    OffscreenMasks,
    BlendConstraints,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceFields {
    ModelParameterGroup {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    ModelHitArea {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Resolved mesh target or preserved unresolved mesh runtime identifier.
        mesh: ModelTargetRef,
    },
    ModelParameterMember {
        /// Resolved document target or preserved unresolved runtime identifier; entry identity
        /// remains independent of the target.
        target: ModelTargetRef,
    },
    CdiCombinedMember {
        /// Resolved document target or preserved unresolved runtime identifier; entry identity
        /// remains independent of the target.
        target: CdiParameterRef,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ReferenceEdit {
    CreateModelParameterGroup {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreateModelHitArea {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreateCdiCombinedSet {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreateMember {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: ReferenceCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Patch {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: ReferenceFields,
        /// Typed replacement metadata fields; their variant must match the addressed reference
        /// kind.
        fields: ReferenceFields,
    },
    Move {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: ReferenceCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Duplicate {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: ReferenceCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
    },
    Dissolve {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    ReplaceSelection {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Explicit field discriminator; must match the addressed object/domain.
        field: ReferenceSelectionField,
        /// Exact current ordered target-ID selection; a mismatch rejects the edit.
        expected: Vec<String>,
        /// Complete replacement ordered target-ID selection; an empty list explicitly clears it.
        targets: Vec<String>,
    },
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
#[derive(Debug, Clone)]
pub struct ReferencePlan {
    effects: ReferenceEffects,
    domain: OrganizationPlan,
    patch: Option<(String, ReferenceFields)>,
    runtime: bool,
}
impl ReferencePlan {
    pub fn effects(&self) -> &ReferenceEffects {
        &self.effects
    }
    pub fn is_noop(&self) -> bool {
        self.domain.no_op && self.patch.is_none()
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.domain.affected
    }
    pub fn copied_ids(&self) -> &BTreeMap<String, String> {
        &self.domain.copied
    }
}
impl Document {
    fn reference_index(&self) -> &ReferenceIndex {
        self.reference_index.get_or_init(|| {
            let mut index = ReferenceIndex::default();
            let mut add = |id: &str, kind, owner| {
                index.slots.insert(id.into(), index.entries.len());
                index.entries.push(ReferenceIdentity {
                    id: id.into(),
                    kind,
                    owner,
                });
            };
            for g in self.model3_settings().groups.iter().flatten() {
                add(&g.id, ReferenceKind::ModelParameterGroup, None);
                for p in &g.parameters {
                    add(
                        &p.id,
                        ReferenceKind::ModelParameterMember,
                        Some(g.id.clone()),
                    );
                }
            }
            for a in self.model3_settings().hit_areas.iter().flatten() {
                add(&a.id, ReferenceKind::ModelHitArea, None);
            }
            for s in self.display_info().combined_parameters.iter().flatten() {
                add(&s.id, ReferenceKind::CdiCombinedSet, None);
                for p in &s.members {
                    add(&p.id, ReferenceKind::CdiCombinedMember, Some(s.id.clone()));
                }
            }
            Arc::new(index)
        })
    }
    pub fn reference_identities(&self) -> &[ReferenceIdentity] {
        &self.reference_index().entries
    }
    pub fn reference_identity(&self, id: &str) -> Option<&ReferenceIdentity> {
        let i = *self.reference_index().slots.get(id)?;
        self.reference_index().entries.get(i)
    }
    pub fn get_model_parameter_group(&self, id: &str) -> Option<&ModelParameterGroup> {
        self.model3_settings()
            .groups
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn get_model_hit_area(&self, id: &str) -> Option<&ModelHitArea> {
        self.model3_settings()
            .hit_areas
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn get_combined_set(&self, id: &str) -> Option<&CdiCombinedSet> {
        self.display_info()
            .combined_parameters
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn get_model_parameter_member(&self, id: &str) -> Option<&ReferenceEntry<ModelTargetRef>> {
        let g = self.reference_identity(id)?.owner.as_ref()?;
        self.get_model_parameter_group(g)?
            .parameters
            .iter()
            .find(|p| p.id == id)
    }
    pub fn get_combined_member(&self, id: &str) -> Option<&ReferenceEntry<CdiParameterRef>> {
        let g = self.reference_identity(id)?.owner.as_ref()?;
        self.get_combined_set(g)?
            .members
            .iter()
            .find(|p| p.id == id)
    }
    pub fn reference_collection(&self, c: &ReferenceCollection) -> Result<Vec<String>, Status> {
        Ok(match c {
            ReferenceCollection::ModelParameterGroups => self
                .model3_settings()
                .groups
                .iter()
                .flatten()
                .map(|g| g.id.clone())
                .collect(),
            ReferenceCollection::ModelHitAreas => self
                .model3_settings()
                .hit_areas
                .iter()
                .flatten()
                .map(|g| g.id.clone())
                .collect(),
            ReferenceCollection::CdiCombinedSets => self
                .display_info()
                .combined_parameters
                .iter()
                .flatten()
                .map(|g| g.id.clone())
                .collect(),
            ReferenceCollection::ModelParameterMembers { group_id } => self
                .get_model_parameter_group(group_id)
                .ok_or_else(|| Status::error("MISSING_MODEL_GROUP", group_id))?
                .parameters
                .iter()
                .map(|p| p.id.clone())
                .collect(),
            ReferenceCollection::CdiCombinedMembers { set_id } => self
                .get_combined_set(set_id)
                .ok_or_else(|| Status::error("MISSING_CDI_SET", set_id))?
                .members
                .iter()
                .map(|p| p.id.clone())
                .collect(),
        })
    }
    pub fn reference_fields(&self, id: &str) -> Result<ReferenceFields, Status> {
        if let Some(g) = self.get_model_parameter_group(id) {
            return Ok(ReferenceFields::ModelParameterGroup {
                name: g.name.clone(),
            });
        }
        if let Some(a) = self.get_model_hit_area(id) {
            return Ok(ReferenceFields::ModelHitArea {
                name: a.name.clone(),
                mesh: a.mesh.clone(),
            });
        }
        if let Some(p) = self.get_model_parameter_member(id) {
            return Ok(ReferenceFields::ModelParameterMember {
                target: p.target.clone(),
            });
        }
        if let Some(p) = self.get_combined_member(id) {
            return Ok(ReferenceFields::CdiCombinedMember {
                target: p.target.clone(),
            });
        }
        Err(Status::error("NO_REFERENCE_FIELDS", id))
    }
    pub fn reference_unlocked(&self, id: &str) -> Result<(), Status> {
        self.authoring_unlocked(id)?;
        let identity = self
            .reference_identity(id)
            .ok_or_else(|| Status::error("MISSING_REFERENCE_OBJECT", id))?;
        if let Some(owner) = &identity.owner {
            self.authoring_unlocked(owner)?;
        }
        for child in self
            .reference_identities()
            .iter()
            .filter(|c| c.owner.as_deref() == Some(id))
        {
            self.authoring_unlocked(&child.id)?;
        }
        Ok(())
    }
    pub(super) fn validate_reference_storage(
        &self,
        model: &Model3Settings,
        info: &DisplayInfo,
    ) -> Result<(), Status> {
        let mut seen = HashSet::new();
        let ids = model
            .groups
            .iter()
            .flatten()
            .flat_map(|g| std::iter::once(&g.id).chain(g.parameters.iter().map(|p| &p.id)))
            .chain(model.hit_areas.iter().flatten().map(|a| &a.id))
            .chain(
                info.combined_parameters
                    .iter()
                    .flatten()
                    .flat_map(|s| std::iter::once(&s.id).chain(s.members.iter().map(|p| &p.id))),
            );
        for id in ids {
            if !valid_uuid(id)
                || !seen.insert(id)
                || (self.contains_id(id) && self.reference_identity(id).is_none())
            {
                return Err(Status::error("INVALID_REFERENCE_IDENTITY", id));
            }
        }
        Ok(())
    }
    pub fn reference_selection(
        &self,
        owner: &str,
        field: ReferenceSelectionField,
    ) -> Result<&[String], Status> {
        match field {
            ReferenceSelectionField::MeshMasks => self.get_mesh(owner).map(|m| m.masks.as_slice()),
            ReferenceSelectionField::OffscreenMasks => {
                self.get_offscreen(owner).map(|m| m.masks.as_slice())
            }
            ReferenceSelectionField::BlendConstraints => self
                .get_blend_binding(owner)
                .map(|b| b.constraint_ids.as_slice()),
        }
        .ok_or_else(|| Status::error("WRONG_REFERENCE_SELECTION", owner))
    }
    pub fn plan_reference_edit(&self, edit: ReferenceEdit) -> Result<ReferencePlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut p = ReferencePlan {
            effects: Default::default(),
            domain: OrganizationPlan {
                baseline: self.snapshot_identity.clone(),
                revision: self.revision(),
                delta: Default::default(),
                affected: vec![],
                copied: BTreeMap::new(),
                effects: Default::default(),
                no_op: true,
            },
            patch: None,
            runtime: false,
        };
        self.plan_reference_operation(&mut p, edit)?;
        p.domain.no_op = !p.domain.delta.changed(self);
        if !p.domain.no_op {
            self.validate_compound_organization(&p.domain)?;
        }
        p.effects.metadata_only = !p.runtime;
        if let Some(model) = &p.domain.delta.model3 {
            p.effects.behavior_groups_changed = model.groups != self.model3_settings().groups;
            p.effects.hit_areas_changed = model.hit_areas != self.model3_settings().hit_areas;
        }
        if let Some(info) = &p.domain.delta.groups {
            p.effects.combined_sets_changed =
                info.combined_parameters != self.display_info().combined_parameters;
        }
        if let Some((_, fields)) = &p.patch {
            match fields {
                ReferenceFields::ModelParameterGroup { .. }
                | ReferenceFields::ModelParameterMember { .. } => {
                    p.effects.behavior_groups_changed = true
                }
                ReferenceFields::ModelHitArea { .. } => p.effects.hit_areas_changed = true,
                ReferenceFields::CdiCombinedMember { .. } => p.effects.combined_sets_changed = true,
            }
        }
        p.effects.drawing_changed = p.runtime
            && (!p.domain.delta.meshes.is_empty() || !p.domain.delta.offscreens.is_empty());
        p.effects.deformation_changed = p.runtime && !p.domain.delta.blends.is_empty();
        p.domain.affected.extend(p.domain.copied.keys().cloned());
        p.domain.affected.extend(p.domain.copied.values().cloned());
        p.domain.affected.sort();
        p.domain.affected.dedup();
        Ok(p)
    }
    pub fn apply_reference_plan(&mut self, p: &ReferencePlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &p.domain.baseline)
            || self.revision() != p.domain.revision
        {
            return self.failed(Status::error("STALE_PLAN", "Replan reference edit"));
        }
        if p.is_noop() {
            return self.failed(Status::ok());
        }
        if let Some((id, fields)) = &p.patch {
            self.install_reference_fields(id, fields);
        } else {
            p.domain.delta.install(self);
        }
        self.changed(
            if p.runtime {
                ChangeKind::Structure
            } else {
                ChangeKind::Metadata
            },
            if p.runtime {
                self.mesh_order().to_vec()
            } else {
                vec![]
            },
            p.domain.affected.clone(),
        )
    }
}
