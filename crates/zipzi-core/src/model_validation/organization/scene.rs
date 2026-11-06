use super::*;
impl Model<'_> {
    pub(crate) fn scene_organization_owner(&self, id: &str) -> Option<&str> {
        self.get_part(id)
            .map(|p| p.parent_id.as_str())
            .or_else(|| self.get_mesh(id).map(|m| m.part_id.as_str()))
            .or_else(|| self.get_transform(id).map(|t| t.part()))
            .or_else(|| self.get_offscreen(id).map(|o| o.part_id.as_str()))
    }
    pub(crate) fn scene_organization_order(&self) -> Vec<String> {
        self.organization_index().scene.clone()
    }
    pub(super) fn scene_insertion(
        &self,
        order: &[String],
        parent: &str,
        before: &Option<String>,
    ) -> Result<usize, Status> {
        if let Some(id) = before {
            order
                .iter()
                .position(|v| v == id && self.scene_organization_owner(v) == Some(parent))
                .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))
        } else {
            Ok(order
                .iter()
                .rposition(|id| self.scene_organization_owner(id) == Some(parent))
                .map_or(order.len(), |i| i + 1))
        }
    }
    pub(super) fn plan_scene_organization_move(
        &self,
        plan: &mut OrganizationChanges,
        ids: &[String],
        parent: &str,
        before: &Option<String>,
    ) -> Result<(), Status> {
        self.check_part_owner((!parent.is_empty()).then_some(parent))?;
        for id in ids {
            let old = self
                .scene_organization_owner(id)
                .ok_or_else(|| Status::error("WRONG_ORGANIZATION_COLLECTION", id))?;
            if self.get_part(id).is_some()
                && !parent.is_empty()
                && self.is_part_ancestor(id, parent)
            {
                return Err(Status::error("RELATION_CYCLE", id));
            }
            if let Some(anchor) = before {
                self.organization_unlocked(anchor)?;
                if parent.is_empty()
                    && self.get_part(id).is_some() != self.get_part(anchor).is_some()
                {
                    return Err(Status::error("ROOT_COLLECTION_MISMATCH", id));
                }
            }
            if old != parent {
                if let Some(os) = self.get_offscreen(id) {
                    if parent.is_empty() {
                        return Err(Status::error("OFFSCREEN_REQUIRES_PART", id));
                    }
                    if self.offscreen_for_part(parent).is_some_and(|o| o.id != *id) {
                        return Err(Status::error("DUPLICATE_OWNER", id));
                    }
                    if self.binding_for_scene(old).is_some()
                        || self.binding_for_scene(parent).is_some()
                    {
                        return Err(Status::error("OFFSCREEN_REBIND_REQUIRES_CONVERSION", id));
                    }
                    let mut proposed = os.clone();
                    proposed.part_id = parent.into();
                    let status = self.validate_offscreen(&proposed);
                    if !status.is_ok() {
                        return Err(status);
                    }
                }
                plan.delta.parents.push((id.clone(), parent.into()));
                plan.effects.visibility_may_change = true;
                plan.effects.composition_may_change |= self.offscreen_for_part(old).is_some()
                    || self.parent_offscreen_for_part(old).is_some()
                    || self.offscreen_for_part(parent).is_some()
                    || self.parent_offscreen_for_part(parent).is_some();
            }
        }
        let original = self.scene_organization_order();
        let mut order = original.clone();
        order.retain(|id| !ids.contains(id));
        let at = self.scene_insertion(&order, parent, before)?;
        order.splice(at..at, ids.iter().cloned());
        let members = |order: &[String]| {
            order
                .iter()
                .filter(|id| ids.contains(id) || self.scene_organization_owner(id) == Some(parent))
                .cloned()
                .collect::<Vec<_>>()
        };
        if members(&original) != members(&order) || !plan.delta.parents.is_empty() {
            let mut hierarchy = self.hierarchy_order().clone();
            hierarchy.organization = order;
            plan.delta.hierarchy = Some(hierarchy);
        }
        if !plan.delta.parents.is_empty() {
            plan.delta.drawing = Some(self.resolved_drawing_groups());
        }
        Ok(())
    }
}
impl Delta {
    pub(crate) fn runtime_changed(&self, d: &Model<'_>) -> bool {
        !self.parameters.is_empty()
            || !self.parts.is_empty()
            || !self.parents.is_empty()
            || !self.meshes.is_empty()
            || !self.transforms.is_empty()
            || !self.offscreens.is_empty()
            || !self.bindings.is_empty()
            || !self.scene_bindings.is_empty()
            || !self.blends.is_empty()
            || !self.glues.is_empty()
            || self
                .removed
                .iter()
                .any(|id| d.get_parameter_group(id).is_none())
    }
    pub(crate) fn changed(&self, d: &Model<'_>) -> bool {
        self.runtime_changed(d)
            || !self.rotation_bone_copies.is_empty()
            || self
                .model3
                .as_ref()
                .is_some_and(|v| v != d.model3_settings())
            || self
                .groups
                .as_ref()
                .is_some_and(|info| info != d.display_info())
            || self
                .hierarchy
                .as_ref()
                .is_some_and(|v| v != d.hierarchy_order())
            || self
                .drawing
                .as_ref()
                .is_some_and(|v| Some(v.as_slice()) != d.draw_order_groups())
    }
}
