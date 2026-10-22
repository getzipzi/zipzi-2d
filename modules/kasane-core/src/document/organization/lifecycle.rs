use super::*;
use crate::{BindingAxis, SceneTrack};
impl Document {
    pub(super) fn plan_organization_copy(
        &self,
        plan: &mut OrganizationPlan,
        ids: &[String],
        dest: &OrganizationCollection,
        before: &Option<String>,
    ) -> Result<(), Status> {
        let roots = self.organization_selection(ids)?;
        let closure = self.organization_subtree(&roots)?;
        self.organization_subtree_unlocked(&roots)?;
        self.organization_collection(dest)?;
        match dest {
            OrganizationCollection::Parameters { group_id } => {
                self.check_group_owner(group_id.as_deref())?
            }
            OrganizationCollection::ParameterGroups { parent_id } => {
                self.check_group_owner(parent_id.as_deref())?
            }
            OrganizationCollection::PartMembers { part_id } => {
                self.check_part_owner(part_id.as_deref())?
            }
        }
        match dest {
            OrganizationCollection::Parameters { group_id } => {
                if roots.iter().any(|id| self.get_parameter(id).is_none()) {
                    return Err(Status::error(
                        "WRONG_ORGANIZATION_COLLECTION",
                        "Expected parameters",
                    ));
                }
                for id in &closure {
                    if self.get_parameter(id).is_some() {
                        plan.copied.insert(id.clone(), new_id());
                    }
                }
                self.copy_parameters(plan, &closure)?;
                let mut info = self.explicit_parameter_info();
                let entries = info.parameters.as_mut().unwrap();
                let at = parameter_insertion(entries, group_id.as_deref(), before)?;
                let mut copied = vec![];
                for id in &roots {
                    let mut e=entries.iter().find(|e|matches!(e,CdiParameterEntry::Resolved { parameter_id,.. } if parameter_id==id)).unwrap().clone();
                    if let CdiParameterEntry::Resolved { parameter_id, .. } = &mut e {
                        *parameter_id = plan.copied[id].clone();
                    }
                    set_group(&mut e, group_id.clone());
                    copied.push(e);
                }
                entries.splice(at..at, copied);
                plan.delta.groups = Some(info);
            }
            OrganizationCollection::ParameterGroups { parent_id } => {
                if roots
                    .iter()
                    .any(|id| self.get_parameter_group(id).is_none())
                {
                    return Err(Status::error(
                        "WRONG_ORGANIZATION_COLLECTION",
                        "Expected groups",
                    ));
                }
                for id in &closure {
                    plan.copied.insert(id.clone(), new_id());
                }
                self.copy_parameters(plan, &closure)?;
                let mut info = self.explicit_parameter_info();
                let mut runtime: HashSet<_> = info
                    .parameter_groups
                    .iter()
                    .flatten()
                    .map(|g| g.runtime_id.clone())
                    .collect();
                let copies: Vec<_> = info
                    .parameter_groups
                    .iter()
                    .flatten()
                    .filter(|g| closure.contains(&g.id))
                    .map(|g| {
                        let mut copied = g.clone();
                        copied.id = plan.copied[&g.id].clone();
                        copied.runtime_id = unique(&g.runtime_id, &mut runtime);
                        copied.parent_id = if roots.contains(&g.id) {
                            parent_id.clone()
                        } else {
                            g.parent_id.as_ref().map(|id| mapped(&plan.copied, id))
                        };
                        copied
                    })
                    .collect();
                let groups = info.parameter_groups.get_or_insert_with(Vec::new);
                let at = group_insertion(groups, parent_id.as_deref(), before)?;
                groups.splice(at..at, copies);
                let mut entries = vec![];
                for e in info.parameters.iter().flatten() {
                    if e.group_id()
                        .is_some_and(|g| closure.iter().any(|id| id == g))
                    {
                        let mut e = e.clone();
                        match &mut e {
                            CdiParameterEntry::Resolved {
                                parameter_id,
                                group_id,
                                ..
                            } => {
                                *parameter_id = plan.copied[parameter_id].clone();
                                *group_id = group_id.as_ref().map(|g| mapped(&plan.copied, g));
                            }
                            CdiParameterEntry::Unresolved { .. } => {
                                return Err(Status::error(
                                    "UNRESOLVED_PARAMETER_COPY_CONFLICT",
                                    "Repair unresolved CDI parameters before copying their group",
                                ))
                            }
                        }
                        entries.push(e);
                    }
                }
                info.parameters.get_or_insert_with(Vec::new).extend(entries);
                plan.delta.groups = Some(info);
            }
            OrganizationCollection::PartMembers { part_id } => {
                if roots.iter().all(|id| self.get_part(id).is_none()) {
                    let scene = self.plan_scene_edit(SceneEdit::Duplicate {
                        ids: roots,
                        scope: SceneCopyScope::Selected,
                        destination: Some(dest.clone()),
                        before: before.clone(),
                    })?;
                    *plan = scene.domain;
                    return Ok(());
                }
                if roots.iter().any(|id| self.get_part(id).is_none()) {
                    return Err(Status::error(
                        "MIXED_OWNERSHIP_SCOPE",
                        "Copy Part subtrees and independent scene objects separately",
                    ));
                }
                for id in &closure {
                    plan.copied.insert(id.clone(), new_id());
                }
                self.copy_scene(
                    plan,
                    &closure,
                    &roots,
                    Some(part_id.as_deref().unwrap_or("")),
                    before,
                )?;
            }
        }
        plan.affected.extend(closure);
        plan.affected.extend(plan.copied.values().cloned());
        self.validate_compound_organization(plan)
    }
    fn copy_parameters(
        &self,
        plan: &mut OrganizationPlan,
        closure: &[String],
    ) -> Result<(), Status> {
        let mut runtimes: HashSet<_> = self
            .parameter_order()
            .iter()
            .map(|id| self.get_parameter(id).unwrap().runtime_id.clone())
            .chain(
                self.display_info()
                    .parameters
                    .iter()
                    .flatten()
                    .filter_map(|e| match e {
                        CdiParameterEntry::Unresolved { runtime_id, .. } => {
                            Some(runtime_id.clone())
                        }
                        _ => None,
                    }),
            )
            .collect();
        for id in closure {
            if let Some(p) = self.get_parameter(id) {
                let mut p = p.clone();
                p.id = plan.copied[id].clone();
                p.runtime_id = unique(&p.runtime_id, &mut runtimes);
                plan.delta.parameters.push(p);
            }
        }
        Ok(())
    }
    pub(in crate::document) fn copy_scene(
        &self,
        plan: &mut OrganizationPlan,
        closure: &[String],
        roots: &[String],
        parent: Option<&str>,
        before: &Option<String>,
    ) -> Result<(), Status> {
        // Copy callers discover their closures through different relations.
        // Validate every included object here, including attached Offscreens.
        for id in closure {
            self.authoring_unlocked(id)?;
        }
        let selected: HashSet<_> = closure.iter().map(String::as_str).collect();
        // Owned bindings are independent of the organizational/deformation tree.
        for id in
            self.binding_order()
                .iter()
                .filter(|id| selected.contains(self.get_binding(id).unwrap().mesh_id.as_str()))
                .chain(self.scene_binding_order().iter().filter(|id| {
                    selected.contains(self.get_scene_binding(id).unwrap().target_id())
                }))
                .chain(self.blend_binding_order().iter().filter(|id| {
                    selected.contains(self.get_blend_binding(id).unwrap().target_id.as_str())
                }))
        {
            self.authoring_unlocked(id)?;
            plan.copied.insert(id.clone(), new_id());
        }
        let map = &mut plan.copied;
        let mut mesh_runtime: HashSet<_> = self
            .mesh_order()
            .iter()
            .map(|id| self.get_mesh(id).unwrap().runtime_id.clone())
            .collect();
        let mut part_runtime: HashSet<_> = self
            .part_order()
            .iter()
            .map(|id| self.get_part(id).unwrap().runtime_id.clone())
            .chain(
                self.display_info()
                    .parts
                    .iter()
                    .flatten()
                    .filter_map(|e| match e {
                        CdiPartEntry::Unresolved { runtime_id, .. } => Some(runtime_id.clone()),
                        _ => None,
                    }),
            )
            .collect();
        let mut transform_runtime: HashSet<_> = self
            .transform_order()
            .iter()
            .map(|id| self.get_transform(id).unwrap().runtime_id.clone())
            .collect();
        let mut offscreen_runtime: HashSet<_> = self
            .offscreen_order()
            .iter()
            .map(|id| self.get_offscreen(id).unwrap().runtime_id.clone())
            .collect();
        for id in closure {
            if let Some(p) = self.get_part(id) {
                let mut p = p.clone();
                p.id = map[id].clone();
                p.runtime_id = unique(&p.runtime_id, &mut part_runtime);
                p.parent_id = if roots.contains(id) {
                    parent
                        .map(str::to_owned)
                        .unwrap_or_else(|| mapped(map, &p.parent_id))
                } else {
                    mapped(map, &p.parent_id)
                };
                plan.delta.parts.push(p);
            } else if let Some(m) = self.get_mesh(id) {
                let mut m = m.clone();
                m.id = map[id].clone();
                m.runtime_id = unique(&m.runtime_id, &mut mesh_runtime);
                m.part_id = if roots.contains(id) {
                    parent
                        .map(str::to_owned)
                        .unwrap_or_else(|| mapped(map, &m.part_id))
                } else {
                    mapped(map, &m.part_id)
                };
                // An empty drawing owner can map to a Part when copying the
                // root group. It is not an identity for a deformation parent.
                if !m.deformer_id.is_empty() {
                    m.deformer_id = mapped(map, &m.deformer_id);
                }
                for mask in &mut m.masks {
                    *mask = mapped(map, mask);
                }
                plan.delta.meshes.push(m);
            } else if let Some(t) = self.get_transform(id) {
                let mut t = t.clone();
                t.id = map[id].clone();
                t.runtime_id = unique(&t.runtime_id, &mut transform_runtime);
                t.part_id = if roots.contains(id) && parent.is_some() {
                    parent
                        .filter(|p| !p.is_empty())
                        .map(|p| p.to_owned().into())
                } else {
                    t.part_id.as_ref().map(|id| mapped(map, id.as_str()).into())
                };
                t.parent_id = t
                    .parent_id
                    .as_ref()
                    .map(|id| mapped(map, id.as_str()).into());
                plan.delta.transforms.push(t);
            } else if let Some(o) = self.get_offscreen(id) {
                let mut o = o.clone();
                o.id = map[id].clone();
                o.runtime_id = unique(&o.runtime_id, &mut offscreen_runtime);
                o.part_id = if roots.contains(id) {
                    parent
                        .map(str::to_owned)
                        .unwrap_or_else(|| mapped(map, &o.part_id))
                } else {
                    mapped(map, &o.part_id)
                };
                for mask in &mut o.masks {
                    *mask = mapped(map, mask);
                }
                plan.delta.offscreens.push(o);
            }
        }
        for id in self.binding_order() {
            let b = self.get_binding(id).unwrap();
            if !selected.contains(b.mesh_id.as_str()) {
                continue;
            }
            let mut b = b.clone();
            b.id = map[id].clone();
            b.mesh_id = mapped(map, &b.mesh_id);
            copy_axes(&mut b.axes, map);
            for f in &mut b.keyforms {
                for s in &mut f.samples {
                    *s = mapped(map, s);
                }
            }
            plan.delta.bindings.push(b);
        }
        for id in self.scene_binding_order() {
            let b = self.get_scene_binding(id).unwrap();
            if !selected.contains(b.target_id()) {
                continue;
            }
            let mut b = b.clone();
            b.id = map[id].clone();
            copy_axes(&mut b.axes, map);
            match &mut b.track {
                SceneTrack::Warp {
                    target_id,
                    keyforms,
                } => {
                    *target_id = mapped(map, target_id.as_str()).into();
                    for f in keyforms {
                        for s in &mut f.samples {
                            *s = mapped(map, s);
                        }
                    }
                }
                SceneTrack::Rotation {
                    target_id,
                    keyforms,
                } => {
                    *target_id = mapped(map, target_id.as_str()).into();
                    for f in keyforms {
                        for s in &mut f.samples {
                            *s = mapped(map, s);
                        }
                    }
                }
                SceneTrack::Part {
                    target_id,
                    keyforms,
                } => {
                    *target_id = mapped(map, target_id.as_str()).into();
                    for f in keyforms {
                        for s in &mut f.samples {
                            *s = mapped(map, s);
                        }
                    }
                }
            }
            plan.delta.scene_bindings.push(b);
        }
        for id in self.blend_binding_order() {
            let b = self.get_blend_binding(id).unwrap();
            if selected.contains(b.target_id.as_str()) {
                let mut b = b.clone();
                b.id = map[id].clone();
                b.target_id = mapped(map, &b.target_id);
                plan.delta.blends.push(b);
            }
        }
        let mut drawing = crate::draw_order::resolved_groups(self);
        let originals = drawing.clone();
        for g in &mut drawing {
            let mut next = vec![];
            for id in &g.items {
                next.push(id.clone());
                if let Some(new) = map.get(id) {
                    next.push(new.clone());
                }
            }
            g.items = next;
        }
        for g in &originals {
            if let Some(owner) = map.get(&g.owner) {
                let items = g
                    .items
                    .iter()
                    .filter_map(|id| map.get(id).cloned())
                    .collect();
                plan.effects
                    .external_drawing_members
                    .extend(g.items.iter().filter(|id| !map.contains_key(*id)).cloned());
                drawing.push(DrawOrderGroup {
                    owner: owner.clone(),
                    items,
                    min_order: g.min_order,
                    max_order: g.max_order,
                });
            }
        }
        // A copied item occurs exactly once, under its copied drawing owner when present.
        for g in &mut drawing {
            if !map.values().any(|id| id == &g.owner) {
                g.items.retain(|id| {
                    !map.iter().any(|(old, new)| {
                        new == id
                            && originals.iter().any(|source| {
                                source.items.contains(old) && map.contains_key(&source.owner)
                            })
                    })
                });
            }
        }
        plan.delta.drawing = Some(drawing);
        let mut order = self.scene_organization_order();
        let at = if let Some(parent) = parent {
            self.scene_insertion(&order, parent, before)?
        } else if let Some(anchor) = before {
            order
                .iter()
                .position(|id| id == anchor)
                .ok_or_else(|| Status::error("INVALID_PLACEMENT", anchor))?
        } else {
            order.len()
        };
        let new_roots: Vec<_> = roots.iter().map(|id| map[id].clone()).collect();
        order.splice(at..at, new_roots);
        order.extend(
            closure
                .iter()
                .filter(|id| !roots.contains(id))
                .map(|id| map[id].clone()),
        );
        let mut hierarchy = self.hierarchy_order().clone();
        hierarchy.organization = order;
        for id in &self.hierarchy_order().deformation {
            if let Some(new) = map.get(id) {
                hierarchy.deformation.push(new.clone());
            }
        }
        plan.delta.hierarchy = Some(hierarchy);
        let mut info = self.display_info().clone();
        if let Some(entries) = &mut info.parts {
            let copies: Vec<_> = entries
                .iter()
                .filter_map(|e| match e {
                    CdiPartEntry::Resolved {
                        part_id,
                        extensions,
                    } => map.get(part_id).map(|id| CdiPartEntry::Resolved {
                        part_id: id.clone(),
                        extensions: extensions.clone(),
                    }),
                    _ => None,
                })
                .collect();
            entries.extend(copies);
            plan.delta.groups = Some(info);
        }
        plan.effects.drawing_preserved = false;
        plan.effects.deformation_preserved = false;
        Ok(())
    }
    pub(in crate::document) fn validate_compound_organization(
        &self,
        plan: &OrganizationPlan,
    ) -> Result<(), Status> {
        let mut candidate = self.fork_candidate();
        plan.delta.install(&mut candidate);
        if let Some(issue) = candidate.validate_structure().into_iter().next() {
            return Err(issue.status);
        }
        Ok(())
    }
    pub fn organization_delete_users(
        &self,
        ids: &[String],
        policy: OrganizationDeletePolicy,
    ) -> Result<Vec<String>, Status> {
        let roots = self.organization_selection(ids)?;
        let closure = if policy == OrganizationDeletePolicy::Contents {
            self.organization_subtree(&roots)?
        } else {
            roots.clone()
        };
        let mut owned: HashSet<_> = closure.iter().cloned().collect();
        for target in &closure {
            if let Some(b) = self.binding_for_mesh(target) {
                owned.insert(b.id.clone());
            }
            if let Some(b) = self.binding_for_scene(target) {
                owned.insert(b.id.clone());
            }
            for b in self.iter_blend_bindings_for_target(target) {
                owned.insert(b.id.clone());
            }
        }
        let mut moved = HashSet::new();
        if policy == OrganizationDeletePolicy::Dissolve {
            for id in &roots {
                moved.extend(
                    self.organization_index()
                        .children
                        .get(id)
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
        }
        let mut users = vec![];
        for id in &owned {
            users.extend(self.references_to(id).into_iter().filter(|r| {
                !owned.contains(r)
                    && !moved.contains(r)
                    && !r.starts_with("display_info.Parameters[")
                    && !r.starts_with("display_info.Parts[")
            }));
        }
        if policy == OrganizationDeletePolicy::Contents {
            for group in self.draw_order_groups().into_iter().flatten() {
                if owned.contains(&group.owner) {
                    users.extend(
                        group
                            .items
                            .iter()
                            .filter(|id| !owned.contains(*id))
                            .cloned(),
                    );
                }
            }
        }
        users.sort();
        users.dedup();
        Ok(users)
    }
    pub(super) fn plan_organization_delete(
        &self,
        plan: &mut OrganizationPlan,
        ids: &[String],
        policy: OrganizationDeletePolicy,
    ) -> Result<(), Status> {
        let roots = self.organization_selection(ids)?;
        self.organization_subtree_unlocked(&roots)?;
        let mut info = self.display_info().clone();
        let mut removing = if policy == OrganizationDeletePolicy::Contents {
            self.organization_subtree(&roots)?
        } else {
            roots.clone()
        };
        if policy == OrganizationDeletePolicy::Dissolve {
            for id in &roots {
                if let Some(group) = self.get_parameter_group(id) {
                    let groups = info.parameter_groups.as_mut().unwrap();
                    let mut children = vec![];
                    groups.retain(|g| {
                        if g.parent_id.as_deref() == Some(id) {
                            let mut g = g.clone();
                            g.parent_id = group.parent_id.clone();
                            children.push(g);
                            false
                        } else {
                            true
                        }
                    });
                    // Flat imported storage need not put parents before children.
                    // Locate the parent after extracting its children.
                    let at = groups.iter().position(|g| g.id == *id).unwrap();
                    groups.splice(at..at, children);
                    for e in info.parameters.iter_mut().flatten() {
                        if e.group_id() == Some(id) {
                            set_group(e, group.parent_id.clone());
                        }
                    }
                } else if self.get_part(id).is_some() {
                    self.plan_part_dissolve(plan, id)?;
                } else {
                    return Err(Status::error("DISSOLVE_REQUIRES_CONTAINER", id));
                }
            }
        }
        let selected: HashSet<_> = removing.iter().cloned().collect();
        {
            removing.extend(
                self.binding_order()
                    .iter()
                    .filter(|id| selected.contains(&self.get_binding(id).unwrap().mesh_id))
                    .cloned(),
            );
            removing.extend(
                self.scene_binding_order()
                    .iter()
                    .filter(|id| selected.contains(self.get_scene_binding(id).unwrap().target_id()))
                    .cloned(),
            );
            removing.extend(
                self.blend_binding_order()
                    .iter()
                    .filter(|id| selected.contains(&self.get_blend_binding(id).unwrap().target_id))
                    .cloned(),
            );
        }
        let removed: HashSet<_> = removing.iter().cloned().collect();
        if policy == OrganizationDeletePolicy::Contents {
            for group in self.draw_order_groups().into_iter().flatten() {
                if removed.contains(&group.owner) {
                    if let Some(external) = group.items.iter().find(|id| !removed.contains(*id)) {
                        return Err(Status::error("OBJECT_REFERENCED", format!("Drawing member {external} is outside the organization delete closure")));
                    }
                }
            }
        }
        let moved: HashSet<_> = plan
            .delta
            .parents
            .iter()
            .map(|(id, _)| id.clone())
            .chain(
                info.parameter_groups
                    .iter()
                    .flatten()
                    .filter(|g| {
                        self.get_parameter_group(&g.id)
                            .is_some_and(|old| old.parent_id != g.parent_id)
                    })
                    .map(|g| g.id.clone()),
            )
            .collect();
        for id in &removing {
            self.authoring_unlocked(id)?;
            let refs: Vec<_> = self
                .references_to(id)
                .into_iter()
                .filter(|r| {
                    !removed.contains(r)
                        && !moved.contains(r)
                        && !r.starts_with("display_info.Parameters[")
                        && !r.starts_with("display_info.Parts[")
                })
                .collect();
            if !refs.is_empty() {
                return Err(Status::error(
                    "OBJECT_REFERENCED",
                    format!("{id}: {}", refs.join(", ")),
                ));
            }
        }
        if let Some(groups) = &mut info.parameter_groups {
            groups.retain(|g| !removed.contains(&g.id));
        }
        if let Some(entries) = &mut info.parameters {
            entries.retain(|e|!e.group_id().is_some_and(|g|removed.contains(g)) && !matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if removed.contains(parameter_id)));
        }
        if let Some(entries) = &mut info.parts {
            entries.retain(
                |e| !matches!(e,CdiPartEntry::Resolved {part_id,..} if removed.contains(part_id)),
            );
        }
        plan.delta.groups = Some(info);
        plan.delta.removed = removing;
        let mut hierarchy = plan
            .delta
            .hierarchy
            .take()
            .unwrap_or_else(|| self.hierarchy_order().clone());
        hierarchy.organization.retain(|id| !removed.contains(id));
        hierarchy.deformation.retain(|id| !removed.contains(id));
        plan.delta.hierarchy = Some(hierarchy);
        if plan
            .delta
            .removed
            .iter()
            .any(|id| self.scene_organization_owner(id).is_some())
        {
            let mut groups = plan
                .delta
                .drawing
                .take()
                .unwrap_or_else(|| crate::draw_order::resolved_groups(self));
            groups.retain(|g| !removed.contains(&g.owner));
            for g in &mut groups {
                g.items.retain(|id| !removed.contains(id));
            }
            plan.delta.drawing = Some(groups);
        }
        plan.affected.extend(plan.delta.removed.clone());
        plan.affected.extend(moved);
        self.validate_compound_organization(plan)
    }
    fn plan_part_dissolve(&self, plan: &mut OrganizationPlan, id: &str) -> Result<(), Status> {
        let p = self.get_part(id).unwrap();
        if !p.enabled
            || self
                .binding_for_scene(id)
                .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
            || self.offscreen_for_part(id).is_some()
            || self
                .iter_active_blend_bindings_for_target(id)
                .next()
                .is_some()
        {
            return Err(Status::error("PART_DISSOLVE_STATE_UNREPRESENTABLE", id));
        }
        let mut groups = plan
            .delta
            .drawing
            .take()
            .unwrap_or_else(|| crate::draw_order::resolved_groups(self));
        if let Some(index) = groups.iter().position(|g| g.owner == id) {
            let children = groups[index].items.clone();
            if let Some(parent_index) = groups
                .iter()
                .position(|g| g.items.iter().any(|item| item == id))
            {
                if groups[parent_index].items.len() > 1 {
                    let child_group = &groups[index];
                    let parent_group = &groups[parent_index];
                    let mut proposed = parent_group.clone();
                    let at = proposed.items.iter().position(|v| v == id).unwrap();
                    proposed.items.splice(at..at + 1, children.clone());
                    for child in &children {
                        if let Some((order, _)) = self.static_draw_item(child) {
                            proposed.min_order = proposed.min_order.min(order);
                            proposed.max_order = proposed.max_order.max(order);
                        }
                    }
                    let static_proof = self
                        .static_group_order(parent_group)
                        .zip(self.static_group_order(child_group))
                        .zip(self.static_group_order(&proposed))
                        .is_some_and(|((mut parent, children), flattened)| {
                            let at = parent.iter().position(|v| v == id).unwrap();
                            parent.splice(at..at + 1, children);
                            parent == flattened
                        });
                    let equal_rank_proof = children.iter().all(|child| {
                        self.static_draw_item(child)
                            .is_some_and(|(order, enabled)| {
                                enabled && order == (p.draw_order + 0.001) as i32
                            })
                    });
                    if !static_proof && !equal_rank_proof {
                        return Err(Status::error("PART_DISSOLVE_DRAW_ORDER_UNPROVEN", id));
                    }
                    groups[parent_index].min_order = proposed.min_order;
                    groups[parent_index].max_order = proposed.max_order;
                }
                let at = groups[parent_index]
                    .items
                    .iter()
                    .position(|v| v == id)
                    .unwrap();
                groups[parent_index].items.splice(at..at + 1, children);
            } else if !children.is_empty() {
                return Err(Status::error("PART_DISSOLVE_DRAW_ORDER_UNPROVEN", id));
            }
            groups.remove(index);
        }
        plan.delta.drawing = Some(groups);
        for member in self.organization_collection(&OrganizationCollection::PartMembers {
            part_id: Some(id.into()),
        })? {
            plan.delta.parents.push((member, p.parent_id.clone()));
        }
        let mut hierarchy = plan
            .delta
            .hierarchy
            .take()
            .unwrap_or_else(|| self.hierarchy_order().clone());
        let mut order = if hierarchy.organization.is_empty() {
            self.scene_organization_order()
        } else {
            hierarchy.organization.clone()
        };
        let members = self.organization_collection(&OrganizationCollection::PartMembers {
            part_id: Some(id.into()),
        })?;
        order.retain(|v| !members.contains(v));
        let at = order.iter().position(|v| v == id).unwrap();
        order.splice(at..at, members);
        hierarchy.organization = order;
        plan.delta.hierarchy = Some(hierarchy);
        Ok(())
    }
    fn static_draw_item(&self, id: &str) -> Option<(i32, bool)> {
        let dynamic = |id: &str| {
            self.binding_for_scene(id)
                .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                || self
                    .iter_active_blend_bindings_for_target(id)
                    .next()
                    .is_some()
        };
        let (order, mut enabled, part, mut transform) = if let Some(p) = self.get_part(id) {
            (p.draw_order, p.enabled, Some(id), None)
        } else {
            let m = self.get_mesh(id)?;
            if self
                .binding_for_mesh(id)
                .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                || self
                    .iter_active_blend_bindings_for_target(id)
                    .next()
                    .is_some()
            {
                return None;
            }
            (
                m.draw_order
                    .unwrap_or(self.mesh_order().iter().position(|v| v == id)? as f32),
                m.enabled,
                (!m.part_id.is_empty()).then_some(m.part_id.as_str()),
                (!m.deformer_id.is_empty()).then_some(m.deformer_id.as_str()),
            )
        };
        let mut owner = part;
        while let Some(id) = owner {
            if dynamic(id) {
                return None;
            }
            let p = self.get_part(id)?;
            enabled &= p.enabled;
            owner = (!p.parent_id.is_empty()).then_some(p.parent_id.as_str());
        }
        while let Some(id) = transform {
            if dynamic(id) {
                return None;
            }
            let t = self.get_transform(id)?;
            enabled &= t.enabled;
            transform = (!t.parent().is_empty()).then_some(t.parent());
        }
        Some(((order + 0.001) as i32, enabled))
    }
    fn static_group_order(&self, group: &DrawOrderGroup) -> Option<Vec<String>> {
        let mut items = group
            .items
            .iter()
            .enumerate()
            .map(|(slot, id)| {
                self.static_draw_item(id).map(|(order, enabled)| {
                    (
                        if enabled {
                            order.clamp(group.min_order, group.max_order)
                        } else {
                            group.min_order
                        },
                        slot,
                        id.clone(),
                    )
                })
            })
            .collect::<Option<Vec<_>>>()?;
        items.sort_by_key(|(order, slot, _)| (*order, *slot));
        Some(items.into_iter().map(|(_, _, id)| id).collect())
    }
}
pub(in crate::document) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub(in crate::document) fn mapped(map: &BTreeMap<String, String>, id: &str) -> String {
    map.get(id).cloned().unwrap_or_else(|| id.into())
}
pub(in crate::document) fn unique(base: &str, used: &mut HashSet<String>) -> String {
    let root = if base.is_empty() { "Copy" } else { base };
    for n in 1_u64.. {
        let v = if n == 1 {
            format!("{root}_copy")
        } else {
            format!("{root}_copy{n}")
        };
        if used.insert(v.clone()) {
            return v;
        }
    }
    unreachable!()
}
pub(in crate::document) fn copy_axes(axes: &mut [BindingAxis], map: &mut BTreeMap<String, String>) {
    for a in axes {
        let id = new_id();
        map.insert(a.id.clone(), id.clone());
        a.id = id;
        for s in &mut a.samples {
            let id = new_id();
            map.insert(s.id.clone(), id.clone());
            s.id = id;
        }
    }
}
