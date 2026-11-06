use super::*;

/// Owned decoding/construction input. It is never a published or mutable state.
/// Collection order is the vector order; freeze validates the complete graph.
#[derive(Debug, Default)]
pub struct OwnedDocument {
    pub id: String,
    pub setup: ModelSetup,
    pub assets: Vec<AssetRecord>,
    pub parameters: Vec<Parameter>,
    pub parts: Vec<Part>,
    pub transforms: Vec<Transform>,
    pub meshes: Vec<Mesh>,
    pub bindings: Vec<MeshBinding>,
    pub scene_bindings: Vec<SceneBinding>,
    pub blend_key_tables: Vec<BlendShapeKeyTable>,
    pub blend_constraints: Vec<BlendShapeConstraint>,
    pub blend_bindings: Vec<BlendShapeBinding>,
    pub glues: Vec<Glue>,
    pub offscreens: Vec<Offscreen>,
    pub expressions: Vec<ExpressionAsset>,
    pub motions: Vec<MotionClip>,
    pub draw_order_groups: Option<Vec<DrawOrderGroup>>,
    pub display_info: DisplayInfo,
    pub hierarchy_order: HierarchyOrder,
    pub editor_state: EditorState,
    pub motion_groups: Vec<MotionGroup>,
    pub motion_roots: Vec<MotionRegistration>,
    pub pose: Option<Arc<PoseAsset>>,
    pub physics: Option<Arc<PhysicsAsset>>,
    pub missing_attachments: Vec<String>,
    pub model3_settings: Model3Settings,
    pub package_attachments: Vec<PackageAttachment>,
}
impl DocumentState {
    pub fn from_owned(input: OwnedDocument) -> Result<Self, Status> {
        let mut builder = Self::new(input.id, input.setup)?.builder();
        for value in input.assets {
            builder.create_asset(value)?;
        }
        for value in input.parameters {
            builder.create_parameter(value)?;
        }
        for value in input.parts {
            builder.create_part(value)?;
        }
        for value in input.transforms {
            builder.create_transform(value)?;
        }
        for value in input.meshes {
            builder.create_mesh(value)?;
        }
        for value in input.bindings {
            builder.create_binding(value)?;
        }
        for value in input.scene_bindings {
            builder.create_scene_binding(value)?;
        }
        for value in input.blend_key_tables {
            builder.create_blend_key_table(value)?;
        }
        for value in input.blend_constraints {
            builder.create_blend_constraint(value)?;
        }
        for value in input.blend_bindings {
            builder.create_blend_binding(value)?;
        }
        for value in input.glues {
            builder.create_glue(value)?;
        }
        for value in input.offscreens {
            builder.create_offscreen(value)?;
        }
        for value in input.expressions {
            builder.create_expression(value)?;
        }
        for value in input.motions {
            builder.create_motion(value)?;
        }
        builder.set_draw_order_groups(input.draw_order_groups)?;
        builder.set_display_info(input.display_info)?;
        builder.set_hierarchy_order(input.hierarchy_order)?;
        builder.set_editor_state(input.editor_state)?;
        builder.set_motion_groups(input.motion_groups)?;
        builder.set_motion_roots(input.motion_roots)?;
        builder.set_pose(input.pose)?;
        builder.set_physics(input.physics)?;
        builder.set_missing_attachments(input.missing_attachments)?;
        builder.set_model3_settings(input.model3_settings)?;
        builder.set_package_attachments(input.package_attachments)?;
        Ok(builder.freeze()?.state)
    }
    pub fn part(&self, id: &str) -> Option<&Part> {
        self.content.parts.get(id)
    }
    pub fn part_order(&self) -> &[String] {
        self.content.parts.order()
    }
    pub fn transform(&self, id: &str) -> Option<&Transform> {
        self.content.transforms.get(id)
    }
    pub fn transform_order(&self) -> &[String] {
        self.content.transforms.order()
    }
    pub fn mesh(&self, id: &str) -> Option<&Mesh> {
        self.content.meshes.get(id)
    }
    pub fn mesh_order(&self) -> &[String] {
        self.content.meshes.order()
    }
    pub fn binding(&self, id: &str) -> Option<&MeshBinding> {
        self.content.bindings.get(id)
    }
    pub fn binding_order(&self) -> &[String] {
        self.content.bindings.order()
    }
    pub fn scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.content.scene_bindings.get(id)
    }
    pub fn scene_binding_order(&self) -> &[String] {
        self.content.scene_bindings.order()
    }
    pub fn blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.content.blend_key_tables.get(id)
    }
    pub fn blend_key_table_order(&self) -> &[String] {
        self.content.blend_key_tables.order()
    }
    pub fn blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.content.blend_constraints.get(id)
    }
    pub fn blend_constraint_order(&self) -> &[String] {
        self.content.blend_constraints.order()
    }
    pub fn blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.content.blend_bindings.get(id)
    }
    pub fn blend_binding_order(&self) -> &[String] {
        self.content.blend_bindings.order()
    }
    pub fn glue(&self, id: &str) -> Option<&Glue> {
        self.content.glues.get(id)
    }
    pub fn glue_order(&self) -> &[String] {
        self.content.glues.order()
    }
    pub fn offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.content.offscreens.get(id)
    }
    pub fn offscreen_order(&self) -> &[String] {
        self.content.offscreens.order()
    }
    pub fn expression(&self, id: &str) -> Option<&ExpressionAsset> {
        self.content.expressions.get(id)
    }
    pub fn expression_order(&self) -> &[String] {
        self.content.expressions.order()
    }
    pub fn motion(&self, id: &str) -> Option<&MotionClip> {
        self.content.motions.get(id)
    }
    pub fn motion_order(&self) -> &[String] {
        self.content.motions.order()
    }
    pub fn draw_order_groups(&self) -> &Option<Vec<DrawOrderGroup>> {
        &self.content.draw_order_groups
    }
    pub fn display_info(&self) -> &DisplayInfo {
        &self.content.display_info
    }
    pub fn hierarchy_order(&self) -> &HierarchyOrder {
        &self.content.hierarchy_order
    }
    pub fn editor_state(&self) -> &EditorState {
        &self.content.editor_state
    }
    pub fn motion_groups(&self) -> &Vec<MotionGroup> {
        &self.content.motion_groups
    }
    pub fn motion_roots(&self) -> &Vec<MotionRegistration> {
        &self.content.motion_roots
    }
    pub fn pose(&self) -> &Option<Arc<PoseAsset>> {
        &self.content.pose
    }
    pub fn physics(&self) -> &Option<Arc<PhysicsAsset>> {
        &self.content.physics
    }
    pub fn missing_attachments(&self) -> &Vec<String> {
        &self.content.missing_attachments
    }
    pub fn model3_settings(&self) -> &Model3Settings {
        &self.content.model3_settings
    }
    pub fn package_attachments(&self) -> &Vec<PackageAttachment> {
        &self.content.package_attachments
    }
}
impl Content {
    pub(super) fn contains_top_id(&self, id: &str) -> bool {
        self.id == id
            || self.assets.get(id).is_some()
            || self.parameters.get(id).is_some()
            || self.parts.get(id).is_some()
            || self.transforms.get(id).is_some()
            || self.meshes.get(id).is_some()
            || self.bindings.get(id).is_some()
            || self.scene_bindings.get(id).is_some()
            || self.blend_key_tables.get(id).is_some()
            || self.blend_constraints.get(id).is_some()
            || self.blend_bindings.get(id).is_some()
            || self.glues.get(id).is_some()
            || self.offscreens.get(id).is_some()
            || self.expressions.get(id).is_some()
            || self.motions.get(id).is_some()
    }
}
impl DocumentBuilder {
    pub fn part(&self, id: &str) -> Option<&Part> {
        self.content.parts.get(id)
    }
    pub fn part_order(&self) -> &[String] {
        self.content.parts.order()
    }

