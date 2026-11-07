use super::*;
pub(crate) enum RecordView<'a, T> {
    Shared(&'a HashMap<String, Arc<T>>),
}
impl<'a, T> RecordView<'a, T> {
    pub fn get(&self, id: &str) -> Option<&'a T> {
        match self {
            Self::Shared(m) => m.get(id).map(Arc::as_ref),
        }
    }
    pub fn contains_key(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
    pub fn iter(&self) -> Box<dyn Iterator<Item = (&'a String, &'a T)> + 'a> {
        match self {
            Self::Shared(m) => Box::new(m.iter().map(|(id, v)| (id, v.as_ref()))),
        }
    }
    pub fn values(&self) -> impl Iterator<Item = &'a T> + 'a {
        self.iter().map(|(_, v)| v)
    }
    pub fn keys(&self) -> impl Iterator<Item = &'a String> + 'a {
        self.iter().map(|(id, _)| id)
    }
}
impl<'a, T> IntoIterator for &RecordView<'a, T> {
    type Item = (&'a String, &'a T);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
#[derive(Clone, Copy)]
pub(crate) struct AssetIds<'a>(pub &'a [String]);
impl AssetIds<'_> {
    pub fn contains_key(&self, id: &str) -> bool {
        self.0.iter().any(|v| v == id)
    }
}
#[derive(Clone)]
pub(crate) struct ModelContent<'a> {
    pub source_canvas: Option<Canvas>,
    pub id: &'a String,
    pub image_availability: ImageAvailability<'a>,
    pub assets: AssetIds<'a>,
    pub parameter_order: &'a Vec<String>,
    pub part_order: &'a Vec<String>,
    pub transform_order: &'a Vec<String>,
    pub offscreen_order: &'a Vec<String>,
    pub blend_binding_order: &'a Vec<String>,
    pub hierarchy_order: &'a HierarchyOrder,
    pub parameters: RecordView<'a, Parameter>,
    pub parts: RecordView<'a, Part>,
    pub transforms: RecordView<'a, Transform>,
    pub meshes: RecordView<'a, Mesh>,
    pub mesh_order: &'a Vec<String>,
    pub bindings: RecordView<'a, MeshBinding>,
    pub binding_order: &'a Vec<String>,
    pub scene_bindings: RecordView<'a, SceneBinding>,
    pub scene_binding_order: &'a Vec<String>,
    pub blend_key_tables: RecordView<'a, BlendShapeKeyTable>,
    pub blend_key_table_order: &'a Vec<String>,
    pub blend_constraints: RecordView<'a, BlendShapeConstraint>,
    pub blend_constraint_order: &'a Vec<String>,
    pub blend_bindings: RecordView<'a, BlendShapeBinding>,
    pub glues: RecordView<'a, Glue>,
    pub glue_order: &'a Vec<String>,
    pub offscreens: RecordView<'a, Offscreen>,
    pub expression_order: &'a Vec<String>,
    pub draw_order_groups: &'a Option<Vec<crate::draw_order::DrawOrderGroup>>,
    pub expressions: RecordView<'a, ExpressionAsset>,
    pub motion_order: &'a Vec<String>,
    pub motions: RecordView<'a, MotionClip>,
    pub display_info: &'a DisplayInfo,
    pub editor_state: &'a EditorState,
    pub motion_groups: &'a Vec<MotionGroup>,
    pub motion_roots: &'a Vec<MotionRegistration>,
    pub pose: &'a Option<Arc<PoseAsset>>,
    pub physics: &'a Option<Arc<PhysicsAsset>>,
    pub model3_settings: &'a Model3Settings,
    pub package_attachments: &'a Vec<PackageAttachment>,
}

impl<T> Clone for RecordView<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Shared(m) => Self::Shared(m),
        }
    }
}
impl<T: Clone> RecordView<'_, T> {
    pub fn shared_clones(&self) -> HashMap<String, Arc<T>> {
        match self {
            Self::Shared(m) => (*m).clone(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ImageAvailability<'a> {
    Shared(&'a HashMap<String, Arc<zipzi_contract::kernel::AssetRecord>>),
}
impl ImageAvailability<'_> {
    pub fn contains(&self, id: &str) -> bool {
        match self {
            Self::Shared(m) => m.get(id).is_some_and(|a| {
                matches!(
                    a.content,
                    zipzi_contract::kernel::AssetContent::Image { .. }
                )
            }),
        }
    }
}
