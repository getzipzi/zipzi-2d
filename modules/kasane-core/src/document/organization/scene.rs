use super::*;
impl Document {
    pub(super) fn scene_organization_owner(&self, id: &str) -> Option<&str> {
        self.get_part(id)
            .map(|p| p.parent_id.as_str())
            .or_else(|| self.get_mesh(id).map(|m| m.part_id.as_str()))
            .or_else(|| self.get_transform(id).map(|t| t.part()))
            .or_else(|| self.get_offscreen(id).map(|o| o.part_id.as_str()))
    }
    pub(in crate::document) fn scene_organization_order(&self) -> Vec<String> {
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
        plan: &mut OrganizationPlan,
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
            plan.delta.drawing = Some(crate::draw_order::resolved_groups(self));
        }
        Ok(())
    }
}
impl Delta {
    pub(in crate::document) fn runtime_changed(&self, d: &Document) -> bool {
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
    pub(in crate::document) fn changed(&self, d: &Document) -> bool {
        self.runtime_changed(d)
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
    pub(in crate::document) fn install(&self, d: &mut Document) {
        if self.runtime_changed(d)
            || self
                .groups
                .as_ref()
                .is_some_and(|info| !same_metadata_shape(d.display_info(), info))
            || self
                .hierarchy
                .as_ref()
                .is_some_and(|order| order.organization != d.hierarchy_order().organization)
        {
            d.organization_index.take();
        }
        let removed: HashSet<_> = self.removed.iter().map(String::as_str).collect();
        // Axis and sample flags belong to the binding, even though their IDs are
        // separate handles. Retain no editor-state references to removed owners.
        let removed_children: Vec<_> = d
            .content
            .editor_state
            .objects
            .keys()
            .filter(|id| {
                d.binding_identity(id)
                    .is_some_and(|child| removed.contains(child.owner.as_str()))
            })
            .cloned()
            .collect();
        for id in removed_children {
            d.content.editor_state.objects.remove(&id);
        }
        macro_rules! remove {
            ($map:ident,$order:ident) => {{
                d.content
                    .$map
                    .retain(|id, _| !removed.contains(id.as_str()));
                d.content.$order.retain(|id| !removed.contains(id.as_str()));
            }};
        }
        remove!(parameters, parameter_order);
        remove!(parts, part_order);
        remove!(meshes, mesh_order);
        remove!(transforms, transform_order);
        remove!(offscreens, offscreen_order);
        remove!(bindings, binding_order);
        remove!(scene_bindings, scene_binding_order);
        remove!(blend_bindings, blend_binding_order);
        remove!(glues, glue_order);
        for id in &self.removed {
            d.content.editor_state.objects.remove(id);
            d.vertex_slots.remove(id);
        }
        d.mesh_runtime_ids
            .retain(|_, id| !removed.contains(id.as_str()));
        macro_rules! put {
            ($field:ident,$map:ident,$order:ident) => {
                for value in &self.$field {
                    if !d.content.$map.contains_key(&value.id) {
                        d.content.$order.push(value.id.clone());
                    }
                    d.content.$map.insert(value.id.clone(), value.clone());
                }
            };
        }
        put!(parameters, parameters, parameter_order);
        put!(parts, parts, part_order);
        put!(meshes, meshes, mesh_order);
        put!(transforms, transforms, transform_order);
        put!(offscreens, offscreens, offscreen_order);
        put!(bindings, bindings, binding_order);
        put!(scene_bindings, scene_bindings, scene_binding_order);
        put!(blends, blend_bindings, blend_binding_order);
        put!(glues, glues, glue_order);
        for m in &self.meshes {
            d.mesh_runtime_ids
                .insert(m.runtime_id.clone(), m.id.clone());
            d.vertex_slots.insert(
                m.id.clone(),
                m.vertex_ids
                    .iter()
                    .enumerate()
                    .map(|(slot, id)| (*id, slot))
                    .collect(),
            );
        }
        for (id, parent) in &self.parents {
            if let Some(p) = d.content.parts.get_mut(id) {
                p.parent_id = parent.clone();
            } else if let Some(m) = d.content.meshes.get_mut(id) {
                m.part_id = parent.clone();
            } else if let Some(t) = d.content.transforms.get_mut(id) {
                t.part_id = (!parent.is_empty()).then(|| parent.clone().into());
            } else if let Some(o) = d.content.offscreens.get_mut(id) {
                o.part_id = parent.clone();
            }
        }
        if let Some(info) = &self.groups {
            d.content.display_info = info.clone();
        }
        if let Some(order) = &self.hierarchy {
            d.content.hierarchy_order = order.clone();
        }
        if let Some(groups) = &self.drawing {
            d.content.draw_order_groups = Some(groups.clone());
        }
        // Compound validation reads the installed binding identities before the
        // normal changed()/publish path invalidates the remaining caches.
        if !self.glues.is_empty()
            || !self.bindings.is_empty()
            || !self.scene_bindings.is_empty()
            || !self.removed.is_empty()
        {
            d.binding_lookup.take();
        }
    }
}
