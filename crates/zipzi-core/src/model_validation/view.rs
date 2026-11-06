use super::*;
pub(crate) enum RecordView<'a, T> {
    Owned(&'a HashMap<String, T>),
    Shared(&'a HashMap<String, Arc<T>>),
}
impl<'a, T> RecordView<'a, T> {
    pub fn get(&self, id: &str) -> Option<&'a T> {
        match self {
            Self::Owned(m) => m.get(id),
            Self::Shared(m) => m.get(id).map(Arc::as_ref),
        }
    }
    pub fn contains_key(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
    pub fn iter(&self) -> Box<dyn Iterator<Item = (&'a String, &'a T)> + 'a> {
        match self {
            Self::Owned(m) => Box::new(m.iter()),
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
pub(crate) struct AssetIds<'a>(pub &'a [String]);
impl AssetIds<'_> {
    pub fn contains_key(&self, id: &str) -> bool {
        self.0.iter().any(|v| v == id)
    }
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }
}
pub(crate) struct ModelContent<'a> {
    pub id: &'a String,
    pub assets: AssetIds<'a>,
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
    pub expressions: RecordView<'a, ExpressionAsset>,
    pub motions: RecordView<'a, MotionClip>,
    pub display_info: &'a DisplayInfo,
    pub motion_groups: &'a Vec<MotionGroup>,
    pub motion_roots: &'a Vec<MotionRegistration>,
    pub pose: &'a Option<Arc<PoseAsset>>,
    pub physics: &'a Option<Arc<PhysicsAsset>>,
    pub model3_settings: &'a Model3Settings,
    pub package_attachments: &'a Vec<PackageAttachment>,
}
