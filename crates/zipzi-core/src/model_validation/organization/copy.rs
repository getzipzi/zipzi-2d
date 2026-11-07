use super::*;
use lifecycle::{copy_axes, mapped, new_id, unique};
impl Model<'_> {
    pub(crate) fn scene_selection(
        &self,
        ids: &[String],
        scope: SceneCopyScope,
    ) -> Result<Vec<String>, Status> {
        let selected: HashSet<_> = ids.iter().collect();
        if ids.is_empty() || selected.len() != ids.len() {
            return Err(Status::error(
                "INVALID_SELECTION",
                "Select distinct scene objects",
            ));
        }
        for id in ids {
            if self.get_mesh(id).is_none()
                && self.get_transform(id).is_none()
                && self.get_offscreen(id).is_none()
                && self.get_glue(id).is_none()
            {
                return Err(Status::error("MISSING_SCENE_OBJECT", id));
            }
        }
        let mut closure: HashSet<String> = ids.iter().cloned().collect();
        if scope == SceneCopyScope::DeformationSubtree {
            loop {
                let mut changed = false;
                for id in self.transform_order().iter().chain(self.mesh_order()) {
                    if self
                        .scene_deformer_parent(id)
                        .is_some_and(|parent| closure.contains(parent))
                    {
                        changed |= closure.insert(id.clone());
                    }
                }
                if !changed {
                    break;
                }
            }
        }
        let mut seen = HashSet::new();
        Ok(self
            .hierarchy_order()
            .organization
            .iter()
            .chain(self.part_order())
            .chain(self.transform_order())
            .chain(self.mesh_order())
            .chain(self.offscreen_order())
            .chain(self.glue_order())
            .filter(|id| closure.contains(id.as_str()) && seen.insert(id.as_str()))
            .cloned()
            .collect())
    }
    pub(crate) fn duplicate_scene_objects(
        &self,
        plan: &mut OrganizationChanges,
        ids: &[String],
        scope: SceneCopyScope,
        destination: Option<&OrganizationCollection>,
        before: &Option<String>,
    ) -> Result<(), Status> {
        let closure = self.scene_selection(ids, scope)?;
        let parent = match destination {
            Some(OrganizationCollection::PartMembers { part_id }) => {
                Some(part_id.as_deref().unwrap_or(""))
            }
            Some(_) => {
                return Err(Status::error(
                    "WRONG_SCENE_COLLECTION",
                    "Scene copies require a Part collection",
                ));
            }
            None => None,
        };
        if let Some(parent) = parent {
            if !parent.is_empty() {
                if self.get_part(parent).is_none() {
                    return Err(Status::error("MISSING_PART", parent));
                }
                self.authoring_unlocked(parent)?;
            }
            if closure.iter().any(|id| self.get_glue(id).is_some()) {
                return Err(Status::error(
                    "GLUE_IS_CONNECTION",
                    "Glue copies use the Glue management collection",
                ));
            }
        }
        for id in &closure {
            self.authoring_unlocked(id)?;
            plan.copied.insert(id.clone(), new_id());
        }
        let scene: Vec<_> = closure
            .iter()
            .filter(|id| self.get_glue(id).is_none())
            .cloned()
            .collect();
        let roots: Vec<_> = closure
            .iter()
            .filter(|id| ids.contains(id) && self.get_glue(id).is_none())
            .cloned()
            .collect();
        if parent.is_none() {
            if let Some(anchor) = before {
                for root in &roots {
                    if self.organization_owner(root) != self.organization_owner(anchor) {
                        return Err(Status::error("INVALID_PLACEMENT", anchor));
                    }
                }
            }
        }
        if !scene.is_empty() {
            self.copy_scene(plan, &scene, &roots, parent, before)?;
        }
        self.copy_scene_glues(plan, &closure, before)?;
        plan.affected.extend(closure);
        plan.affected.extend(plan.copied.values().cloned());
        Ok(())
    }
    pub(crate) fn copy_scene_glues(
        &self,
        plan: &mut OrganizationChanges,
        closure: &[String],
        before: &Option<String>,
    ) -> Result<(), Status> {
        let mut runtimes: HashSet<_> = self
            .glue_order()
            .iter()
            .map(|id| self.get_glue(id).unwrap().runtime_id.clone())
            .collect();
        for id in closure {
            let Some(source) = self.get_glue(id) else {
                continue;
            };
            let mut copy = source.clone();
            copy.id = plan.copied[id].clone();
            copy.runtime_id = unique(&source.runtime_id, &mut runtimes);
            for pair in &mut copy.pairs {
                self.authoring_unlocked(&pair.id)?;
                let next = new_id();
                plan.copied.insert(pair.id.clone(), next.clone());
                pair.id = next;
            }
            copy.mesh_a_id.remap(&plan.copied);
            copy.mesh_b_id.remap(&plan.copied);
            if let Some(binding) = &mut copy.binding {
                copy_axes(&mut binding.axes, &mut plan.copied);
                for form in &mut binding.keyforms {
                    for sample in &mut form.samples {
                        *sample = mapped(&plan.copied, sample);
                    }
                }
            }
            for binding in self.iter_blend_bindings_for_target(id) {
                self.authoring_unlocked(&binding.id)?;
                let mut b = binding.clone();
                b.id = new_id();
                plan.copied.insert(binding.id.clone(), b.id.clone());
                b.target_id = copy.id.clone();
                plan.delta.blends.push(b);
            }
            plan.delta.glues.push(copy);
        }
        if !plan.delta.glues.is_empty() {
            let mut hierarchy = plan
                .delta
                .hierarchy
                .take()
                .unwrap_or_else(|| self.hierarchy_order().clone());
            let mut order = self.ordered_glue_ids();
            let at = match before {
                Some(anchor) => order
                    .iter()
                    .position(|id| id == anchor)
                    .ok_or_else(|| Status::error("INVALID_PLACEMENT", anchor))?,
                None => order.len(),
            };
            order.splice(at..at, plan.delta.glues.iter().map(|g| g.id.clone()));
            hierarchy.glues = order;
            plan.delta.hierarchy = Some(hierarchy);
        }
        Ok(())
    }
}
