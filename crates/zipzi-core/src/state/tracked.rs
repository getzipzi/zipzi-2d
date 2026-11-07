use super::*;
impl DocumentBuilder {
    pub(super) fn retain_assets(&mut self, mut keep: impl FnMut(&str, &AssetRecord) -> bool) {
        let ids = self
            .content
            .assets
            .shared_map()
            .iter()
            .filter(|(id, v)| !keep(id, v))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.touched_assets.insert(id.clone());
            self.content.assets.remove(&id);
        }
    }
    pub(super) fn retain_parameters(&mut self, mut keep: impl FnMut(&str, &Parameter) -> bool) {
        let ids = self
            .content
            .parameters
            .shared_map()
            .iter()
            .filter(|(id, v)| !keep(id, v))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.touched_parameters.insert(id.clone());
            if let Some(p) = self.content.parameters.get(&id) {
                Arc::make_mut(&mut self.content.parameter_runtime_ids).remove(&p.runtime_id);
            }
            self.content.parameters.remove(&id);
        }
    }
    pub(super) fn parts_mut(&mut self, id: &str) -> Option<&mut Part> {
        if self.content.parts.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::Part)
            .or_default()
            .insert(id.to_owned());
        self.content.parts.get_mut(id)
    }
    pub(super) fn transforms_mut(&mut self, id: &str) -> Option<&mut Transform> {
        if self.content.transforms.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::Transform)
            .or_default()
            .insert(id.to_owned());
        self.content.transforms.get_mut(id)
    }
    pub(super) fn meshes_mut(&mut self, id: &str) -> Option<&mut Mesh> {
        if self.content.meshes.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::Mesh)
            .or_default()
            .insert(id.to_owned());
        self.content.meshes.get_mut(id)
    }
    pub(super) fn scene_bindings_mut(&mut self, id: &str) -> Option<&mut SceneBinding> {
        if self.content.scene_bindings.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::SceneBinding)
            .or_default()
            .insert(id.to_owned());
        self.content.scene_bindings.get_mut(id)
    }
    pub(super) fn retain_blend_key_tables(
        &mut self,
        mut keep: impl FnMut(&str, &BlendShapeKeyTable) -> bool,
    ) {
        let ids = self
            .content
            .blend_key_tables
            .shared_map()
            .iter()
            .filter(|(id, v)| !keep(id, v))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.touched
                .entry(RecordDomain::BlendKeyTable)
                .or_default()
                .insert(id.clone());
            self.content.blend_key_tables.remove(&id);
        }
    }
    pub(super) fn retain_blend_constraints(
        &mut self,
        mut keep: impl FnMut(&str, &BlendShapeConstraint) -> bool,
    ) {
        let ids = self
            .content
            .blend_constraints
            .shared_map()
            .iter()
            .filter(|(id, v)| !keep(id, v))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.touched
                .entry(RecordDomain::BlendConstraint)
                .or_default()
                .insert(id.clone());
            self.content.blend_constraints.remove(&id);
        }
    }
    pub(super) fn blend_bindings_mut(&mut self, id: &str) -> Option<&mut BlendShapeBinding> {
        if self.content.blend_bindings.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::BlendBinding)
            .or_default()
            .insert(id.to_owned());
        self.content.blend_bindings.get_mut(id)
    }
    pub(super) fn glues_mut(&mut self, id: &str) -> Option<&mut Glue> {
        if self.content.glues.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::Glue)
            .or_default()
            .insert(id.to_owned());
        self.content.glues.get_mut(id)
    }
    pub(super) fn offscreens_mut(&mut self, id: &str) -> Option<&mut Offscreen> {
        if self.content.offscreens.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::Offscreen)
            .or_default()
            .insert(id.to_owned());
        self.content.offscreens.get_mut(id)
    }
    pub(super) fn expressions_mut(&mut self, id: &str) -> Option<&mut ExpressionAsset> {
        if self.content.expressions.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::Expression)
            .or_default()
            .insert(id.to_owned());
        self.content.expressions.get_mut(id)
    }
    pub(super) fn retain_expressions(
        &mut self,
        mut keep: impl FnMut(&str, &ExpressionAsset) -> bool,
    ) {
        let ids = self
            .content
            .expressions
            .shared_map()
            .iter()
            .filter(|(id, v)| !keep(id, v))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.touched
                .entry(RecordDomain::Expression)
                .or_default()
                .insert(id.clone());
            self.content.expressions.remove(&id);
        }
    }
    pub(super) fn motions_mut(&mut self, id: &str) -> Option<&mut MotionClip> {
        if self.content.motions.get(id).is_none() {
            return None;
        }
        self.touched
            .entry(RecordDomain::Motion)
            .or_default()
            .insert(id.to_owned());
        self.content.motions.get_mut(id)
    }
    pub(super) fn retain_motions(&mut self, mut keep: impl FnMut(&str, &MotionClip) -> bool) {
        let ids = self
            .content
            .motions
            .shared_map()
            .iter()
            .filter(|(id, v)| !keep(id, v))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.touched
                .entry(RecordDomain::Motion)
                .or_default()
                .insert(id.clone());
            self.content.motions.remove(&id);
        }
    }
    pub(super) fn draw_order_groups_mut(&mut self) -> &mut Option<Vec<DrawOrderGroup>> {
        self.roots_touched.insert(RootDomain::DrawOrderGroups);
        Arc::make_mut(&mut self.content.draw_order_groups)
    }
    pub(super) fn display_info_mut(&mut self) -> &mut DisplayInfo {
        self.roots_touched.insert(RootDomain::DisplayInfo);
        Arc::make_mut(&mut self.content.display_info)
    }
    pub(super) fn hierarchy_order_mut(&mut self) -> &mut HierarchyOrder {
        self.roots_touched.insert(RootDomain::HierarchyOrder);
        Arc::make_mut(&mut self.content.hierarchy_order)
    }
    pub(super) fn editor_state_mut(&mut self) -> &mut EditorState {
        self.roots_touched.insert(RootDomain::EditorState);
        Arc::make_mut(&mut self.content.editor_state)
    }
    pub(super) fn motion_groups_mut(&mut self) -> &mut Vec<MotionGroup> {
        self.roots_touched.insert(RootDomain::MotionGroups);
        Arc::make_mut(&mut self.content.motion_groups)
    }
    pub(super) fn motion_roots_mut(&mut self) -> &mut Vec<MotionRegistration> {
        self.roots_touched.insert(RootDomain::MotionRoots);
        Arc::make_mut(&mut self.content.motion_roots)
    }
    pub(super) fn pose_mut(&mut self) -> &mut Option<Arc<PoseAsset>> {
        self.roots_touched.insert(RootDomain::Pose);
        Arc::make_mut(&mut self.content.pose)
    }
    pub(super) fn physics_mut(&mut self) -> &mut Option<Arc<PhysicsAsset>> {
        self.roots_touched.insert(RootDomain::Physics);
        Arc::make_mut(&mut self.content.physics)
    }
    pub(super) fn model3_settings_mut(&mut self) -> &mut Model3Settings {
        self.roots_touched.insert(RootDomain::Model3Settings);
        Arc::make_mut(&mut self.content.model3_settings)
    }
    pub(super) fn package_attachments_mut(&mut self) -> &mut Vec<PackageAttachment> {
        self.roots_touched.insert(RootDomain::PackageAttachments);
        Arc::make_mut(&mut self.content.package_attachments)
    }
}
