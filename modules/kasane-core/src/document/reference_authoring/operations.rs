use super::*;
#[derive(Clone)]
enum Node {
    Group(ModelParameterGroup),
    Area(ModelHitArea),
    Set(CdiCombinedSet),
    ModelMember(ReferenceEntry<ModelTargetRef>),
    CombinedMember(ReferenceEntry<CdiParameterRef>),
}
impl Node {
    fn id(&self) -> &str {
        match self {
            Self::Group(v) => &v.id,
            Self::Area(v) => &v.id,
            Self::Set(v) => &v.id,
            Self::ModelMember(v) => &v.id,
            Self::CombinedMember(v) => &v.id,
        }
    }
    fn copy(&mut self, map: &mut BTreeMap<String, String>) {
        let mut replace = |id: &mut String| {
            let new = uuid::Uuid::new_v4().to_string();
            map.insert(id.clone(), new.clone());
            *id = new;
        };
        match self {
            Self::Group(g) => {
                replace(&mut g.id);
                for p in &mut g.parameters {
                    replace(&mut p.id);
                }
            }
            Self::Set(s) => {
                replace(&mut s.id);
                for p in &mut s.members {
                    replace(&mut p.id);
                }
            }
            Self::Area(v) => replace(&mut v.id),
            Self::ModelMember(v) => replace(&mut v.id),
            Self::CombinedMember(v) => replace(&mut v.id),
        }
    }
}
struct Working {
    model: Model3Settings,
    display: DisplayInfo,
}
impl Working {
    fn get(&self, id: &str) -> Option<Node> {
        for g in self.model.groups.iter().flatten() {
            if g.id == id {
                return Some(Node::Group(g.clone()));
            }
            for p in &g.parameters {
                if p.id == id {
                    return Some(Node::ModelMember(p.clone()));
                }
            }
        }
        if let Some(a) = self.model.hit_areas.iter().flatten().find(|a| a.id == id) {
            return Some(Node::Area(a.clone()));
        }
        for s in self.display.combined_parameters.iter().flatten() {
            if s.id == id {
                return Some(Node::Set(s.clone()));
            }
            for p in &s.members {
                if p.id == id {
                    return Some(Node::CombinedMember(p.clone()));
                }
            }
        }
        None
    }
    fn remove(&mut self, ids: &HashSet<String>) {
        if let Some(groups) = &mut self.model.groups {
            groups.retain(|g| !ids.contains(&g.id));
            for g in groups {
                g.parameters.retain(|p| !ids.contains(&p.id));
            }
        }
        if let Some(areas) = &mut self.model.hit_areas {
            areas.retain(|a| !ids.contains(&a.id));
        }
        if let Some(sets) = &mut self.display.combined_parameters {
            sets.retain(|s| !ids.contains(&s.id));
            for s in sets {
                s.members.retain(|p| !ids.contains(&p.id));
            }
        }
    }
    fn insert(
        &mut self,
        node: Node,
        destination: &ReferenceCollection,
        before: &Option<String>,
    ) -> Result<(), Status> {
        macro_rules! put {
            ($list:expr,$value:expr) => {{
                let list = $list;
                let at = match before {
                    Some(id) => list
                        .iter()
                        .position(|v| v.id == *id)
                        .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))?,
                    None => list.len(),
                };
                list.insert(at, $value);
                Ok(())
            }};
        }
        match (node, destination) {
            (Node::Group(v), ReferenceCollection::ModelParameterGroups) => {
                put!(self.model.groups.get_or_insert_with(Vec::new), v)
            }
            (Node::Area(v), ReferenceCollection::ModelHitAreas) => {
                put!(self.model.hit_areas.get_or_insert_with(Vec::new), v)
            }
            (Node::Set(v), ReferenceCollection::CdiCombinedSets) => put!(
                self.display
                    .combined_parameters
                    .get_or_insert_with(Vec::new),
                v
            ),
            (Node::ModelMember(v), ReferenceCollection::ModelParameterMembers { group_id }) => {
                put!(
                    &mut self
                        .model
                        .groups
                        .iter_mut()
                        .flatten()
                        .find(|g| g.id == *group_id)
                        .ok_or_else(|| Status::error("MISSING_MODEL_GROUP", group_id))?
                        .parameters,
                    v
                )
            }
            (Node::CombinedMember(v), ReferenceCollection::CdiCombinedMembers { set_id }) => put!(
                &mut self
                    .display
                    .combined_parameters
                    .iter_mut()
                    .flatten()
                    .find(|g| g.id == *set_id)
                    .ok_or_else(|| Status::error("MISSING_CDI_SET", set_id))?
                    .members,
                v
            ),
            _ => Err(Status::error(
                "WRONG_REFERENCE_COLLECTION",
                "Reference kinds cannot be converted by a collection move",
            )),
        }
    }
}
impl Document {
    pub(super) fn plan_reference_operation(
        &self,
        p: &mut ReferencePlan,
        edit: ReferenceEdit,
    ) -> Result<(), Status> {
        if let ReferenceEdit::Patch {
            id,
            expected,
            fields,
        } = edit
        {
            self.reference_unlocked(&id)?;
            if self.reference_fields(&id)? != expected {
                return Err(Status::error("STALE_FIELD", &id));
            }
            if std::mem::discriminant(&expected) != std::mem::discriminant(&fields) {
                return Err(Status::error("WRONG_REFERENCE_FIELDS", &id));
            }
            self.validate_reference_fields(&id, &fields)?;
            if expected != fields {
                p.patch = Some((id.clone(), fields));
                p.domain.affected.push(id);
            }
            return Ok(());
        }
        if let ReferenceEdit::ReplaceSelection {
            owner,
            field,
            expected,
            targets,
        } = edit
        {
            self.authoring_unlocked(&owner)?;
            if self.reference_selection(&owner, field)? != expected {
                return Err(Status::error("STALE_REFERENCE_SELECTION", &owner));
            }
            if targets.iter().collect::<HashSet<_>>().len() != targets.len() {
                return Err(Status::error("DUPLICATE_REFERENCE_TARGET", &owner));
            }
            if expected == targets {
                return Ok(());
            }
            match field {
                ReferenceSelectionField::MeshMasks => {
                    let mut m = self.get_mesh(&owner).unwrap().clone();
                    m.masks = targets;
                    p.domain.delta.meshes.push(m);
                }
                ReferenceSelectionField::OffscreenMasks => {
                    let mut os = self.get_offscreen(&owner).unwrap().clone();
                    os.masks = targets;
                    p.domain.delta.offscreens.push(os);
                }
                ReferenceSelectionField::BlendConstraints => {
                    let mut b = self.get_blend_binding(&owner).unwrap().clone();
                    b.constraint_ids = targets;
                    p.domain.delta.blends.push(b);
                }
            }
            p.runtime = true;
            p.domain.affected.push(owner);
            return Ok(());
        }
        let copying = matches!(&edit, ReferenceEdit::Duplicate { .. });
        let mut w = Working {
            model: self.model3_settings().clone(),
            display: self.display_info().clone(),
        };
        match edit {
            ReferenceEdit::CreateModelParameterGroup { id, name, before } => {
                self.fresh_reference(&id)?;
                w.insert(
                    Node::Group(ModelParameterGroup {
                        id: id.clone(),
                        name: if name.is_empty() {
                            "Parameter Group".into()
                        } else {
                            name
                        },
                        parameters: vec![],
                        extensions: Default::default(),
                    }),
                    &ReferenceCollection::ModelParameterGroups,
                    &before,
                )?;
                p.domain.affected.push(id);
            }
            ReferenceEdit::CreateModelHitArea { id, name, before } => {
                self.fresh_reference(&id)?;
                w.insert(
                    Node::Area(ModelHitArea {
                        id: id.clone(),
                        name: if name.is_empty() {
                            "Hit Area".into()
                        } else {
                            name
                        },
                        mesh: ModelTargetRef::Unspecified,
                        extensions: Default::default(),
                    }),
                    &ReferenceCollection::ModelHitAreas,
                    &before,
                )?;
                p.domain.affected.push(id);
            }
            ReferenceEdit::CreateCdiCombinedSet { id, before } => {
                self.fresh_reference(&id)?;
                w.insert(
                    Node::Set(CdiCombinedSet {
                        id: id.clone(),
                        members: vec![],
                    }),
                    &ReferenceCollection::CdiCombinedSets,
                    &before,
                )?;
                p.domain.affected.push(id);
            }
            ReferenceEdit::CreateMember {
                id,
                collection,
                before,
            } => {
                self.fresh_reference(&id)?;
                self.reference_collection_unlocked(&collection)?;
                let node = match &collection {
                    ReferenceCollection::ModelParameterMembers { .. } => {
                        Node::ModelMember(ReferenceEntry {
                            id: id.clone(),
                            target: ModelTargetRef::Unspecified,
                        })
                    }
                    ReferenceCollection::CdiCombinedMembers { .. } => {
                        Node::CombinedMember(ReferenceEntry {
                            id: id.clone(),
                            target: CdiParameterRef::Unspecified,
                        })
                    }
                    _ => return Err(Status::error("NOT_A_REFERENCE_MEMBER_COLLECTION", &id)),
                };
                w.insert(node, &collection, &before)?;
                p.domain.affected.push(id);
            }
            ReferenceEdit::Move {
                ids,
                destination,
                before,
            }
            | ReferenceEdit::Duplicate {
                ids,
                destination,
                before,
            } => {
                let copy = copying;
                self.reference_collection_unlocked(&destination)?;
                let selected = self.reference_selection_roots(&ids)?;
                for id in &selected {
                    self.reference_unlocked(id)?;
                }
                if !copy && before.as_ref().is_some_and(|a| selected.contains(a)) {
                    let destination_ids = self.reference_collection(&destination)?;
                    if selected.iter().all(|id| destination_ids.contains(id)) {
                        return Ok(());
                    }
                    return Err(Status::error(
                        "INVALID_PLACEMENT",
                        "A selected anchor is only valid for a same-owner move",
                    ));
                }
                let mut nodes = selected
                    .iter()
                    .map(|id| {
                        w.get(id)
                            .ok_or_else(|| Status::error("MISSING_REFERENCE_OBJECT", id))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if copy {
                    for node in &mut nodes {
                        node.copy(&mut p.domain.copied);
                    }
                } else {
                    w.remove(&selected.iter().cloned().collect());
                }
                for node in nodes {
                    p.domain.affected.push(node.id().into());
                    w.insert(node, &destination, &before)?;
                }
                p.domain.affected.extend(selected);
            }
            ReferenceEdit::Delete { ids } => {
                let selected = self.reference_selection_roots(&ids)?;
                for id in &selected {
                    self.reference_unlocked(id)?;
                }
                let closure = self
                    .reference_identities()
                    .iter()
                    .filter(|i| {
                        selected.contains(&i.id)
                            || i.owner.as_ref().is_some_and(|o| selected.contains(o))
                    })
                    .map(|i| i.id.clone())
                    .collect::<Vec<_>>();
                w.remove(&selected.into_iter().collect());
                p.domain.affected.extend(closure.clone());
                p.domain.delta.removed = closure;
            }
            ReferenceEdit::Dissolve {
                id,
                destination,
                before,
            } => {
                self.reference_unlocked(&id)?;
                self.reference_unlocked(&destination)?;
                if id == destination {
                    return Err(Status::error("SAME_REFERENCE_OWNER", &id));
                }
                let (members, collection) = match w.get(&id) {
                    Some(Node::Group(g)) => (
                        g.parameters
                            .into_iter()
                            .map(Node::ModelMember)
                            .collect::<Vec<_>>(),
                        ReferenceCollection::ModelParameterMembers {
                            group_id: destination.clone(),
                        },
                    ),
                    Some(Node::Set(s)) => (
                        s.members
                            .into_iter()
                            .map(Node::CombinedMember)
                            .collect::<Vec<_>>(),
                        ReferenceCollection::CdiCombinedMembers {
                            set_id: destination.clone(),
                        },
                    ),
                    _ => return Err(Status::error("REFERENCE_CANNOT_DISSOLVE", &id)),
                };
                self.reference_collection_unlocked(&collection)?;
                w.remove(&[id.clone()].into());
                for node in members {
                    p.domain.affected.push(node.id().into());
                    w.insert(node, &collection, &before)?;
                }
                p.domain.delta.removed.push(id.clone());
                p.domain.affected.extend([id, destination]);
            }
            _ => unreachable!(),
        }
        if w.model != *self.model3_settings() {
            p.domain.delta.model3 = Some(w.model);
        }
        if w.display != *self.display_info() {
            p.domain.delta.groups = Some(w.display);
        }
        Ok(())
    }
    fn fresh_reference(&self, id: &str) -> Result<(), Status> {
        if !valid_uuid(id) {
            Err(Status::error("INVALID_ID", id))
        } else if self.contains_id(id) {
            Err(Status::error("DUPLICATE_ID", id))
        } else {
            Ok(())
        }
    }
    pub fn reference_collection_unlocked(&self, c: &ReferenceCollection) -> Result<(), Status> {
        self.reference_collection(c)?;
        match c {
            ReferenceCollection::ModelParameterMembers { group_id } => {
                self.reference_unlocked(group_id)
            }
            ReferenceCollection::CdiCombinedMembers { set_id } => self.reference_unlocked(set_id),
            _ => Ok(()),
        }
    }
    fn reference_selection_roots(&self, ids: &[String]) -> Result<Vec<String>, Status> {
        if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
            return Err(Status::error(
                "DUPLICATE_SELECTION",
                "Repeated reference identity",
            ));
        }
        for id in ids {
            if self.reference_identity(id).is_none() {
                return Err(Status::error("MISSING_REFERENCE_OBJECT", id));
            }
        }
        Ok(self
            .reference_identities()
            .iter()
            .filter(|i| {
                ids.contains(&i.id) && !i.owner.as_ref().is_some_and(|owner| ids.contains(owner))
            })
            .map(|i| i.id.clone())
            .collect())
    }
    fn validate_reference_fields(&self, id: &str, f: &ReferenceFields) -> Result<(), Status> {
        match f {
            ReferenceFields::ModelParameterGroup { name } => {
                if name.is_empty() || name.contains('\0') {
                    return Err(Status::error("INVALID_MODEL3_GROUPS", id));
                }
            }
            ReferenceFields::ModelHitArea { name, mesh } => {
                if name.contains('\0') {
                    return Err(Status::error("INVALID_MODEL3_HIT_AREAS", id));
                }
                self.validate_model_target(mesh, false)?;
            }
            ReferenceFields::ModelParameterMember { target } => {
                self.validate_model_target(target, true)?;
                let owner = self.reference_identity(id).unwrap().owner.as_ref().unwrap();
                if !matches!(target, ModelTargetRef::Unspecified)
                    && self
                        .get_model_parameter_group(owner)
                        .unwrap()
                        .parameters
                        .iter()
                        .any(|p| {
                            p.id != id
                                && self.model_parameter_key(&p.target)
                                    == self.model_parameter_key(target)
                        })
                {
                    return Err(Status::error("DUPLICATE_REFERENCE_TARGET", id));
                }
            }
            ReferenceFields::CdiCombinedMember { target } => {
                match target {
                    CdiParameterRef::Unspecified => {}
                    CdiParameterRef::Resolved { parameter_id } => {
                        if self.get_parameter(parameter_id).is_none() {
                            return Err(Status::error("MISSING_PARAMETER", parameter_id));
                        }
                    }
                    CdiParameterRef::Unresolved { runtime_id } => {
                        if runtime_id.is_empty() || runtime_id.contains('\0') {
                            return Err(Status::error("INVALID_REFERENCE_TARGET", id));
                        }
                    }
                }
                let owner = self.reference_identity(id).unwrap().owner.as_ref().unwrap();
                if !matches!(target, CdiParameterRef::Unspecified)
                    && self
                        .get_combined_set(owner)
                        .unwrap()
                        .members
                        .iter()
                        .any(|p| {
                            p.id != id
                                && self.cdi_parameter_key(&p.target)
                                    == self.cdi_parameter_key(target)
                        })
                {
                    return Err(Status::error("DUPLICATE_REFERENCE_TARGET", id));
                }
            }
        }
        Ok(())
    }
    pub(super) fn validate_model_target(
        &self,
        target: &ModelTargetRef,
        parameter: bool,
    ) -> Result<(), Status> {
        let valid = match target {
            ModelTargetRef::Unspecified => true,
            ModelTargetRef::Unresolved { runtime_id } => {
                !runtime_id.is_empty() && !runtime_id.contains('\0')
            }
            ModelTargetRef::Resolved { object_id } => {
                if parameter {
                    self.get_parameter(object_id).is_some()
                } else {
                    self.get_mesh(object_id).is_some()
                }
            }
        };
        if valid {
            Ok(())
        } else {
            Err(Status::error(
                "INVALID_REFERENCE_TARGET",
                format!("{target:?}"),
            ))
        }
    }
    pub(super) fn install_reference_fields(&mut self, id: &str, f: &ReferenceFields) {
        let owner = self.reference_identity(id).and_then(|i| i.owner.clone());
        match f {
            ReferenceFields::ModelParameterGroup { name } => {
                self.content
                    .model3_settings
                    .groups
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|g| g.id == id)
                    .unwrap()
                    .name = name.clone()
            }
            ReferenceFields::ModelHitArea { name, mesh } => {
                let a = self
                    .content
                    .model3_settings
                    .hit_areas
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|a| a.id == id)
                    .unwrap();
                a.name = name.clone();
                a.mesh = mesh.clone();
            }
            ReferenceFields::ModelParameterMember { target } => {
                self.content
                    .model3_settings
                    .groups
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|g| Some(&g.id) == owner.as_ref())
                    .unwrap()
                    .parameters
                    .iter_mut()
                    .find(|p| p.id == id)
                    .unwrap()
                    .target = target.clone()
            }
            ReferenceFields::CdiCombinedMember { target } => {
                self.content
                    .display_info
                    .combined_parameters
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|g| Some(&g.id) == owner.as_ref())
                    .unwrap()
                    .members
                    .iter_mut()
                    .find(|p| p.id == id)
                    .unwrap()
                    .target = target.clone()
            }
        }
    }
}
impl Document {
    pub(in crate::document) fn validate_reference_replacement_unlocked(
        &self,
        model: &Model3Settings,
        display: &DisplayInfo,
    ) -> Result<(), Status> {
        let old = Working {
            model: self.model3_settings().clone(),
            display: self.display_info().clone(),
        };
        let next = Working {
            model: model.clone(),
            display: display.clone(),
        };
        let collection = |w: &Working, kind: ReferenceKind, owner: Option<&str>| -> Vec<String> {
            match kind {
                ReferenceKind::ModelParameterGroup => w
                    .model
                    .groups
                    .iter()
                    .flatten()
                    .map(|v| v.id.clone())
                    .collect(),
                ReferenceKind::ModelHitArea => w
                    .model
                    .hit_areas
                    .iter()
                    .flatten()
                    .map(|v| v.id.clone())
                    .collect(),
                ReferenceKind::CdiCombinedSet => w
                    .display
                    .combined_parameters
                    .iter()
                    .flatten()
                    .map(|v| v.id.clone())
                    .collect(),
                ReferenceKind::ModelParameterMember => w
                    .model
                    .groups
                    .iter()
                    .flatten()
                    .find(|g| Some(g.id.as_str()) == owner)
                    .map(|g| g.parameters.iter().map(|p| p.id.clone()).collect())
                    .unwrap_or_default(),
                ReferenceKind::CdiCombinedMember => w
                    .display
                    .combined_parameters
                    .iter()
                    .flatten()
                    .find(|g| Some(g.id.as_str()) == owner)
                    .map(|g| g.members.iter().map(|p| p.id.clone()).collect())
                    .unwrap_or_default(),
            }
        };
        for i in self.reference_identities() {
            let a = old.get(&i.id);
            let b = next.get(&i.id);
            let equal = match (&a, &b) {
                (Some(Node::Group(a)), Some(Node::Group(b))) => a == b,
                (Some(Node::Area(a)), Some(Node::Area(b))) => a == b,
                (Some(Node::Set(a)), Some(Node::Set(b))) => a == b,
                (Some(Node::ModelMember(a)), Some(Node::ModelMember(b))) => a == b,
                (Some(Node::CombinedMember(a)), Some(Node::CombinedMember(b))) => a == b,
                _ => false,
            };
            let old_order = collection(&old, i.kind, i.owner.as_deref());
            let new_order = collection(&next, i.kind, i.owner.as_deref());
            if !equal
                || old_order.iter().position(|id| id == &i.id)
                    != new_order.iter().position(|id| id == &i.id)
            {
                self.reference_unlocked(&i.id)?;
            }
        }
        Ok(())
    }
    pub(in crate::document) fn clean_retired_reference_flags(&mut self, previous: Vec<String>) {
        self.reference_index.take();
        self.reference_projection.take();
        let retired = previous
            .into_iter()
            .filter(|id| self.reference_identity(id).is_none())
            .collect::<Vec<_>>();
        for id in retired {
            self.content.editor_state.objects.remove(&id);
        }
    }
}

impl Document {
    fn model_parameter_key(&self, t: &ModelTargetRef) -> Option<String> {
        match t {
            ModelTargetRef::Unspecified => None,
            ModelTargetRef::Unresolved { runtime_id } => Some(runtime_id.clone()),
            ModelTargetRef::Resolved { object_id } => {
                self.get_parameter(object_id).map(|p| p.runtime_id.clone())
            }
        }
    }
    fn cdi_parameter_key(&self, t: &CdiParameterRef) -> Option<String> {
        match t {
            CdiParameterRef::Unspecified => None,
            CdiParameterRef::Unresolved { runtime_id } => Some(runtime_id.clone()),
            CdiParameterRef::Resolved { parameter_id } => self
                .get_parameter(parameter_id)
                .map(|p| p.runtime_id.clone()),
        }
    }
}