    pub fn transform(&self, id: &str) -> Option<&Transform> {
        self.content.transforms.get(id)
    }
    pub fn transform_order(&self) -> &[String] {
        self.content.transforms.order()
    }

    pub fn mesh(&self, id: &str) -> Option<&Mesh> {
        self.content.meshes.get(id)
    }
    pub fn mesh_order(&self) -> &[String] {
        self.content.meshes.order()
    }

    pub fn binding(&self, id: &str) -> Option<&MeshBinding> {
        self.content.bindings.get(id)
    }
    pub fn binding_order(&self) -> &[String] {
        self.content.bindings.order()
    }

    pub fn scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.content.scene_bindings.get(id)
    }
    pub fn scene_binding_order(&self) -> &[String] {
        self.content.scene_bindings.order()
    }

    pub fn blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.content.blend_key_tables.get(id)
    }
    pub fn blend_key_table_order(&self) -> &[String] {
        self.content.blend_key_tables.order()
    }

    pub fn blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.content.blend_constraints.get(id)
    }
    pub fn blend_constraint_order(&self) -> &[String] {
        self.content.blend_constraints.order()
    }

    pub fn blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.content.blend_bindings.get(id)
    }
    pub fn blend_binding_order(&self) -> &[String] {
        self.content.blend_bindings.order()
    }

    pub fn glue(&self, id: &str) -> Option<&Glue> {
        self.content.glues.get(id)
    }
    pub fn glue_order(&self) -> &[String] {
        self.content.glues.order()
    }

    pub fn offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.content.offscreens.get(id)
    }
    pub fn offscreen_order(&self) -> &[String] {
        self.content.offscreens.order()
    }

    pub fn expression(&self, id: &str) -> Option<&ExpressionAsset> {
        self.content.expressions.get(id)
    }
    pub fn expression_order(&self) -> &[String] {
        self.content.expressions.order()
    }

    pub fn motion(&self, id: &str) -> Option<&MotionClip> {
        self.content.motions.get(id)
    }
    pub fn motion_order(&self) -> &[String] {
        self.content.motions.order()
    }

    pub fn draw_order_groups(&self) -> &Option<Vec<DrawOrderGroup>> {
        &self.content.draw_order_groups
    }
    pub fn set_draw_order_groups(
        &mut self,
        value: Option<Vec<DrawOrderGroup>>,
    ) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::DrawOrderGroups);
            this.content.draw_order_groups = Arc::new(value);
            Ok(())
        })
    }
    pub fn display_info(&self) -> &DisplayInfo {
        &self.content.display_info
    }
    pub fn set_display_info(&mut self, value: DisplayInfo) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::DisplayInfo);
            this.content.display_info = Arc::new(value);
            Ok(())
        })
    }
    pub fn hierarchy_order(&self) -> &HierarchyOrder {
        &self.content.hierarchy_order
    }
    pub fn set_hierarchy_order(&mut self, value: HierarchyOrder) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::HierarchyOrder);
            this.content.hierarchy_order = Arc::new(value);
            Ok(())
        })
    }
    pub fn editor_state(&self) -> &EditorState {
        &self.content.editor_state
    }
    pub fn set_editor_state(&mut self, value: EditorState) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::EditorState);
            this.content.editor_state = Arc::new(value);
            Ok(())
        })
    }
    pub fn motion_groups(&self) -> &Vec<MotionGroup> {
        &self.content.motion_groups
    }
    pub fn set_motion_groups(&mut self, value: Vec<MotionGroup>) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::MotionGroups);
            this.content.motion_groups = Arc::new(value);
            Ok(())
        })
    }
    pub fn motion_roots(&self) -> &Vec<MotionRegistration> {
        &self.content.motion_roots
    }
    pub fn set_motion_roots(&mut self, value: Vec<MotionRegistration>) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::MotionRoots);
            this.content.motion_roots = Arc::new(value);
            Ok(())
        })
    }
    pub fn pose(&self) -> &Option<Arc<PoseAsset>> {
        &self.content.pose
    }
    pub fn set_pose(&mut self, value: Option<Arc<PoseAsset>>) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::Pose);
            this.content.pose = Arc::new(value);
            Ok(())
        })
    }
    pub fn physics(&self) -> &Option<Arc<PhysicsAsset>> {
        &self.content.physics
    }
    pub fn set_physics(&mut self, value: Option<Arc<PhysicsAsset>>) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::Physics);
            this.content.physics = Arc::new(value);
            Ok(())
        })
    }
    pub fn missing_attachments(&self) -> &Vec<String> {
        &self.content.missing_attachments
    }
    pub fn set_missing_attachments(&mut self, value: Vec<String>) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::MissingAttachments);
            this.content.missing_attachments = Arc::new(value);
            Ok(())
        })
    }
    pub fn model3_settings(&self) -> &Model3Settings {
        &self.content.model3_settings
    }
    pub fn set_model3_settings(&mut self, value: Model3Settings) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::Model3Settings);
            this.content.model3_settings = Arc::new(value);
            Ok(())
        })
    }
    pub fn package_attachments(&self) -> &Vec<PackageAttachment> {
        &self.content.package_attachments
    }
    pub fn set_package_attachments(&mut self, value: Vec<PackageAttachment>) -> Result<(), Status> {
        self.write(|this| {
            this.roots_touched.insert(RootDomain::PackageAttachments);
            this.content.package_attachments = Arc::new(value);
            Ok(())
        })
    }
    pub fn reorder(&mut self, domain: RecordDomain, order: Vec<String>) -> Result<(), Status> {
        self.write(|this| {
            match domain {
                RecordDomain::Asset => {
                    this.content.assets.set_order(order)?;
                    this.touched_assets
                        .extend(this.content.assets.order().iter().cloned());
                }
                RecordDomain::Parameter => {
                    this.content.parameters.set_order(order)?;
                    this.touched_parameters
                        .extend(this.content.parameters.order().iter().cloned());
                }
                RecordDomain::Part => {
                    this.content.parts.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.parts.order().iter().cloned());
                }
                RecordDomain::Transform => {
                    this.content.transforms.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.transforms.order().iter().cloned());
                }
                RecordDomain::Mesh => {
                    this.content.meshes.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.meshes.order().iter().cloned());
                }
                RecordDomain::Binding => {
                    this.content.bindings.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.bindings.order().iter().cloned());
                }
                RecordDomain::SceneBinding => {
                    this.content.scene_bindings.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.scene_bindings.order().iter().cloned());
                }
                RecordDomain::BlendKeyTable => {
                    this.content.blend_key_tables.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.blend_key_tables.order().iter().cloned());
                }
                RecordDomain::BlendConstraint => {
                    this.content.blend_constraints.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.blend_constraints.order().iter().cloned());
                }
                RecordDomain::BlendBinding => {
                    this.content.blend_bindings.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.blend_bindings.order().iter().cloned());
                }
                RecordDomain::Glue => {
                    this.content.glues.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.glues.order().iter().cloned());
                }
                RecordDomain::Offscreen => {
                    this.content.offscreens.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.offscreens.order().iter().cloned());
                }
                RecordDomain::Expression => {
                    this.content.expressions.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.expressions.order().iter().cloned());
                }
                RecordDomain::Motion => {
                    this.content.motions.set_order(order)?;
                    this.touched
                        .entry(domain)
                        .or_default()
                        .extend(this.content.motions.order().iter().cloned());
                }
            }
            Ok(())
        })
    }
    pub(super) fn finish_domains(&mut self, writes: &mut WriteSet) {
        if self.content.parts.finish(
            &self.base.content.parts,
            self.touched.remove(&RecordDomain::Part).unwrap_or_default(),
            RecordDomain::Part,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Part);
        }
        if self.content.transforms.finish(
            &self.base.content.transforms,
            self.touched
                .remove(&RecordDomain::Transform)
                .unwrap_or_default(),
            RecordDomain::Transform,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Transform);
        }
        if self.content.meshes.finish(
            &self.base.content.meshes,
            self.touched.remove(&RecordDomain::Mesh).unwrap_or_default(),
            RecordDomain::Mesh,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Mesh);
        }
        if self.content.bindings.finish(
            &self.base.content.bindings,
            self.touched
                .remove(&RecordDomain::Binding)
                .unwrap_or_default(),
            RecordDomain::Binding,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Binding);
        }
        if self.content.scene_bindings.finish(
            &self.base.content.scene_bindings,
            self.touched
                .remove(&RecordDomain::SceneBinding)
                .unwrap_or_default(),
            RecordDomain::SceneBinding,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::SceneBinding);
        }
        if self.content.blend_key_tables.finish(
            &self.base.content.blend_key_tables,
            self.touched
                .remove(&RecordDomain::BlendKeyTable)
                .unwrap_or_default(),
            RecordDomain::BlendKeyTable,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::BlendKeyTable);
        }
        if self.content.blend_constraints.finish(
            &self.base.content.blend_constraints,
            self.touched
                .remove(&RecordDomain::BlendConstraint)
                .unwrap_or_default(),
            RecordDomain::BlendConstraint,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::BlendConstraint);
        }
        if self.content.blend_bindings.finish(
            &self.base.content.blend_bindings,
            self.touched
                .remove(&RecordDomain::BlendBinding)
                .unwrap_or_default(),
            RecordDomain::BlendBinding,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::BlendBinding);
        }
        if self.content.glues.finish(
            &self.base.content.glues,
            self.touched.remove(&RecordDomain::Glue).unwrap_or_default(),
            RecordDomain::Glue,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Glue);
        }
        if self.content.offscreens.finish(
            &self.base.content.offscreens,
            self.touched
                .remove(&RecordDomain::Offscreen)
                .unwrap_or_default(),
            RecordDomain::Offscreen,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Offscreen);
        }
        if self.content.expressions.finish(
            &self.base.content.expressions,
            self.touched
                .remove(&RecordDomain::Expression)
                .unwrap_or_default(),
            RecordDomain::Expression,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Expression);
        }
        if self.content.motions.finish(
            &self.base.content.motions,
            self.touched
                .remove(&RecordDomain::Motion)
                .unwrap_or_default(),
            RecordDomain::Motion,
            &mut writes.records,
        ) {
            writes.orders.insert(RecordDomain::Motion);
        }
        if self.roots_touched.contains(&RootDomain::DrawOrderGroups) {
            if self.content.draw_order_groups != self.base.content.draw_order_groups {
                writes.roots.insert(RootDomain::DrawOrderGroups);
            } else {
                self.content.draw_order_groups = self.base.content.draw_order_groups.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::DisplayInfo) {
            if self.content.display_info != self.base.content.display_info {
                writes.roots.insert(RootDomain::DisplayInfo);
            } else {
                self.content.display_info = self.base.content.display_info.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::HierarchyOrder) {
            if self.content.hierarchy_order != self.base.content.hierarchy_order {
                writes.roots.insert(RootDomain::HierarchyOrder);
            } else {
                self.content.hierarchy_order = self.base.content.hierarchy_order.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::EditorState) {
            if self.content.editor_state != self.base.content.editor_state {
                writes.roots.insert(RootDomain::EditorState);
            } else {
                self.content.editor_state = self.base.content.editor_state.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::MotionGroups) {
            if self.content.motion_groups != self.base.content.motion_groups {
                writes.roots.insert(RootDomain::MotionGroups);
            } else {
                self.content.motion_groups = self.base.content.motion_groups.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::MotionRoots) {
            if self.content.motion_roots != self.base.content.motion_roots {
                writes.roots.insert(RootDomain::MotionRoots);
            } else {
                self.content.motion_roots = self.base.content.motion_roots.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::Pose) {
            if self.content.pose != self.base.content.pose {
                writes.roots.insert(RootDomain::Pose);
            } else {
                self.content.pose = self.base.content.pose.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::Physics) {
            if self.content.physics != self.base.content.physics {
                writes.roots.insert(RootDomain::Physics);
            } else {
                self.content.physics = self.base.content.physics.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::MissingAttachments) {
            if self.content.missing_attachments != self.base.content.missing_attachments {
                writes.roots.insert(RootDomain::MissingAttachments);
            } else {
                self.content.missing_attachments = self.base.content.missing_attachments.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::Model3Settings) {
            if self.content.model3_settings != self.base.content.model3_settings {
                writes.roots.insert(RootDomain::Model3Settings);
            } else {
                self.content.model3_settings = self.base.content.model3_settings.clone();
            }
        }
        if self.roots_touched.contains(&RootDomain::PackageAttachments) {
            if self.content.package_attachments != self.base.content.package_attachments {
                writes.roots.insert(RootDomain::PackageAttachments);
            } else {
                self.content.package_attachments = self.base.content.package_attachments.clone();
            }
        }
    }
}

// Compile-time boilerplate only: callers retain distinct domain types and methods.
// Cross-record references may be assembled in any order and are checked at freeze.
macro_rules! record_writes {
    ($field:ident, $ty:ty, $domain:ident, $create:ident, $replace:ident, $remove:ident) => {
        impl DocumentBuilder {
            pub fn $create(&mut self, value: $ty) -> Result<(), Status> {
                self.write(|this| {
                    this.check_new_id(&value.id)?;
                    this.touched
                        .entry(RecordDomain::$domain)
                        .or_default()
                        .insert(value.id.clone());
                    this.content.$field.insert(value.id.clone(), value);
                    Ok(())
                })
            }
            pub fn $replace(&mut self, value: $ty) -> Result<(), Status> {
                self.write(|this| {
                    if this.content.$field.get(&value.id).is_none() {
                        return Err(Status::error("MISSING_OBJECT", &value.id));
                    }
                    this.touched
                        .entry(RecordDomain::$domain)
                        .or_default()
                        .insert(value.id.clone());
                    this.content.$field.insert(value.id.clone(), value);
                    Ok(())
                })
            }
            pub fn $remove(&mut self, id: &str) -> Result<(), Status> {
                self.write(|this| {
                    if this.content.$field.get(id).is_none() {
                        return Err(Status::error("MISSING_OBJECT", id));
                    }
                    this.touched
                        .entry(RecordDomain::$domain)
                        .or_default()
                        .insert(id.into());
                    this.content.$field.remove(id);
                    Ok(())
                })
            }
        }
    };
}
record_writes!(parts, Part, Part, create_part, replace_part, remove_part);
record_writes!(
    transforms,
    Transform,
    Transform,
    create_transform,
    replace_transform,
    remove_transform
);
record_writes!(meshes, Mesh, Mesh, create_mesh, replace_mesh, remove_mesh);
record_writes!(
    bindings,
    MeshBinding,
    Binding,
    create_binding,
    replace_binding,
    remove_binding
);
record_writes!(
    scene_bindings,
    SceneBinding,
    SceneBinding,
    create_scene_binding,
    replace_scene_binding,
    remove_scene_binding
);
record_writes!(
    blend_key_tables,
    BlendShapeKeyTable,
    BlendKeyTable,
    create_blend_key_table,
    replace_blend_key_table,
    remove_blend_key_table
);
record_writes!(
    blend_constraints,
    BlendShapeConstraint,
    BlendConstraint,
    create_blend_constraint,
    replace_blend_constraint,
    remove_blend_constraint
);
record_writes!(
    blend_bindings,
    BlendShapeBinding,
    BlendBinding,
    create_blend_binding,
    replace_blend_binding,
    remove_blend_binding
);
record_writes!(glues, Glue, Glue, create_glue, replace_glue, remove_glue);
record_writes!(
    offscreens,
    Offscreen,
    Offscreen,
    create_offscreen,
    replace_offscreen,
    remove_offscreen
);
record_writes!(
    expressions,
    ExpressionAsset,
    Expression,
    create_expression,
    replace_expression,
    remove_expression
);
record_writes!(
    motions,
    MotionClip,
    Motion,
    create_motion,
    replace_motion,
    remove_motion
);
