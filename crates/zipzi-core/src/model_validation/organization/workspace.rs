use super::*;
pub(crate) struct SceneWorkspace<'a> {
    source: ModelContent<'a>,
    pub parameters: HashMap<String, Arc<Parameter>>,
    parameter_order: Vec<String>,
    pub parts: HashMap<String, Arc<Part>>,
    part_order: Vec<String>,
    pub meshes: HashMap<String, Arc<Mesh>>,
    mesh_order: Vec<String>,
    pub transforms: HashMap<String, Arc<Transform>>,
    transform_order: Vec<String>,
    pub offscreens: HashMap<String, Arc<Offscreen>>,
    offscreen_order: Vec<String>,
    pub bindings: HashMap<String, Arc<MeshBinding>>,
    binding_order: Vec<String>,
    pub scene_bindings: HashMap<String, Arc<SceneBinding>>,
    scene_binding_order: Vec<String>,
    pub blend_bindings: HashMap<String, Arc<BlendShapeBinding>>,
    blend_binding_order: Vec<String>,
    pub glues: HashMap<String, Arc<Glue>>,
    glue_order: Vec<String>,
    hierarchy: HierarchyOrder,
    info: DisplayInfo,
    model3: Model3Settings,
    drawing: Option<Vec<DrawOrderGroup>>,
}
impl<'a> SceneWorkspace<'a> {
    pub fn new(model: &Model<'a>) -> Self {
        Self {
            source: model.content.clone(),
            parameters: model.content.parameters.shared_clones(),
            parameter_order: model.content.parameter_order.clone(),
            parts: model.content.parts.shared_clones(),
            part_order: model.content.part_order.clone(),
            meshes: model.content.meshes.shared_clones(),
            mesh_order: model.content.mesh_order.clone(),
            transforms: model.content.transforms.shared_clones(),
            transform_order: model.content.transform_order.clone(),
            offscreens: model.content.offscreens.shared_clones(),
            offscreen_order: model.content.offscreen_order.clone(),
            bindings: model.content.bindings.shared_clones(),
            binding_order: model.content.binding_order.clone(),
            scene_bindings: model.content.scene_bindings.shared_clones(),
            scene_binding_order: model.content.scene_binding_order.clone(),
            blend_bindings: model.content.blend_bindings.shared_clones(),
            blend_binding_order: model.content.blend_binding_order.clone(),
            glues: model.content.glues.shared_clones(),
            glue_order: model.content.glue_order.clone(),
            hierarchy: model.hierarchy_order().clone(),
            info: model.display_info().clone(),
            model3: model.model3_settings().clone(),
            drawing: model.content.draw_order_groups.clone(),
        }
    }
    pub fn model(&self) -> Model<'_> {
        let mut c = self.source.clone();
        c.parameters = RecordView::Shared(&self.parameters);
        c.parameter_order = &self.parameter_order;
        c.parts = RecordView::Shared(&self.parts);
        c.part_order = &self.part_order;
        c.meshes = RecordView::Shared(&self.meshes);
        c.mesh_order = &self.mesh_order;
        c.transforms = RecordView::Shared(&self.transforms);
        c.transform_order = &self.transform_order;
        c.offscreens = RecordView::Shared(&self.offscreens);
        c.offscreen_order = &self.offscreen_order;
        c.bindings = RecordView::Shared(&self.bindings);
        c.binding_order = &self.binding_order;
        c.scene_bindings = RecordView::Shared(&self.scene_bindings);
        c.scene_binding_order = &self.scene_binding_order;
        c.blend_bindings = RecordView::Shared(&self.blend_bindings);
        c.blend_binding_order = &self.blend_binding_order;
        c.glues = RecordView::Shared(&self.glues);
        c.glue_order = &self.glue_order;
        c.hierarchy_order = &self.hierarchy;
        c.display_info = &self.info;
        c.model3_settings = &self.model3;
        c.draw_order_groups = &self.drawing;
        Model::new(c)
    }
    pub fn apply(&mut self, delta: &Delta) {
        self.parameters.retain(|id, _| !delta.removed.contains(id));
        self.parameter_order
            .retain(|id| !delta.removed.contains(id));
        for v in &delta.parameters {
            if !self.parameters.contains_key(&v.id) {
                self.parameter_order.push(v.id.clone());
            }
            self.parameters.insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.parts.retain(|id, _| !delta.removed.contains(id));
        self.part_order.retain(|id| !delta.removed.contains(id));
        for v in &delta.parts {
            if !self.parts.contains_key(&v.id) {
                self.part_order.push(v.id.clone());
            }
            self.parts.insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.meshes.retain(|id, _| !delta.removed.contains(id));
        self.mesh_order.retain(|id| !delta.removed.contains(id));
        for v in &delta.meshes {
            if !self.meshes.contains_key(&v.id) {
                self.mesh_order.push(v.id.clone());
            }
            self.meshes.insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.transforms.retain(|id, _| !delta.removed.contains(id));
        self.transform_order
            .retain(|id| !delta.removed.contains(id));
        for v in &delta.transforms {
            if !self.transforms.contains_key(&v.id) {
                self.transform_order.push(v.id.clone());
            }
            self.transforms.insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.offscreens.retain(|id, _| !delta.removed.contains(id));
        self.offscreen_order
            .retain(|id| !delta.removed.contains(id));
        for v in &delta.offscreens {
            if !self.offscreens.contains_key(&v.id) {
                self.offscreen_order.push(v.id.clone());
            }
            self.offscreens.insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.bindings.retain(|id, _| !delta.removed.contains(id));
        self.binding_order.retain(|id| !delta.removed.contains(id));
        for v in &delta.bindings {
            if !self.bindings.contains_key(&v.id) {
                self.binding_order.push(v.id.clone());
            }
            self.bindings.insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.scene_bindings
            .retain(|id, _| !delta.removed.contains(id));
        self.scene_binding_order
            .retain(|id| !delta.removed.contains(id));
        for v in &delta.scene_bindings {
            if !self.scene_bindings.contains_key(&v.id) {
                self.scene_binding_order.push(v.id.clone());
            }
            self.scene_bindings
                .insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.blend_bindings
            .retain(|id, _| !delta.removed.contains(id));
        self.blend_binding_order
            .retain(|id| !delta.removed.contains(id));
        for v in &delta.blends {
            if !self.blend_bindings.contains_key(&v.id) {
                self.blend_binding_order.push(v.id.clone());
            }
            self.blend_bindings
                .insert(v.id.clone(), Arc::new(v.clone()));
        }
        self.glues.retain(|id, _| !delta.removed.contains(id));
        self.glue_order.retain(|id| !delta.removed.contains(id));
        for v in &delta.glues {
            if !self.glues.contains_key(&v.id) {
                self.glue_order.push(v.id.clone());
            }
            self.glues.insert(v.id.clone(), Arc::new(v.clone()));
        }
        for (id, parent) in &delta.parents {
            if let Some(p) = self.parts.get_mut(id) {
                Arc::make_mut(p).parent_id = parent.clone();
            } else if let Some(m) = self.meshes.get_mut(id) {
                Arc::make_mut(m).part_id = parent.clone();
            } else if let Some(t) = self.transforms.get_mut(id) {
                Arc::make_mut(t).part_id = (!parent.is_empty()).then(|| parent.clone().into());
            } else if let Some(o) = self.offscreens.get_mut(id) {
                Arc::make_mut(o).part_id = parent.clone();
            }
        }
        if let Some(v) = &delta.groups {
            self.info = v.clone();
        }
        if let Some(v) = &delta.hierarchy {
            self.hierarchy = v.clone();
        }
        if let Some(v) = &delta.drawing {
            self.drawing = Some(v.clone());
        }
        if let Some(v) = &delta.model3 {
            self.model3 = v.clone();
        }
    }
}
