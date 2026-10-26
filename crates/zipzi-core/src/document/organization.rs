//! Organization collections are native relations, independent of deformation and drawing.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
mod index;
pub(in crate::document) mod lifecycle;
pub(in crate::document) use index::{OrganizationIndex, same_metadata_shape};
mod scene;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum OrganizationCollection {
    Parameters {
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
    },
    ParameterGroups {
        /// Parent object ID in this hierarchy; independent of storage and drawing order. None
        /// selects the root/unassigned container.
        parent_id: Option<String>,
    },
    PartMembers {
        /// Organizational Part owner ID; independent of the deformation parent. None selects
        /// the root/unassigned container.
        part_id: Option<String>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum OrganizationDeletePolicy {
    Contents,
    Dissolve,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum OrganizationEdit {
    CreateParameter {
        /// Complete parameter record with identity, finite range, default and runtime behavior.
        parameter: Parameter,
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreateGroup {
        /// Complete CDI parameter-group metadata record to create or replace.
        group: CdiParameterGroup,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    CreatePart {
        /// Complete Part record, including identity, hierarchy context and enabled/drawing state.
        part: Part,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    SetParameterGroup {
        /// Stable document parameter ID, not its exported runtime identifier.
        parameter_id: String,
        /// Parameter-group ID in the document metadata hierarchy. None selects the
        /// root/unassigned container.
        group_id: Option<String>,
    },
    PatchGroup {
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: CdiParameterGroup,
        /// Complete CDI parameter-group metadata record to create or replace.
        group: CdiParameterGroup,
    },
    Move {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: OrganizationCollection,
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
        destination: OrganizationCollection,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Delete {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: OrganizationDeletePolicy,
    },
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct OrganizationEffects {
    pub drawing_preserved: bool,
    pub deformation_preserved: bool,
    pub visibility_may_change: bool,
    pub composition_may_change: bool,
    /// Drawing membership is exclusive. External members stay in the original group.
    pub external_drawing_members: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct OrganizationPlan {
    pub(in crate::document) baseline: Arc<()>,
    pub(in crate::document) revision: u64,
    pub(in crate::document) delta: Delta,
    pub(in crate::document) affected: Vec<String>,
    pub(in crate::document) copied: BTreeMap<String, String>,
    pub(in crate::document) effects: OrganizationEffects,
    pub(in crate::document) no_op: bool,
}
impl OrganizationPlan {
    pub fn is_noop(&self) -> bool {
        self.no_op
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
    pub fn copied_ids(&self) -> &BTreeMap<String, String> {
        &self.copied
    }
    pub fn effects(&self) -> &OrganizationEffects {
        &self.effects
    }
    pub fn preserves_identities(&self) -> bool {
        self.delta.removed.is_empty()
    }
}
#[derive(Debug, Clone, Default)]
pub(in crate::document) struct Delta {
    pub(in crate::document) model3: Option<Model3Settings>,
    pub(in crate::document) parameters: Vec<Parameter>,
    pub(in crate::document) groups: Option<DisplayInfo>,
    pub(in crate::document) parts: Vec<Part>,
    pub(in crate::document) parents: Vec<(String, String)>,
    pub(in crate::document) meshes: Vec<Mesh>,
    pub(in crate::document) transforms: Vec<Transform>,
    pub(in crate::document) offscreens: Vec<Offscreen>,
    pub(in crate::document) bindings: Vec<MeshBinding>,
    pub(in crate::document) scene_bindings: Vec<SceneBinding>,
    pub(in crate::document) blends: Vec<BlendShapeBinding>,
    pub(in crate::document) glues: Vec<Glue>,
    pub(in crate::document) hierarchy: Option<HierarchyOrder>,
    pub(in crate::document) drawing: Option<Vec<DrawOrderGroup>>,
    pub(in crate::document) removed: Vec<String>,
}
impl Document {
    pub(super) fn validate_organization_metadata_unlocked(
        &self,
        info: &DisplayInfo,
    ) -> Result<(), Status> {
        for g in info.parameter_groups.iter().flatten() {
            if self.get_parameter_group(&g.id).is_none() && self.contains_id(&g.id) {
                return Err(Status::error("DUPLICATE_ID", &g.id));
            }
            if self.get_parameter_group(&g.id) != Some(g) {
                self.organization_unlocked(&g.id)?;
                if let Some(parent) = g.parent_id.as_deref() {
                    self.organization_unlocked(parent)?;
                }
                if self
                    .get_parameter_group(&g.id)
                    .is_some_and(|old| old.parent_id != g.parent_id)
                {
                    self.organization_subtree_unlocked(&[g.id.clone()])?;
                }
            }
        }
        for old in self.display_info().parameter_groups.iter().flatten() {
            if !info
                .parameter_groups
                .iter()
                .flatten()
                .any(|g| g.id == old.id)
            {
                self.organization_subtree_unlocked(&[old.id.clone()])?;
            }
        }
        for old in self.display_info().parameter_groups.iter().flatten() {
            let new = info
                .parameter_groups
                .iter()
                .flatten()
                .find(|g| g.id == old.id);
            if new.is_some_and(|g| g.parent_id == old.parent_id) {
                let rank = |info: &DisplayInfo| {
                    info.parameter_groups
                        .iter()
                        .flatten()
                        .filter(|g| g.parent_id == old.parent_id)
                        .position(|g| g.id == old.id)
                };
                if rank(self.display_info()) != rank(info) {
                    self.organization_subtree_unlocked(&[old.id.clone()])?;
                }
            }
        }
        for group in self.display_info().parameter_groups.iter().flatten() {
            let unresolved = |value: &DisplayInfo| {
                value.parameters.iter().flatten()
                .filter(|entry| matches!(entry, CdiParameterEntry::Unresolved { group_id: Some(owner), .. } if owner == &group.id))
                .cloned().collect::<Vec<_>>()
            };
            if unresolved(self.display_info()) != unresolved(info) {
                self.organization_unlocked(&group.id)?;
            }
        }
        let previous = self.explicit_parameter_info();
        let mut proposed = info.clone();
        let entries = proposed.parameters.get_or_insert_with(Vec::new);
        for id in self.parameter_order() {
            if !entries.iter().any(
                |e| matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if parameter_id==id),
            ) {
                entries.push(CdiParameterEntry::Resolved {
                    parameter_id: id.clone(),
                    group_id: None,
                    extensions: Default::default(),
                });
            }
        }
        let placement = |info: &DisplayInfo, id: &str| {
            let group=info.parameters.iter().flatten().find(|e|matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if parameter_id==id)).and_then(|e|e.group_id());
            let rank=info.parameters.iter().flatten().filter(|e|e.group_id()==group && matches!(e,CdiParameterEntry::Resolved {..})).position(|e|matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if parameter_id==id));
            (group.map(str::to_owned), rank)
        };
        for id in self.parameter_order() {
            let old = placement(&previous, id);
            let new = placement(&proposed, id);
            let extensions = |info: &DisplayInfo| {
                info.parameters.iter().flatten().find(|e|matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if parameter_id==id)).map(|e|e.extensions().clone())
            };
            if old != new || extensions(&previous) != extensions(&proposed) {
                self.organization_unlocked(id)?;
                if let Some(owner) = new.0 {
                    self.organization_unlocked(&owner)?;
                }
            }
        }

        Ok(())
    }
    pub fn get_parameter_group(&self, id: &str) -> Option<&CdiParameterGroup> {
        self.display_info()
            .parameter_groups
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn parameter_group_for(&self, id: &str) -> Option<&str> {
        self.display_info()
            .parameters
            .iter()
            .flatten()
            .find(
                |e| matches!(e,CdiParameterEntry::Resolved { parameter_id,.. } if parameter_id==id),
            )
            .and_then(|e| e.group_id())
    }
    pub fn organization_owner(&self, id: &str) -> Option<&str> {
        self.organization_index()
            .owners
            .get(id)
            .and_then(|owner| owner.as_deref())
    }
    pub fn ordered_parameter_ids(&self) -> Vec<String> {
        self.organization_index().parameters.clone()
    }
    pub fn organization_collection(
        &self,
        c: &OrganizationCollection,
    ) -> Result<Vec<String>, Status> {
        let key = match c {
            OrganizationCollection::Parameters { group_id } => {
                if let Some(id) = group_id {
                    if self.get_parameter_group(id).is_none() {
                        return Err(Status::error("MISSING_CDI_GROUP", id));
                    }
                }
                (1, group_id.clone())
            }
            OrganizationCollection::ParameterGroups { parent_id } => {
                if let Some(id) = parent_id {
                    if self.get_parameter_group(id).is_none() {
                        return Err(Status::error("MISSING_CDI_GROUP", id));
                    }
                }
                (0, parent_id.clone())
            }
            OrganizationCollection::PartMembers { part_id } => {
                if let Some(id) = part_id {
                    if self.get_part(id).is_none() {
                        return Err(Status::error("MISSING_PART", id));
                    }
                }
                (2, part_id.clone())
            }
        };
        Ok(self
            .organization_index()
            .collections
            .get(&key)
            .cloned()
            .unwrap_or_default())
    }
    pub fn organization_unlocked(&self, id: &str) -> Result<(), Status> {
        self.authoring_unlocked(id)?;
        if self.get_parameter(id).is_some() || self.get_parameter_group(id).is_some() {
            let mut owner = Some(id);
            while let Some(id) = owner {
                if self.editor_state().get(id).locked {
                    return Err(Status::error("OBJECT_LOCKED", id));
                }
                owner = if let Some(g) = self.get_parameter_group(id) {
                    g.parent_id.as_deref()
                } else {
                    self.parameter_group_for(id)
                };
            }
        }
        Ok(())
    }
    pub fn organization_subtree(&self, ids: &[String]) -> Result<Vec<String>, Status> {
        let roots = self.organization_selection(ids)?;
        let index = self.organization_index();
        fn visit(id: &str, index: &OrganizationIndex, out: &mut Vec<String>) {
            out.push(id.into());
            if let Some(children) = index.children.get(id) {
                for child in children {
                    visit(child, index, out);
                }
            }
        }
        let mut out = vec![];
        for root in roots {
            visit(&root, index, &mut out);
        }
        Ok(out)
    }
    pub fn organization_subtree_unlocked(&self, ids: &[String]) -> Result<(), Status> {
        let closure = self.organization_subtree(ids)?;
        for id in &closure {
            self.organization_unlocked(id)?;
            if let Some(b) = self.binding_for_mesh(id) {
                self.authoring_unlocked(&b.id)?;
            }
            if let Some(b) = self.binding_for_scene(id) {
                self.authoring_unlocked(&b.id)?;
            }
            for b in self.iter_blend_bindings_for_target(id) {
                self.authoring_unlocked(&b.id)?;
            }
        }
        Ok(())
    }
    fn organization_selection(&self, ids: &[String]) -> Result<Vec<String>, Status> {
        let wanted: HashSet<_> = ids.iter().map(String::as_str).collect();
        if ids.is_empty() || wanted.len() != ids.len() {
            return Err(Status::error(
                "INVALID_SELECTION",
                "Select distinct organization objects",
            ));
        }
        let index = self.organization_index();
        if ids.iter().any(|id| !index.ranks.contains_key(id)) {
            return Err(Status::error(
                "MISSING_ORGANIZATION_OBJECT",
                "Source no longer exists",
            ));
        }
        let mut selected: Vec<_> = ids
            .iter()
            .filter(|id| {
                let mut owner = self.organization_owner(id);
                while let Some(parent) = owner {
                    if wanted.contains(parent) {
                        return false;
                    }
                    owner = self.organization_owner(parent);
                }
                true
            })
            .cloned()
            .collect();
        selected.sort_by_key(|id| index.ranks[id]);
        Ok(selected)
    }
    fn check_group_owner(&self, id: Option<&str>) -> Result<(), Status> {
        if let Some(id) = id {
            if self.get_parameter_group(id).is_none() {
                return Err(Status::error("MISSING_CDI_GROUP", id));
            }
            self.organization_unlocked(id)?;
        }
        Ok(())
    }
    fn check_part_owner(&self, id: Option<&str>) -> Result<(), Status> {
        if let Some(id) = id {
            if self.get_part(id).is_none() {
                return Err(Status::error("MISSING_PART", id));
            }
            self.organization_unlocked(id)?;
        }
        Ok(())
    }
    fn explicit_parameter_info(&self) -> DisplayInfo {
        let mut info = self.display_info().clone();
        let entries = info.parameters.get_or_insert_with(Vec::new);
        for id in self.parameter_order() {
            if !entries.iter().any(
                |e| matches!(e,CdiParameterEntry::Resolved { parameter_id,.. } if parameter_id==id),
            ) {
                entries.push(CdiParameterEntry::Resolved {
                    parameter_id: id.clone(),
                    group_id: None,
                    extensions: Default::default(),
                });
            }
        }
        info
    }
    pub fn plan_organization_edit(
        &self,
        edit: OrganizationEdit,
    ) -> Result<OrganizationPlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut plan = OrganizationPlan {
            baseline: self.snapshot_identity.clone(),
            revision: self.revision(),
            delta: Delta::default(),
            affected: vec![],
            copied: BTreeMap::new(),
            no_op: false,
            effects: OrganizationEffects {
                drawing_preserved: true,
                deformation_preserved: true,
                ..Default::default()
            },
        };
        match edit {
            OrganizationEdit::Move {
                ids,
                destination,
                before,
            } => self.plan_organization_move(&mut plan, &ids, &destination, &before)?,
            OrganizationEdit::CreateParameter {
                mut parameter,
                group_id,
                before,
            } => {
                fresh(self, &parameter.id)?;
                self.check_group_owner(group_id.as_deref())?;
                if parameter.runtime_id.is_empty() {
                    parameter.runtime_id = parameter.id.clone();
                }
                let status = self.validate_parameter(&parameter);
                if !status.is_ok() {
                    return Err(status);
                }
                let mut info = self.explicit_parameter_info();
                let entries = info.parameters.as_mut().unwrap();
                // Appending to a group does not require grouping the global CDI
                // array. Keep newly created parameters last in the public list,
                // as the existing creation/reveal flow expects.
                let at = if before.is_some() {
                    parameter_insertion(entries, group_id.as_deref(), &before)?
                } else {
                    entries.len()
                };
                entries.insert(
                    at,
                    CdiParameterEntry::Resolved {
                        parameter_id: parameter.id.clone(),
                        group_id,
                        extensions: Default::default(),
                    },
                );
                plan.affected.push(parameter.id.clone());
                plan.delta.parameters.push(parameter);
                plan.delta.groups = Some(info);
            }
            OrganizationEdit::CreateGroup { mut group, before } => {
                fresh(self, &group.id)?;
                self.check_group_owner(group.parent_id.as_deref())?;
                if group.runtime_id.is_empty() {
                    group.runtime_id = group.id.clone();
                }
                let mut info = self.display_info().clone();
                let groups = info.parameter_groups.get_or_insert_with(Vec::new);
                let at = group_insertion(groups, group.parent_id.as_deref(), &before)?;
                groups.insert(at, group.clone());
                plan.affected.push(group.id);
                plan.delta.groups = Some(info);
            }
            OrganizationEdit::CreatePart { mut part, before } => {
                fresh(self, &part.id)?;
                self.check_part_owner(
                    (!part.parent_id.is_empty()).then_some(part.parent_id.as_str()),
                )?;
                if part.runtime_id.is_empty() {
                    part.runtime_id = part.id.clone();
                }
                let status = self.validate_part(&part);
                if !status.is_ok() {
                    return Err(status);
                }
                let mut order = self.scene_organization_order();
                let at = self.scene_insertion(&order, &part.parent_id, &before)?;
                order.insert(at, part.id.clone());
                let mut hierarchy = self.hierarchy_order().clone();
                hierarchy.organization = order;
                plan.delta.hierarchy = Some(hierarchy);
                if self.draw_order_groups().is_some() {
                    let mut groups = crate::draw_order::resolved_groups(self);
                    groups.push(DrawOrderGroup {
                        owner: part.id.clone(),
                        items: vec![],
                        min_order: 0,
                        max_order: 0,
                    });
                    groups[0].items.push(part.id.clone());
                    plan.delta.drawing = Some(groups);
                }
                plan.affected.push(part.id.clone());
                plan.delta.parts.push(part);
            }
            OrganizationEdit::SetParameterGroup {
                parameter_id,
                group_id,
            } => {
                if self.get_parameter(&parameter_id).is_none() {
                    return Err(Status::error("MISSING_PARAMETER", parameter_id));
                }
                self.organization_unlocked(&parameter_id)?;
                self.check_group_owner(group_id.as_deref())?;
                if self.parameter_group_for(&parameter_id)!=group_id.as_deref() || !self.display_info().parameters.iter().flatten().any(|e|matches!(e,CdiParameterEntry::Resolved {parameter_id:id,..} if id==&parameter_id)) {
                    let mut info=if self.display_info().origin==DisplayInfoOrigin::Generated && self.display_info().parameters.is_none() {self.explicit_parameter_info()}else {self.display_info().clone()};
                    let entries=info.parameters.get_or_insert_with(Vec::new);
                    if let Some(e)=entries.iter_mut().find(|e|matches!(e,CdiParameterEntry::Resolved {parameter_id:id,..} if id==&parameter_id)) {set_group(e,group_id);}
                    else {entries.push(CdiParameterEntry::Resolved {parameter_id:parameter_id.clone(),group_id,extensions:Default::default()});}
                    plan.delta.groups=Some(info);
                }
                plan.affected.push(parameter_id);
            }
            OrganizationEdit::PatchGroup { expected, group } => {
                let old = self
                    .get_parameter_group(&group.id)
                    .ok_or_else(|| Status::error("MISSING_CDI_GROUP", &group.id))?;
                if *old != expected {
                    return Err(Status::error("STALE_FIELD", &group.id));
                }
                self.organization_unlocked(&group.id)?;
                if old.parent_id != group.parent_id {
                    self.plan_organization_move(
                        &mut plan,
                        &[group.id.clone()],
                        &OrganizationCollection::ParameterGroups {
                            parent_id: group.parent_id.clone(),
                        },
                        &None,
                    )?;
                }
                let info = plan
                    .delta
                    .groups
                    .get_or_insert_with(|| self.display_info().clone());
                *info
                    .parameter_groups
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|g| g.id == group.id)
                    .unwrap() = group.clone();
                plan.affected.push(group.id);
            }
            OrganizationEdit::Duplicate {
                ids,
                destination,
                before,
            } => self.plan_organization_copy(&mut plan, &ids, &destination, &before)?,
            OrganizationEdit::Delete { ids, policy } => {
                self.plan_organization_delete(&mut plan, &ids, policy)?
            }
        }
        if let Some(info) = &plan.delta.groups {
            if let Some(issue) = self
                .validate_display_info_value(info, &plan.delta.parameters, &plan.delta.parts)
                .into_iter()
                .next()
            {
                return Err(issue.status);
            }
        }
        plan.affected.sort();
        plan.affected.dedup();
        plan.no_op = !plan.delta.changed(self);
        Ok(plan)
    }
    fn plan_organization_move(
        &self,
        plan: &mut OrganizationPlan,
        ids: &[String],
        dest: &OrganizationCollection,
        before: &Option<String>,
    ) -> Result<(), Status> {
        let ids = self.organization_selection(ids)?;
        self.organization_subtree_unlocked(&ids)?;
        if before.as_ref().is_some_and(|id| ids.contains(id)) {
            if ids.len() == 1 && self.organization_collection(dest)?.contains(&ids[0]) {
                return Ok(());
            }
            return Err(Status::error("INVALID_PLACEMENT", "Anchor is selected"));
        }
        let mut members = self.organization_collection(dest)?;
        if matches!(dest, OrganizationCollection::PartMembers { part_id: None }) {
            if ids.iter().all(|id| self.get_part(id).is_some()) {
                members.retain(|id| self.get_part(id).is_some());
            } else if ids.iter().all(|id| self.get_part(id).is_none()) {
                members.retain(|id| self.get_part(id).is_none());
            }
        }
        if ids.iter().all(|id| members.contains(id)) {
            let mut next: Vec<_> = members
                .iter()
                .filter(|id| !ids.contains(id))
                .cloned()
                .collect();
            let at = if let Some(anchor) = before {
                next.iter()
                    .position(|id| id == anchor)
                    .ok_or_else(|| Status::error("INVALID_PLACEMENT", anchor))?
            } else {
                next.len()
            };
            next.splice(at..at, ids.iter().cloned());
            if next == members {
                return Ok(());
            }
        }
        match dest {
            OrganizationCollection::Parameters { group_id } => {
                self.check_group_owner(group_id.as_deref())?;
                if ids.iter().any(|id| self.get_parameter(id).is_none()) {
                    return Err(Status::error(
                        "WRONG_ORGANIZATION_COLLECTION",
                        "Expected parameters",
                    ));
                }
                let mut info = self.explicit_parameter_info();
                let entries = info.parameters.as_mut().unwrap();
                let original = entries.clone();
                let mut resolved: Vec<_> = entries
                    .iter()
                    .filter(|e| matches!(e, CdiParameterEntry::Resolved { .. }))
                    .cloned()
                    .collect();
                let mut moving = Vec::new();
                for id in &ids {
                    let at=resolved.iter().position(|e|matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if parameter_id==id)).unwrap();
                    let mut e = resolved.remove(at);
                    set_group(&mut e, group_id.clone());
                    moving.push(e);
                }
                let at = parameter_insertion(&resolved, group_id.as_deref(), before)?;
                resolved.splice(at..at, moving);
                let mut resolved = resolved.into_iter();
                for e in entries.iter_mut() {
                    if matches!(e, CdiParameterEntry::Resolved { .. }) {
                        *e = resolved.next().unwrap();
                    }
                }
                entries.extend(resolved);
                if *entries != original {
                    plan.delta.groups = Some(info);
                }
            }
            OrganizationCollection::ParameterGroups { parent_id } => {
                self.check_group_owner(parent_id.as_deref())?;
                for id in &ids {
                    if self.get_parameter_group(id).is_none() {
                        return Err(Status::error("WRONG_ORGANIZATION_COLLECTION", id));
                    }
                    let mut current = parent_id.as_deref();
                    while let Some(p) = current {
                        if p == id {
                            return Err(Status::error("CDI_GROUP_CYCLE", id));
                        }
                        current = self
                            .get_parameter_group(p)
                            .and_then(|g| g.parent_id.as_deref());
                    }
                }
                let mut info = self.display_info().clone();
                let groups = info.parameter_groups.as_mut().unwrap();
                let original = groups.clone();
                let mut moving = vec![];
                for id in &ids {
                    let i = groups.iter().position(|g| g.id == *id).unwrap();
                    let mut g = groups.remove(i);
                    g.parent_id = parent_id.clone();
                    moving.push(g);
                }
                let at = group_insertion(groups, parent_id.as_deref(), before)?;
                groups.splice(at..at, moving);
                if *groups != original {
                    plan.delta.groups = Some(info);
                }
            }
            OrganizationCollection::PartMembers { part_id } => self.plan_scene_organization_move(
                plan,
                &ids,
                part_id.as_deref().unwrap_or(""),
                before,
            )?,
        }
        plan.affected.extend(self.organization_subtree(&ids)?);
        Ok(())
    }
    pub fn apply_organization_plan(&mut self, plan: &OrganizationPlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.baseline) || self.revision() != plan.revision
        {
            return self.failed(Status::error("STALE_PLAN", "Replan organization"));
        }
        if !plan.delta.changed(self) {
            return self.failed(Status::ok());
        }
        let runtime = plan.delta.runtime_changed(self);
        plan.delta.install(self);
        self.changed(
            if runtime {
                ChangeKind::Structure
            } else {
                ChangeKind::Metadata
            },
            if runtime {
                self.mesh_order().to_vec()
            } else {
                vec![]
            },
            plan.affected.clone(),
        )
    }
}
fn fresh(d: &Document, id: &str) -> Result<(), Status> {
    if !valid_uuid(id) {
        Err(Status::error("INVALID_ID", id))
    } else if d.contains_id(id) {
        Err(Status::error("DUPLICATE_ID", id))
    } else {
        Ok(())
    }
}
fn set_group(e: &mut CdiParameterEntry, group: Option<String>) {
    match e {
        CdiParameterEntry::Resolved { group_id, .. }
        | CdiParameterEntry::Unresolved { group_id, .. } => *group_id = group,
    }
}
fn group_insertion(
    groups: &[CdiParameterGroup],
    parent: Option<&str>,
    before: &Option<String>,
) -> Result<usize, Status> {
    if let Some(id) = before {
        groups
            .iter()
            .position(|g| g.id == *id && g.parent_id.as_deref() == parent)
            .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))
    } else {
        Ok(groups
            .iter()
            .rposition(|g| g.parent_id.as_deref() == parent)
            .map_or(groups.len(), |i| i + 1))
    }
}
fn parameter_insertion(
    entries: &[CdiParameterEntry],
    group: Option<&str>,
    before: &Option<String>,
) -> Result<usize, Status> {
    if let Some(id) = before {
        entries
            .iter()
            .position(|e| {
                matches!(e,CdiParameterEntry::Resolved { parameter_id,.. } if parameter_id==id)
                    && e.group_id() == group
            })
            .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))
    } else {
        Ok(entries
            .iter()
            .rposition(|e| e.group_id() == group)
            .map_or(entries.len(), |i| i + 1))
    }
}
