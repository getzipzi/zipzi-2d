use super::*;
impl Document {
    pub(super) fn drawing_unlocked(&self, owner: &str) -> Result<(), Status> {
        if !owner.is_empty() {
            if self.get_part(owner).is_none() {
                return Err(Status::error("MISSING_PART", owner));
            }
            self.authoring_unlocked(owner)?;
        }
        Ok(())
    }
    pub fn drawing_group_unlocked(&self, owner: &str) -> Result<(), Status> {
        self.drawing_unlocked(owner)?;
        let groups = crate::draw_order::resolved_groups(self);
        let mut pending = vec![owner.to_owned()];
        let mut seen = HashSet::new();
        while let Some(owner) = pending.pop() {
            if !seen.insert(owner.clone()) {
                continue;
            }
            if let Some(group) = groups.iter().find(|g| g.owner == owner) {
                for id in &group.items {
                    if self.get_part(id).is_some() {
                        self.authoring_unlocked(id)?;
                        pending.push(id.clone());
                        if let Some(os) = self.offscreen_for_part(id) {
                            self.authoring_unlocked(&os.id)?;
                        }
                    } else {
                        self.scene_lifecycle_unlocked(&[id.clone()], SceneCopyScope::Selected)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn finish_drawing_plan(
        &self,
        plan: &mut ScenePlan,
        groups: Vec<DrawOrderGroup>,
    ) -> Result<(), Status> {
        let groups = crate::draw_order::validate_groups(self, &groups)?;
        if self.draw_order_groups() != Some(groups.as_slice()) {
            plan.domain.affected.extend(
                groups
                    .iter()
                    .flat_map(|g| std::iter::once(&g.owner).chain(g.items.iter()))
                    .filter(|id| !id.is_empty())
                    .cloned(),
            );
            plan.domain.delta.drawing = Some(groups);
            plan.domain.effects.drawing_preserved = false;
        }
        Ok(())
    }
    pub(super) fn plan_drawing_move(
        &self,
        plan: &mut ScenePlan,
        ids: &[String],
        owner: &str,
        before: &Option<String>,
    ) -> Result<(), Status> {
        self.drawing_unlocked(owner)?;
        let mut groups = crate::draw_order::resolved_groups(self);
        let target = groups
            .iter()
            .position(|g| g.owner == owner)
            .ok_or_else(|| Status::error("MISSING_DRAWING_GROUP", owner))?;
        let flattened: Vec<_> = groups
            .iter()
            .flat_map(|g| g.items.iter().cloned())
            .collect();
        let selected = ordered_selection(&flattened, ids)?;
        for id in &selected {
            if self.get_part(id).is_some() {
                self.drawing_group_unlocked(id)?;
            } else {
                self.scene_lifecycle_unlocked(&[id.clone()], SceneCopyScope::Selected)?;
            }
            let source = groups.iter().find(|g| g.items.contains(id)).unwrap();
            self.drawing_unlocked(&source.owner)?;
        }
        if before.as_ref().is_some_and(|id| selected.contains(id)) {
            if selected.len() == 1 && groups[target].items.contains(&selected[0]) {
                return Ok(());
            }
            return Err(Status::error(
                "INVALID_PLACEMENT",
                "Drawing anchor is selected",
            ));
        }
        let old = groups.clone();
        for group in &mut groups {
            group.items.retain(|id| !selected.contains(id));
        }
        let at = match before {
            Some(id) => groups[target]
                .items
                .iter()
                .position(|v| v == id)
                .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))?,
            None => groups[target].items.len(),
        };
        groups[target].items.splice(at..at, selected);
        if groups != old {
            self.finish_drawing_plan(plan, groups)?;
        }
        Ok(())
    }
    pub(super) fn plan_ensure_drawing(
        &self,
        plan: &mut ScenePlan,
        owner: &str,
        parent: &str,
        before: &Option<String>,
    ) -> Result<(), Status> {
        if owner.is_empty() {
            return Err(Status::error(
                "SYSTEM_DRAWING_ROOT",
                "The root drawing role is always present",
            ));
        }
        self.drawing_group_unlocked(owner)?;
        self.drawing_unlocked(parent)?;
        let mut groups = crate::draw_order::resolved_groups(self);
        if !groups.iter().any(|g| g.owner == owner) {
            groups.push(DrawOrderGroup {
                owner: owner.into(),
                items: vec![],
                min_order: 0,
                max_order: 0,
            });
        }
        for group in &mut groups {
            group.items.retain(|id| id != owner);
        }
        let group = groups
            .iter_mut()
            .find(|g| g.owner == parent)
            .ok_or_else(|| Status::error("MISSING_DRAWING_GROUP", parent))?;
        let at = match before {
            Some(id) => group
                .items
                .iter()
                .position(|v| v == id)
                .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))?,
            None => group.items.len(),
        };
        group.items.insert(at, owner.into());
        self.finish_drawing_plan(plan, groups)
    }
    pub(super) fn plan_drawing_dissolve(
        &self,
        plan: &mut ScenePlan,
        owner: &str,
    ) -> Result<(), Status> {
        if owner.is_empty() {
            return Err(Status::error(
                "SYSTEM_DRAWING_ROOT",
                "The drawing root cannot be dissolved",
            ));
        }
        self.drawing_group_unlocked(owner)?;
        if self.offscreen_for_part(owner).is_some() {
            return Err(Status::error(
                "DRAWING_GROUP_HAS_OFFSCREEN",
                "Remove or transfer its Offscreen before dissolving this drawing group",
            ));
        }
        let mut groups = crate::draw_order::resolved_groups(self);
        let source = groups
            .iter()
            .find(|g| g.owner == owner)
            .ok_or_else(|| Status::error("MISSING_DRAWING_GROUP", owner))?
            .clone();
        for id in &source.items {
            self.authoring_unlocked(id)?;
        }
        let parent = groups
            .iter_mut()
            .find(|g| g.items.iter().any(|id| id == owner))
            .ok_or_else(|| Status::error("INVALID_DRAW_GROUP", owner))?;
        self.drawing_unlocked(&parent.owner)?;
        let at = parent.items.iter().position(|id| id == owner).unwrap();
        parent.items.splice(at..at + 1, source.items);
        parent.min_order = parent.min_order.min(source.min_order);
        parent.max_order = parent.max_order.max(source.max_order);
        groups.retain(|g| g.owner != owner);
        self.finish_drawing_plan(plan, groups)
    }
    pub(super) fn plan_duplicate_drawing(
        &self,
        plan: &mut ScenePlan,
        source: &str,
        target: &str,
    ) -> Result<(), Status> {
        if target.is_empty() {
            return Err(Status::error(
                "SYSTEM_DRAWING_ROOT",
                "A drawing copy requires a distinct Part role",
            ));
        }
        if source == target {
            return Err(Status::error("DUPLICATE_DRAWING_OWNER", target));
        }
        self.drawing_unlocked(source)?;
        self.drawing_unlocked(target)?;
        let original = crate::draw_order::resolved_groups(self);
        let source_group = original
            .iter()
            .find(|g| g.owner == source)
            .ok_or_else(|| Status::error("MISSING_DRAWING_GROUP", source))?;
        if original
            .iter()
            .any(|g| g.owner == target && !g.items.is_empty())
        {
            return Err(Status::error("DRAWING_COPY_TARGET_NOT_EMPTY", target));
        }
        let mut owners = HashSet::from([source.to_owned()]);
        let mut closure = Vec::new();
        let mut cursor = 0;
        let mut pending = vec![source_group.clone()];
        while cursor < pending.len() {
            for id in &pending[cursor].items.clone() {
                if id == target {
                    return Err(Status::error("DRAWING_COPY_TARGET_IN_SOURCE", target));
                }
                self.authoring_unlocked(id)?;
                closure.push(id.clone());
                if let Some(group) = original.iter().find(|g| g.owner == *id) {
                    owners.insert(id.clone());
                    pending.push(group.clone());
                }
            }
            cursor += 1;
        }
        let copied_parts: HashSet<_> = closure
            .iter()
            .filter(|id| self.get_part(id).is_some())
            .cloned()
            .collect();
        closure.extend(
            self.offscreen_order()
                .iter()
                .filter(|id| copied_parts.contains(&self.get_offscreen(id).unwrap().part_id))
                .cloned(),
        );
        plan.domain.effects.composition_may_change =
            self.offscreen_for_part(source).is_some() || self.offscreen_for_part(target).is_some();
        plan.domain.copied.insert(source.into(), target.into());
        for id in &closure {
            plan.domain.copied.insert(id.clone(), new_id());
        }
        self.copy_scene(&mut plan.domain, &closure, &source_group.items, None, &None)?;
        let mut groups: Vec<_> = original
            .iter()
            .filter(|g| g.owner != target)
            .cloned()
            .collect();
        if !groups.iter().any(|g| g.items.iter().any(|id| id == target)) {
            groups
                .iter_mut()
                .find(|g| g.owner.is_empty())
                .unwrap()
                .items
                .push(target.into());
        }
        for group in original.iter().filter(|g| owners.contains(&g.owner)) {
            groups.push(DrawOrderGroup {
                owner: mapped(&plan.domain.copied, &group.owner),
                items: group
                    .items
                    .iter()
                    .map(|id| mapped(&plan.domain.copied, id))
                    .collect(),
                min_order: group.min_order,
                max_order: group.max_order,
            });
        }
        plan.domain.delta.drawing = Some(groups);
        plan.domain.copied.remove(source);
        plan.domain.effects.drawing_preserved = false;
        plan.domain.affected.extend(closure);
        plan.domain.affected.push(target.into());
        plan.domain
            .affected
            .extend(plan.domain.copied.values().cloned());
        self.validate_compound_organization(&plan.domain)
    }
}
