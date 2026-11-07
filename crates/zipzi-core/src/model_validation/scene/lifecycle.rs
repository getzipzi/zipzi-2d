use super::*;
impl Model<'_> {
    pub(super) fn plan_scene_create(
        &self,
        plan: &mut SceneChanges,
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
            SceneCreate::Rotation { .. } => "Rotation",
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
                SceneCreate::Mesh { .. } | SceneCreate::Warp { .. } | SceneCreate::Rotation { .. }
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
                    .is_some_and(|target| !self.content.assets.contains_key(target))
                {
                    return Err(Status::error(
                        "MISSING_ASSET",
                        mesh.texture_asset_id.as_str(),
                    ));
                }
                check(self.validate_mesh_properties(&mesh))?;
                plan.domain.delta.meshes.push(mesh);
                if self.draw_order_groups().is_some() {
                    let mut groups = self.resolved_drawing_groups();
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
            SceneCreate::Rotation { .. } | SceneCreate::Warp { .. } => {
                let warp_authoring = matches!(&object, SceneCreate::Warp { .. }).then(|| {
                    crate::WarpAuthoringSettings {
                        bezier_grid: Some(crate::WarpBezierGrid {
                            rows: 2,
                            columns: 2,
                        }),
                        ..Default::default()
                    }
                });
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
                    let (rows, columns) = (5, 5);
                    TransformData::Warp(WarpTransform {
                        bezier: None,
                        rows,
                        columns,
                        quad: false,
                        points: (0..=rows)
                            .flat_map(|row| {
                                (0..=columns).map(move |column| {
                                    let u = column as f32 / columns as f32;
                                    let v = row as f32 / rows as f32;
                                    Vec2::new(x0 * (1. - u) + x1 * u, y0 * (1. - v) + y1 * v)
                                })
                            })
                            .collect(),
                    })
                } else {
                    let SceneCreate::Rotation { origin } = object else {
                        unreachable!()
                    };
                    TransformData::Rotation(RotationTransform {
                        pose: crate::RotationPose {
                            origin: origin.unwrap_or_default(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                };
                let t = Transform {
                    id: id.clone(),
                    runtime_id: id.clone(),
                    name,
                    part_id: part_id.clone().map(Into::into),
                    parent_id: parent_id.clone().map(Into::into),
                    data,
                    warp_authoring,
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
    pub(super) fn plan_scene_duplicate(
        &self,
        plan: &mut SceneChanges,
        ids: &[String],
        scope: SceneCopyScope,
        destination: Option<&OrganizationCollection>,
        before: &Option<String>,
    ) -> Result<(), Status> {
        self.duplicate_scene_objects(&mut plan.domain, ids, scope, destination, before)
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
        plan: &mut SceneChanges,
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
            let mut drawing = self.resolved_drawing_groups();
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
        Ok(())
    }
}
