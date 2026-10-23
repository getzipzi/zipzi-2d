use super::*;
impl Document {
    pub(super) fn plan_scene_create(
        &self,
        plan: &mut ScenePlan,
        id: String,
        name: String,
        object: SceneCreate,
        part_id: Option<String>,
        parent_id: Option<String>,
        before: Option<String>,
        deformation_before: Option<String>,
    ) -> Result<(), Status> {
        if !valid_uuid(&id) {
            return Err(Status::error("INVALID_ID", &id));
        }
        if self.contains_id(&id) {
            return Err(Status::error("DUPLICATE_ID", &id));
        }
        if let Some(part) = &part_id {
            if self.get_part(part).is_none() {
                return Err(Status::error("MISSING_PART", part));
            }
            self.authoring_unlocked(part)?;
        }
        if let Some(parent) = &parent_id {
            if self.get_transform(parent).is_none() {
                return Err(Status::error("MISSING_TRANSFORM", parent));
            }
            self.authoring_unlocked(parent)?;
        }
        let label = match &object {
            SceneCreate::Mesh { .. } => "Mesh",
            SceneCreate::Warp { .. } => "Warp",
            SceneCreate::Rotation => "Rotation",
            SceneCreate::Offscreen => "Offscreen",
            SceneCreate::Glue { .. } => "Glue",
        };
        let name = if name.is_empty() { label.into() } else { name };
        if matches!(object, SceneCreate::Glue { .. }) && (part_id.is_some() || parent_id.is_some())
        {
            return Err(Status::error(
                "GLUE_IS_CONNECTION",
                "Glue belongs to its management collection",
            ));
        }
        if matches!(object, SceneCreate::Offscreen) && parent_id.is_some() {
            return Err(Status::error(
                "OFFSCREEN_IS_COMPOSITE",
                "Offscreen has a Part owner, no deformation parent",
            ));
        }
        if let Some(anchor) = &deformation_before {
            if !matches!(
                object,
                SceneCreate::Mesh { .. } | SceneCreate::Warp { .. } | SceneCreate::Rotation
            ) {
                return Err(Status::error("NO_DEFORMATION_COLLECTION", &id));
            }
            let members = self.scene_collection(&SceneCollection::Deformation {
                parent_id: parent_id.clone(),
            })?;
            if !members.contains(anchor) {
                return Err(Status::error("INVALID_DEFORMATION_PLACEMENT", anchor));
            }
        }
        let mut hierarchy = self.hierarchy_order().clone();
        match object {
            SceneCreate::Mesh { texture } => {
                let mesh = Mesh {
                    id: id.clone(),
                    runtime_id: id.clone(),
                    name,
                    texture_asset_id: texture,
                    part_id: part_id.clone().unwrap_or_default(),
                    deformer_id: parent_id.clone().unwrap_or_default(),
                    ..Default::default()
                };
                check(mesh.texture_asset_id.validate())?;
                if mesh
                    .texture_asset_id
                    .resolved_id()
                    .is_some_and(|target| self.get_asset(target).is_none())
                {
                    return Err(Status::error(
                        "MISSING_ASSET",
                        mesh.texture_asset_id.as_str(),
                    ));
                }
                check(self.validate_mesh_properties(&mesh))?;
                plan.domain.delta.meshes.push(mesh);
                if self.draw_order_groups().is_some() {
                    let mut groups = crate::draw_order::resolved_groups(self);
                    let mut owner = part_id.clone().unwrap_or_default();
                    while !groups.iter().any(|g| g.owner == owner) {
                        owner = self.get_part(&owner).unwrap().parent_id.clone();
                    }
                    groups
                        .iter_mut()
                        .find(|g| g.owner == owner)
                        .unwrap()
                        .items
                        .push(id.clone());
                    plan.domain.delta.drawing = Some(groups);
                }
            }
            SceneCreate::Rotation | SceneCreate::Warp { .. } => {
                let data = if let SceneCreate::Warp { bounds } = object {
                    let canvas = self.source_canvas();
                    let [x0, y0, x1, y1] = bounds.unwrap_or([
                        0.,
                        0.,
                        canvas.map_or(100., |c| c.width),
                        canvas.map_or(100., |c| c.height),
                    ]);
                    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) || x1 <= x0 || y1 <= y0 {
                        return Err(Status::error("INVALID_WARP_BOUNDS", &id));
                    }
                    TransformData::Warp(WarpTransform {
                        bezier: None,
                        rows: 1,
                        columns: 1,
                        quad: false,
                        points: vec![
                            Vec2::new(x0, y0),
                            Vec2::new(x1, y0),
                            Vec2::new(x0, y1),
                            Vec2::new(x1, y1),
                        ],
                    })
                } else {
                    TransformData::Rotation(RotationTransform::default())
                };
                let t = Transform {
                    id: id.clone(),
                    runtime_id: id.clone(),
                    name,
                    part_id: part_id.clone().map(Into::into),
                    parent_id: parent_id.clone().map(Into::into),
                    data,
                    ..Default::default()
                };
                check(self.validate_transform(&t))?;
                plan.domain.delta.transforms.push(t);
            }
            SceneCreate::Offscreen => {
                let os = Offscreen {
                    id: id.clone(),
                    runtime_id: id.clone(),
                    name,
                    part_id: part_id
                        .clone()
                        .ok_or_else(|| Status::error("OFFSCREEN_REQUIRES_PART", &id))?,
                    ..Default::default()
                };
                check(self.validate_offscreen(&os))?;
                plan.domain.effects.composition_may_change = true;
                plan.domain.delta.offscreens.push(os);
            }
            SceneCreate::Glue { a, b } => {
                let g = Glue {
                    id: id.clone(),
                    runtime_id: id.clone(),
                    name,
                    mesh_a_id: a,
                    mesh_b_id: b,
                    ..Default::default()
                };
                check(self.validate_glue(&g))?;
                plan.domain.effects.deformation_preserved = false;
                plan.domain.delta.glues.push(g);
                let members = self.scene_collection(&SceneCollection::Glues)?;
                let mut order = members.clone();
                let at = match &before {
                    Some(anchor) => order
                        .iter()
                        .position(|v| v == anchor)
                        .ok_or_else(|| Status::error("INVALID_PLACEMENT", anchor))?,
                    None => order.len(),
                };
                order.insert(at, id.clone());
                hierarchy.glues = order;
            }
        }
        if plan.domain.delta.glues.is_empty() {
            let members = self.scene_collection(&SceneCollection::Organization {
                part_id: part_id.clone(),
            })?;
            if let Some(anchor) = &before {
                if !members.contains(anchor) {
                    return Err(Status::error("INVALID_PLACEMENT", anchor));
                }
            }
            let mut order = Vec::new();
            let mut seen = HashSet::new();
            for source in self
                .hierarchy_order()
                .organization
                .iter()
                .chain(self.part_order())
                .chain(self.transform_order())
                .chain(self.mesh_order())
                .chain(self.offscreen_order())
            {
                if seen.insert(source) {
                    order.push(source.clone());
                }
            }
            let at = before.as_ref().map_or(Ok(order.len()), |anchor| {
                order
                    .iter()
                    .position(|v| v == anchor)
                    .ok_or_else(|| Status::error("INVALID_PLACEMENT", anchor))
            })?;
            order.insert(at, id.clone());
            hierarchy.organization = order;
            if !plan.domain.delta.meshes.is_empty() || !plan.domain.delta.transforms.is_empty() {
                hierarchy.deformation = self.scene_deformation_order();
                let at = deformation_before.as_ref().map_or(
                    Ok(hierarchy.deformation.len()),
                    |anchor| {
                        hierarchy
                            .deformation
                            .iter()
                            .position(|v| v == anchor)
                            .ok_or_else(|| Status::error("INVALID_DEFORMATION_PLACEMENT", anchor))
                    },
                )?;
                hierarchy.deformation.insert(at, id.clone());
            }
        }
        plan.domain.delta.hierarchy = Some(hierarchy);
        plan.domain.affected.push(id);
        Ok(())
    }
    pub(super) fn scene_selection(
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
    pub(super) fn plan_scene_duplicate(
        &self,
        plan: &mut ScenePlan,
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
                ))
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
            plan.domain.copied.insert(id.clone(), new_id());
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
            self.copy_scene(&mut plan.domain, &scene, &roots, parent, before)?;
        }
        self.copy_scene_glues(plan, &closure, before)?;
        plan.domain.affected.extend(closure);
        plan.domain
            .affected
            .extend(plan.domain.copied.values().cloned());
        self.validate_compound_organization(&plan.domain)
    }
    pub(super) fn copy_scene_glues(
        &self,
        plan: &mut ScenePlan,
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
            copy.id = plan.domain.copied[id].clone();
            copy.runtime_id = unique(&source.runtime_id, &mut runtimes);
            for pair in &mut copy.pairs {
                self.authoring_unlocked(&pair.id)?;
                let next = new_id();
                plan.domain.copied.insert(pair.id.clone(), next.clone());
                pair.id = next;
            }
            copy.mesh_a_id.remap(&plan.domain.copied);
            copy.mesh_b_id.remap(&plan.domain.copied);
            if let Some(binding) = &mut copy.binding {
                copy_axes(&mut binding.axes, &mut plan.domain.copied);
                for form in &mut binding.keyforms {
                    for sample in &mut form.samples {
                        *sample = mapped(&plan.domain.copied, sample);
                    }
                }
            }
            for binding in self.iter_blend_bindings_for_target(id) {
                self.authoring_unlocked(&binding.id)?;
                let mut b = binding.clone();
                b.id = new_id();
                plan.domain.copied.insert(binding.id.clone(), b.id.clone());
                b.target_id = copy.id.clone();
                plan.domain.delta.blends.push(b);
            }
            plan.domain.delta.glues.push(copy);
        }
        if !plan.domain.delta.glues.is_empty() {
            let mut hierarchy = plan
                .domain
                .delta
                .hierarchy
                .take()
                .unwrap_or_else(|| self.hierarchy_order().clone());
            let mut order = self.scene_collection(&SceneCollection::Glues)?;
            let at = match before {
                Some(anchor) => order
                    .iter()
                    .position(|id| id == anchor)
                    .ok_or_else(|| Status::error("INVALID_PLACEMENT", anchor))?,
                None => order.len(),
            };
            order.splice(at..at, plan.domain.delta.glues.iter().map(|g| g.id.clone()));
            hierarchy.glues = order;
            plan.domain.delta.hierarchy = Some(hierarchy);
        }
        Ok(())
    }
    pub fn scene_delete_users(
        &self,
        ids: &[String],
        scope: SceneCopyScope,
    ) -> Result<Vec<String>, Status> {
        let mut closure = self.scene_selection(ids, scope)?;
        self.include_owned_scene_bindings(&mut closure);
        let selected: HashSet<_> = closure.iter().collect();
        let mut users = Vec::new();
        for id in &closure {
            users.extend(
                self.references_to(id)
                    .into_iter()
                    .filter(|user| !selected.contains(user)),
            );
        }
        users.sort();
        users.dedup();
        Ok(users)
    }
    pub(super) fn include_owned_scene_bindings(&self, closure: &mut Vec<String>) {
        let pairs: Vec<_> = closure
            .iter()
            .filter_map(|id| self.get_glue(id))
            .flat_map(|g| g.pairs.iter().map(|p| p.id.clone()))
            .collect();
        closure.extend(pairs);
        let selected: HashSet<_> = closure.iter().cloned().collect();
        closure.extend(
            self.binding_order()
                .iter()
                .filter(|id| selected.contains(&self.get_binding(id).unwrap().mesh_id))
                .cloned(),
        );
        closure.extend(
            self.scene_binding_order()
                .iter()
                .filter(|id| selected.contains(self.get_scene_binding(id).unwrap().target_id()))
                .cloned(),
        );
        closure.extend(
            self.blend_binding_order()
                .iter()
                .filter(|id| selected.contains(&self.get_blend_binding(id).unwrap().target_id))
                .cloned(),
        );
    }
    pub(super) fn plan_scene_delete(
        &self,
        plan: &mut ScenePlan,
        ids: &[String],
        scope: SceneCopyScope,
    ) -> Result<(), Status> {
        let users = self.scene_delete_users(ids, scope)?;
        if !users.is_empty() {
            return Err(Status::error("OBJECT_REFERENCED", users.join(", ")));
        }
        let mut closure = self.scene_selection(ids, scope)?;
        self.include_owned_scene_bindings(&mut closure);
        for id in &closure {
            self.authoring_unlocked(id)?;
        }
        let selected: HashSet<_> = closure.iter().collect();
        let mut hierarchy = self.hierarchy_order().clone();
        hierarchy.organization.retain(|id| !selected.contains(id));
        hierarchy.deformation.retain(|id| !selected.contains(id));
        hierarchy.glues.retain(|id| !selected.contains(id));
        plan.domain.delta.hierarchy = Some(hierarchy);
        if closure.iter().any(|id| self.get_mesh(id).is_some()) {
            let mut drawing = crate::draw_order::resolved_groups(self);
            for group in &mut drawing {
                group.items.retain(|id| !selected.contains(id));
            }
            plan.domain.delta.drawing = Some(drawing);
        }
        plan.domain.effects.composition_may_change =
            closure.iter().any(|id| self.get_offscreen(id).is_some());
        plan.domain.effects.deformation_preserved =
            !closure.iter().any(|id| self.get_glue(id).is_some());
        plan.domain.affected = closure.clone();
        plan.domain.delta.removed = closure;
        self.validate_compound_organization(&plan.domain)
    }
}
