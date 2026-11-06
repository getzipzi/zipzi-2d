use super::*;
impl Model<'_> {
    pub fn id(&self) -> &str {
        self.content.id
    }
    pub fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.content.parameters.get(id)
    }

    pub fn get_part(&self, id: &str) -> Option<&Part> {
        self.content.parts.get(id)
    }

    pub fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.content.transforms.get(id)
    }

    pub fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.content.meshes.get(id)
    }
    pub fn mesh_order(&self) -> &[String] {
        self.content.mesh_order
    }
    pub fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.content.bindings.get(id)
    }
    pub fn binding_order(&self) -> &[String] {
        self.content.binding_order
    }
    pub fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.content.scene_bindings.get(id)
    }
    pub fn scene_binding_order(&self) -> &[String] {
        self.content.scene_binding_order
    }
    pub fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.content.blend_key_tables.get(id)
    }
    pub fn blend_key_table_order(&self) -> &[String] {
        self.content.blend_key_table_order
    }
    pub fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.content.blend_constraints.get(id)
    }
    pub fn blend_constraint_order(&self) -> &[String] {
        self.content.blend_constraint_order
    }

    pub fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.content.glues.get(id)
    }
    pub fn glue_order(&self) -> &[String] {
        self.content.glue_order
    }

    pub fn get_motion(&self, id: &str) -> Option<&MotionClip> {
        self.content.motions.get(id)
    }

    pub fn display_info(&self) -> &DisplayInfo {
        self.content.display_info
    }
    pub fn model3_settings(&self) -> &Model3Settings {
        self.content.model3_settings
    }
    pub fn pose(&self) -> Option<&PoseAsset> {
        self.content.pose.as_deref()
    }
    pub fn physics(&self) -> Option<&PhysicsAsset> {
        self.content.physics.as_deref()
    }
    pub fn motion_roots(&self) -> &[MotionRegistration] {
        self.content.motion_roots
    }
    pub fn motion_registrations(&self) -> impl Iterator<Item = &MotionRegistration> {
        self.content
            .motion_groups
            .iter()
            .flat_map(|g| g.entries.iter())
            .chain(self.content.motion_roots.iter())
    }
    pub fn get_motion_group(&self, id: &str) -> Option<&MotionGroup> {
        self.content.motion_groups.iter().find(|g| g.id == id)
    }
    pub fn get_motion_registration(&self, id: &str) -> Option<&MotionRegistration> {
        self.motion_registrations().find(|g| g.id == id)
    }
    pub fn binding_for_mesh(&self, id: &str) -> Option<&MeshBinding> {
        self.content.bindings.values().find(|v| v.mesh_id == id)
    }
    pub fn binding_for_scene(&self, id: &str) -> Option<&SceneBinding> {
        self.content
            .scene_bindings
            .values()
            .find(|v| v.target_id() == id)
    }
    pub fn offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.content.offscreens.values().find(|v| v.part_id == id)
    }
    pub fn get_attachment(&self, id: &str) -> Option<&PackageAttachment> {
        self.content.package_attachments.iter().find(|a| a.id == id)
    }
    pub fn attachment_path<'s>(&'s self, r: &'s AttachmentRef) -> Option<&'s str> {
        match r {
            AttachmentRef::Resolved { attachment_id } => {
                self.get_attachment(attachment_id).map(|a| a.path.as_str())
            }
            AttachmentRef::Unresolved { path } => Some(path),
        }
    }
    pub fn get_parameter_group(&self, id: &str) -> Option<&CdiParameterGroup> {
        self.content
            .display_info
            .parameter_groups
            .iter()
            .flatten()
            .find(|g| g.id == id)
    }
    pub fn get_expression_entry(&self, id: &str) -> Option<(&ExpressionAsset, &ExpressionEntry)> {
        self.content
            .expressions
            .values()
            .find_map(|e| e.entries.iter().find(|v| v.id == id).map(|v| (e, v)))
    }
    pub fn get_glue_pair(&self, id: &str) -> Option<&GlueVertexPair> {
        self.content
            .glues
            .values()
            .flat_map(|g| g.pairs.iter())
            .find(|p| p.id == id)
    }
    pub fn glue_pair_owner(&self, id: &str) -> Option<&str> {
        self.content
            .glues
            .values()
            .find(|g| g.pairs.iter().any(|p| p.id == id))
            .map(|g| g.id.as_str())
    }
    pub fn motion_identity_exists(&self, id: &str) -> bool {
        self.content.motions.contains_key(id)
            || self.get_motion_group(id).is_some()
            || self.get_motion_registration(id).is_some()
            || self.content.motions.values().any(|c| {
                c.tracks
                    .iter()
                    .any(|t| t.id == id || t.keys().any(|k| k.id == id))
                    || c.events.iter().any(|e| e.id == id)
            })
    }
    pub fn physics_owner(&self, id: &str) -> Option<&str> {
        self.physics()
            .filter(|a| a.entity_ids().skip(1).any(|v| v == id))
            .map(|a| a.id.as_str())
    }
    pub fn reference_identities(&self) -> &[ReferenceIdentity] {
        self.reference_ids.get_or_init(|| {
            let mut result = Vec::new();
            let mut add = |id: &str, kind, owner| {
                result.push(ReferenceIdentity {
                    id: id.into(),
                    kind,
                    owner,
                })
            };
            for g in self.model3_settings().groups.iter().flatten() {
                add(&g.id, ReferenceKind::ModelParameterGroup, None);
                for p in &g.parameters {
                    add(
                        &p.id,
                        ReferenceKind::ModelParameterMember,
                        Some(g.id.clone()),
                    );
                }
            }
            for a in self.model3_settings().hit_areas.iter().flatten() {
                add(&a.id, ReferenceKind::ModelHitArea, None);
            }
            for s in self.display_info().combined_parameters.iter().flatten() {
                add(&s.id, ReferenceKind::CdiCombinedSet, None);
                for p in &s.members {
                    add(&p.id, ReferenceKind::CdiCombinedMember, Some(s.id.clone()));
                }
            }
            result
        })
    }
    pub fn reference_identity(&self, id: &str) -> Option<&ReferenceIdentity> {
        self.reference_identities().iter().find(|r| r.id == id)
    }
    pub fn binding_identities(&self) -> &[BindingIdentity] {
        self.binding_ids
            .get_or_init(|| self.build_binding_identities())
    }
    pub fn binding_identity(&self, id: &str) -> Option<&BindingIdentity> {
        self.binding_identities().iter().find(|r| r.id == id)
    }
    pub fn contains_id(&self, id: &str) -> bool {
        self.base_identity_set().contains(id)
            || self.binding_identity(id).is_some()
            || self.reference_identity(id).is_some()
            || self.get_glue_pair(id).is_some()
            || self.motion_identity_exists(id)
    }
    pub fn check_binding_ids(
        &self,
        owner: &str,
        ids: impl Iterator<Item = String>,
    ) -> Result<(), Status> {
        let mut seen = HashSet::new();
        let base = self.base_identity_set();
        for id in ids {
            if id == owner
                || !valid_uuid(&id)
                || !seen.insert(id.clone())
                || self.binding_identity(&id).is_some_and(|v| v.owner != owner)
                || base.contains(id.as_str())
                || self.reference_identity(&id).is_some()
                || self.get_glue_pair(&id).is_some()
                || self.motion_identity_exists(&id)
            {
                return Err(Status::error("DUPLICATE_ID", id));
            }
        }
        Ok(())
    }
    pub(crate) fn base_identity_set(&self) -> std::collections::HashSet<&str> {
        let c = &self.content;
        let mut ids = std::collections::HashSet::new();
        ids.insert(c.id.as_str());
        ids.extend(c.assets.keys().map(String::as_str));
        ids.extend(c.parts.keys().map(String::as_str));
        ids.extend(c.transforms.keys().map(String::as_str));
        ids.extend(c.meshes.keys().map(String::as_str));
        ids.extend(c.parameters.keys().map(String::as_str));
        ids.extend(c.bindings.keys().map(String::as_str));
        ids.extend(c.scene_bindings.keys().map(String::as_str));
        ids.extend(c.blend_key_tables.keys().map(String::as_str));
        ids.extend(c.blend_constraints.keys().map(String::as_str));
        ids.extend(c.blend_bindings.keys().map(String::as_str));
        ids.extend(c.glues.keys().map(String::as_str));
        ids.extend(c.offscreens.keys().map(String::as_str));
        ids.extend(c.expressions.keys().map(String::as_str));
        ids.extend(c.motions.keys().map(String::as_str));
        ids.extend(
            c.display_info
                .parameter_groups
                .iter()
                .flatten()
                .map(|g| g.id.as_str()),
        );
        ids.extend(
            c.display_info
                .combined_parameters
                .iter()
                .flatten()
                .map(|s| s.id.as_str()),
        );
        ids.extend(
            c.expressions
                .values()
                .flat_map(|e| e.entries.iter().map(|e| e.id.as_str())),
        );
        ids.extend(c.motion_groups.iter().map(|g| g.id.as_str()));
        ids.extend(self.motion_registrations().map(|r| r.id.as_str()));
        ids.extend(c.package_attachments.iter().map(|a| a.id.as_str()));
        ids.extend(c.pose.iter().flat_map(|p| p.entity_ids()));
        ids.extend(c.physics.iter().flat_map(|p| p.entity_ids()));
        #[cfg(feature = "bench-instrumentation")]
        crate::benchmark::count("core.motion_validation_reserved_ids", ids.len());
        ids
    }
    fn build_binding_identities(&self) -> Vec<BindingIdentity> {
        let mut result = Vec::new();
        let mut axes = |owner: &str, axes: &[BindingAxis]| {
            for a in axes {
                result.push(BindingIdentity {
                    kind: BindingIdentityKind::Axis,
                    id: a.id.clone(),
                    owner: owner.into(),
                    axis: None,
                });
                result.extend(a.samples.iter().map(|s| BindingIdentity {
                    kind: BindingIdentityKind::Sample,
                    id: s.id.clone(),
                    owner: owner.into(),
                    axis: Some(a.id.clone()),
                }));
            }
        };
        for b in self
            .binding_order()
            .iter()
            .filter_map(|id| self.get_binding(id))
        {
            axes(&b.id, &b.axes);
        }
        for b in self
            .scene_binding_order()
            .iter()
            .filter_map(|id| self.get_scene_binding(id))
        {
            axes(&b.id, &b.axes);
        }
        for g in self.glue_order().iter().filter_map(|id| self.get_glue(id)) {
            if let Some(b) = &g.binding {
                axes(&g.id, &b.axes);
            }
        }
        for t in self
            .blend_key_table_order()
            .iter()
            .filter_map(|id| self.get_blend_key_table(id))
        {
            result.extend(t.samples.iter().map(|s| BindingIdentity {
                kind: BindingIdentityKind::Sample,
                id: s.id.clone(),
                owner: t.id.clone(),
                axis: None,
            }));
        }
        for c in self
            .blend_constraint_order()
            .iter()
            .filter_map(|id| self.get_blend_constraint(id))
        {
            result.extend(c.samples.iter().map(|s| BindingIdentity {
                kind: BindingIdentityKind::Sample,
                id: s.id.clone(),
                owner: c.id.clone(),
                axis: None,
            }));
        }
        result
    }
}
