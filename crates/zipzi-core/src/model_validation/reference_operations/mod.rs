//! Editable metadata references have native identities; targets remain shared.
use super::organization::{Delta, OrganizationChanges};
use super::*;
use std::collections::{BTreeMap, HashSet};
mod operations;

pub use zipzi_contract::document::{
    ReferenceCollection, ReferenceEdit, ReferenceFields, ReferenceSelectionField,
};

#[derive(Debug, Clone)]
pub struct ReferenceChanges {
    pub(crate) effects: ReferenceEffects,
    pub(crate) domain: OrganizationChanges,
    pub(crate) patch: Option<(String, ReferenceFields)>,
    runtime: bool,
}
impl Model<'_> {
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
    pub fn plan_reference_edit(&self, edit: ReferenceEdit) -> Result<ReferenceChanges, Status> {
        let mut p = ReferenceChanges {
            effects: Default::default(),
            domain: OrganizationChanges {
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
}
