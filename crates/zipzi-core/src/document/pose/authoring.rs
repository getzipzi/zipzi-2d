//! Pose owns groups, entries and reference rows. Part targets remain shared.
use super::*;
use std::sync::Arc;

pub use zipzi_contract::document::{PoseCollection, PoseEdit, PoseFields};

#[derive(Debug, Clone)]
pub struct PosePlan {
    snapshot: Arc<()>,
    revision: u64,
    replacement: Option<PoseAsset>,
    patch: Option<(String, PoseFields)>,
    affected: Vec<String>,
}
impl PosePlan {
    pub fn preserves_identities(&self) -> bool {
        self.patch.is_some()
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
}
impl Document {
    pub fn get_pose_group(&self, id: &str) -> Option<&PoseGroup> {
        self.pose()?.groups.iter().find(|g| g.id == id)
    }
    pub fn get_pose_entry(&self, id: &str) -> Option<(&PoseGroup, &PoseEntry)> {
        self.pose()?
            .groups
            .iter()
            .find_map(|g| g.entries.iter().find(|e| e.id == id).map(|e| (g, e)))
    }
    pub fn get_pose_link(&self, id: &str) -> Option<(&PoseEntry, &PoseLink)> {
        self.pose()?
            .groups
            .iter()
            .flat_map(|g| &g.entries)
            .find_map(|e| {
                e.links
                    .iter()
                    .flatten()
                    .find(|l| l.id == id)
                    .map(|l| (e, l))
            })
    }
    pub fn pose_owner(&self, id: &str) -> Option<&str> {
        if self.get_pose_group(id).is_some() {
            return Some(&self.pose()?.id);
        }
        if let Some((g, _)) = self.get_pose_entry(id) {
            return Some(&g.id);
        }
        self.get_pose_link(id).map(|(e, _)| e.id.as_str())
    }
    pub fn pose_fields(&self, id: &str) -> Option<PoseFields> {
        if let Some(p) = self.pose().filter(|p| p.id == id) {
            return Some(PoseFields::Asset { fade_in: p.fade_in });
        }
        if let Some(g) = self.get_pose_group(id) {
            return Some(PoseFields::Group {
                name: g.name.clone(),
            });
        }
        if let Some((_, e)) = self.get_pose_entry(id) {
            return Some(PoseFields::Entry {
                part: e.part.clone(),
            });
        }
        self.get_pose_link(id).map(|(_, l)| PoseFields::Link {
            target: l.target.clone(),
        })
    }
    pub fn pose_unlocked(&self, id: &str) -> Result<(), Status> {
        if self.pose_fields(id).is_none() {
            return Err(Status::error("MISSING_POSE_OBJECT", id));
        }
        let mut current = Some(id);
        while let Some(id) = current {
            if self.editor_state().get(id).locked {
                return Err(Status::error("OBJECT_LOCKED", id));
            }
            current = self.pose_owner(id);
        }
        Ok(())
    }
    pub fn pose_subtree_unlocked(&self, id: &str) -> Result<(), Status> {
        self.pose_unlocked(id)?;
        let check = |id: &str| {
            if self.editor_state().get(id).locked {
                Err(Status::error("OBJECT_LOCKED", id))
            } else {
                Ok(())
            }
        };
        let entry = |e: &PoseEntry| -> Result<(), Status> {
            check(&e.id)?;
            for link in e.links.iter().flatten() {
                check(&link.id)?;
            }
            Ok(())
        };
        if let Some(group) = self.get_pose_group(id) {
            for e in &group.entries {
                entry(e)?;
            }
        } else if let Some((_, e)) = self.get_pose_entry(id) {
            entry(e)?;
        } else if self.pose().is_some_and(|p| p.id == id) {
            for g in &self.pose().unwrap().groups {
                check(&g.id)?;
                for e in &g.entries {
                    entry(e)?;
                }
            }
        }
        Ok(())
    }
    pub(in crate::document) fn validate_pose_replacement(
        &self,
        candidate: Option<&PoseAsset>,
    ) -> Result<(), Status> {
        if let Some(p) = candidate {
            let s = self.validate_pose(p);
            if !s.is_ok() {
                return Err(s);
            }
        }
        if let Some(old) = self.pose() {
            for id in old
                .entity_ids()
                .filter(|id| self.editor_state().get(id).locked)
            {
                if node(old, id) != candidate.and_then(|p| node(p, id)) {
                    return Err(Status::error("OBJECT_LOCKED", id));
                }
            }
        }
        Ok(())
    }
    fn validate_pose_patch(&self, id: &str, fields: &PoseFields) -> Result<(), Status> {
        self.pose_unlocked(id)?;
        match (self.pose_fields(id).unwrap(), fields) {
            (PoseFields::Asset { .. }, PoseFields::Asset { fade_in }) => {
                if fade_in.is_some_and(|v| !v.is_finite() || v < 0.) {
                    return Err(Status::error("INVALID_POSE_FADE", id));
                }
            }
            (PoseFields::Group { .. }, PoseFields::Group { name }) => {
                if name.contains('\0') {
                    return Err(Status::error("INVALID_POSE_GROUP_NAME", id));
                }
            }
            (PoseFields::Entry { .. }, PoseFields::Entry { part }) => {
                let runtime = self.validate_pose_ref(part)?;
                let (g, _) = self.get_pose_entry(id).unwrap();
                if runtime.is_some()
                    && g.entries
                        .iter()
                        .filter(|e| e.id != id)
                        .any(|e| self.validate_pose_ref(&e.part).ok() == Some(runtime.clone()))
                {
                    return Err(Status::error("DUPLICATE_POSE_PART", id));
                }
            }
            (PoseFields::Link { .. }, PoseFields::Link { target }) => {
                let runtime = self.validate_pose_ref(target)?;
                let (e, _) = self.get_pose_link(id).unwrap();
                if runtime.is_some()
                    && e.links
                        .iter()
                        .flatten()
                        .filter(|l| l.id != id)
                        .any(|l| self.validate_pose_ref(&l.target).ok() == Some(runtime.clone()))
                {
                    return Err(Status::error("DUPLICATE_POSE_LINK", id));
                }
            }
            _ => return Err(Status::error("WRONG_POSE_FIELDS", id)),
        }
        Ok(())
    }
    pub fn plan_pose_edit(&self, edit: PoseEdit) -> Result<PosePlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut plan = PosePlan {
            snapshot: self.snapshot_identity.clone(),
            revision: self.revision(),
            replacement: None,
            patch: None,
            affected: vec![],
        };
        if let PoseEdit::Patch {
            id,
            expected,
            fields,
        } = edit
        {
            if self.pose_fields(&id).as_ref() != Some(&expected) {
                return Err(Status::error("STALE_FIELD", id));
            }
            self.validate_pose_patch(&id, &fields)?;
            plan.affected.push(id.clone());
            plan.patch = Some((id, fields));
            return Ok(plan);
        }
        let fresh = |id: &str| -> Result<(), Status> {
            if !super::super::valid_uuid(id) {
                return Err(Status::error("INVALID_POSE_IDENTITY", id));
            }
            if self.contains_id(id) {
                return Err(Status::error("DUPLICATE_ID", id));
            }
            Ok(())
        };
        let mut asset = match &edit {
            PoseEdit::Create { id } => {
                if self.pose().is_some() {
                    return Err(Status::error("POSE_EXISTS", "Pose is a singleton"));
                }
                fresh(id)?;
                PoseAsset::empty(id.clone())
            }
            PoseEdit::CreateGroup { pose_id, .. } if self.pose().is_none() => {
                fresh(pose_id)?;
                PoseAsset::empty(pose_id.clone())
            }
            _ => self
                .pose()
                .cloned()
                .ok_or_else(|| Status::error("MISSING_POSE", "Create a group first"))?,
        };
        match &edit {
            PoseEdit::Create { .. } => {}
            PoseEdit::CreateGroup {
                pose_id,
                id,
                name,
                before,
            } => {
                if *pose_id != asset.id {
                    return Err(Status::error("WRONG_POSE_OWNER", pose_id));
                }
                fresh(id)?;
                let i = index(asset.groups.iter().map(|g| g.id.as_str()), before)?;
                asset.groups.insert(
                    i,
                    PoseGroup {
                        id: id.clone(),
                        name: name.clone(),
                        entries: Default::default(),
                    },
                );
            }
            PoseEdit::CreateEntry {
                group_id,
                id,
                before,
            } => {
                fresh(id)?;
                let g = asset
                    .groups
                    .iter_mut()
                    .find(|g| g.id == *group_id)
                    .ok_or_else(|| Status::error("MISSING_POSE_GROUP", group_id))?;
                let i = index(g.entries.iter().map(|e| e.id.as_str()), before)?;
                g.entries.insert(
                    i,
                    PoseEntry {
                        id: id.clone(),
                        part: PosePartRef::Unspecified,
                        links: None,
                        extensions: BTreeMap::new(),
                    },
                );
            }
            PoseEdit::CreateLink {
                entry_id,
                id,
                before,
            } => {
                fresh(id)?;
                let e = asset
                    .groups
                    .iter_mut()
                    .flat_map(|g| &mut g.entries)
                    .find(|e| e.id == *entry_id)
                    .ok_or_else(|| Status::error("MISSING_POSE_ENTRY", entry_id))?;
                let links = e.links.get_or_insert_with(Default::default);
                let i = index(links.iter().map(|l| l.id.as_str()), before)?;
                links.insert(
                    i,
                    PoseLink {
                        id: id.clone(),
                        target: PosePartRef::Unspecified,
                    },
                );
            }
            PoseEdit::Move {
                ids,
                destination,
                before,
            }
            | PoseEdit::Duplicate {
                ids,
                destination,
                before,
            } => {
                let ids = self.pose_selection(ids)?;
                let copy = matches!(edit, PoseEdit::Duplicate { .. });
                if !copy && before.as_ref().is_some_and(|a| ids.contains(a)) {
                    return Err(Status::error("INVALID_PLACEMENT", "Anchor is selected"));
                }
                for id in &ids {
                    self.pose_subtree_unlocked(id)?;
                }
                match destination {
                    PoseCollection::Groups => {
                        let mut moving = vec![];
                        for id in &ids {
                            let mut g = self
                                .get_pose_group(id)
                                .cloned()
                                .ok_or_else(|| Status::error("WRONG_POSE_COLLECTION", id))?;
                            if copy {
                                g.id = new_id();
                                for e in &mut g.entries {
                                    copy_entry(e);
                                }
                            }
                            moving.push(g);
                        }
                        if !copy {
                            asset.groups.retain(|g| !ids.contains(&g.id));
                        }
                        let i = index(asset.groups.iter().map(|g| g.id.as_str()), before)?;
                        asset.groups.splice(i..i, moving);
                    }
                    PoseCollection::Entries { group_id } => {
                        self.pose_unlocked(group_id)?;
                        let mut moving = vec![];
                        for id in &ids {
                            let mut e = self
                                .get_pose_entry(id)
                                .map(|(_, e)| e.clone())
                                .ok_or_else(|| Status::error("WRONG_POSE_COLLECTION", id))?;
                            if copy {
                                copy_entry(&mut e);
                            }
                            moving.push(e);
                        }
                        if !copy {
                            for g in &mut asset.groups {
                                g.entries.retain(|e| !ids.contains(&e.id));
                            }
                        }
                        let g = asset
                            .groups
                            .iter_mut()
                            .find(|g| g.id == *group_id)
                            .ok_or_else(|| Status::error("MISSING_POSE_GROUP", group_id))?;
                        let i = index(g.entries.iter().map(|e| e.id.as_str()), before)?;
                        g.entries.splice(i..i, moving);
                    }
                    PoseCollection::Links { entry_id } => {
                        self.pose_unlocked(entry_id)?;
                        let mut moving = vec![];
                        for id in &ids {
                            let mut l = self
                                .get_pose_link(id)
                                .map(|(_, l)| l.clone())
                                .ok_or_else(|| Status::error("WRONG_POSE_COLLECTION", id))?;
                            if copy {
                                l.id = new_id();
                            }
                            moving.push(l);
                        }
                        if !copy {
                            for e in asset.groups.iter_mut().flat_map(|g| &mut g.entries) {
                                if let Some(ls) = &mut e.links {
                                    ls.retain(|l| !ids.contains(&l.id));
                                }
                            }
                        }
                        let e = asset
                            .groups
                            .iter_mut()
                            .flat_map(|g| &mut g.entries)
                            .find(|e| e.id == *entry_id)
                            .ok_or_else(|| Status::error("MISSING_POSE_ENTRY", entry_id))?;
                        let ls = e.links.get_or_insert_with(Default::default);
                        let i = index(ls.iter().map(|l| l.id.as_str()), before)?;
                        ls.splice(i..i, moving);
                    }
                }
            }
            PoseEdit::Delete { ids } => {
                let ids = self.pose_selection(ids)?;
                for id in &ids {
                    self.pose_subtree_unlocked(id)?;
                }
                if ids.contains(&asset.id) {
                    plan.affected = self
                        .pose()
                        .unwrap()
                        .entity_ids()
                        .map(str::to_owned)
                        .collect();
                    self.validate_pose_replacement(None)?;
                    return Ok(plan);
                }
                asset.groups.retain(|g| !ids.contains(&g.id));
                for g in &mut asset.groups {
                    g.entries.retain(|e| !ids.contains(&e.id));
                    for e in &mut g.entries {
                        if let Some(ls) = &mut e.links {
                            ls.retain(|l| !ids.contains(&l.id));
                        }
                    }
                }
            }
            PoseEdit::DissolveGroup {
                id,
                destination,
                before,
            } => {
                if id == destination {
                    return Err(Status::error("INVALID_DESTINATION", id));
                }
                self.pose_subtree_unlocked(id)?;
                self.pose_unlocked(destination)?;
                let i = asset
                    .groups
                    .iter()
                    .position(|g| g.id == *id)
                    .ok_or_else(|| Status::error("MISSING_POSE_GROUP", id))?;
                let entries = asset.groups.remove(i).entries;
                let g = asset
                    .groups
                    .iter_mut()
                    .find(|g| g.id == *destination)
                    .ok_or_else(|| Status::error("MISSING_POSE_GROUP", destination))?;
                let i = index(g.entries.iter().map(|e| e.id.as_str()), before)?;
                g.entries.splice(i..i, entries);
            }
            PoseEdit::Patch { .. } => unreachable!(),
        }
        self.validate_pose_replacement(Some(&asset))?;
        plan.affected = self
            .pose()
            .into_iter()
            .flat_map(|p| p.entity_ids())
            .chain(asset.entity_ids())
            .map(str::to_owned)
            .collect();
        plan.affected.sort();
        plan.affected.dedup();
        plan.replacement = Some(asset);
        Ok(plan)
    }
    fn pose_selection(&self, ids: &[String]) -> Result<Vec<String>, Status> {
        let wanted: HashSet<_> = ids.iter().map(String::as_str).collect();
        if ids.is_empty() || wanted.len() != ids.len() {
            return Err(Status::error(
                "INVALID_SELECTION",
                "Select distinct Pose objects",
            ));
        }
        for id in ids {
            if self.pose_fields(id).is_none() {
                return Err(Status::error("MISSING_POSE_OBJECT", id));
            }
        }
        Ok(self
            .pose()
            .unwrap()
            .entity_ids()
            .filter(|id| wanted.contains(*id))
            .filter(|id| {
                let mut current = self.pose_owner(id);
                while let Some(owner) = current {
                    if wanted.contains(owner) {
                        return false;
                    }
                    current = self.pose_owner(owner);
                }
                true
            })
            .map(str::to_owned)
            .collect())
    }
    pub fn apply_pose_plan(&mut self, plan: &PosePlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.snapshot) || self.revision() != plan.revision
        {
            return self.failed(Status::error("STALE_PLAN", "Replan Pose"));
        }
        if let Some((id, fields)) = &plan.patch {
            if self.pose_fields(id).as_ref() == Some(fields) {
                return self.failed(Status::ok());
            }
            if let Err(s) = self.validate_pose_patch(id, fields) {
                return self.failed(s);
            }
            let p = Arc::make_mut(self.content.pose.as_mut().unwrap());
            match fields {
                PoseFields::Asset { fade_in } => p.fade_in = *fade_in,
                PoseFields::Group { name } => {
                    p.groups.iter_mut().find(|g| g.id == *id).unwrap().name = name.clone()
                }
                PoseFields::Entry { part } => {
                    let (gi, ei) = p
                        .groups
                        .iter()
                        .enumerate()
                        .find_map(|(gi, g)| {
                            g.entries
                                .iter()
                                .position(|e| e.id == *id)
                                .map(|ei| (gi, ei))
                        })
                        .unwrap();
                    p.groups[gi].entries[ei].part = part.clone();
                }
                PoseFields::Link { target } => {
                    let (gi, ei, li) = p
                        .groups
                        .iter()
                        .enumerate()
                        .find_map(|(gi, g)| {
                            g.entries.iter().enumerate().find_map(|(ei, e)| {
                                e.links
                                    .as_ref()
                                    .and_then(|ls| ls.iter().position(|l| l.id == *id))
                                    .map(|li| (gi, ei, li))
                            })
                        })
                        .unwrap();
                    p.groups[gi].entries[ei].links.as_mut().unwrap()[li].target = target.clone();
                }
            }
        } else {
            if self.pose() == plan.replacement.as_ref() {
                return self.failed(Status::ok());
            }
            if let Err(s) = self.validate_pose_replacement(plan.replacement.as_ref()) {
                return self.failed(s);
            }
            self.install_pose(plan.replacement.clone());
        }
        self.changed(ChangeKind::Metadata, vec![], plan.affected.clone())
    }
}
// Include the storage position and entire owned subtree: locking a container also
// protects its semantic order and descendants, including direct set_pose callers.
#[derive(PartialEq)]
enum Node<'a> {
    Asset(&'a PoseAsset),
    Group(usize, &'a PoseGroup),
    Entry(&'a str, usize, &'a PoseEntry),
    Link(&'a str, usize, &'a PoseLink),
}
fn node<'a>(p: &'a PoseAsset, id: &str) -> Option<Node<'a>> {
    if p.id == id {
        return Some(Node::Asset(p));
    }
    for (i, g) in p.groups.iter().enumerate() {
        if g.id == id {
            return Some(Node::Group(i, g));
        }
        for (j, e) in g.entries.iter().enumerate() {
            if e.id == id {
                return Some(Node::Entry(&g.id, j, e));
            }
            for (k, l) in e.links.iter().flatten().enumerate() {
                if l.id == id {
                    return Some(Node::Link(&e.id, k, l));
                }
            }
        }
    }
    None
}
fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn copy_entry(e: &mut PoseEntry) {
    e.id = new_id();
    for l in e.links.iter_mut().flatten() {
        l.id = new_id();
    }
}
fn index<'a>(ids: impl Iterator<Item = &'a str>, before: &Option<String>) -> Result<usize, Status> {
    let mut len = 0;
    for (i, id) in ids.enumerate() {
        if before.as_deref() == Some(id) {
            return Ok(i);
        }
        len = i + 1;
    }
    if let Some(id) = before {
        Err(Status::error("INVALID_PLACEMENT", id))
    } else {
        Ok(len)
    }
}
