use super::*;
pub trait AllocateModelMetadata: Sized {
    fn allocate() -> Self;
}
impl AllocateModelMetadata for ModelParameterGroup {
    fn allocate() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Parameter Group".into(),
            parameters: vec![],
            extensions: Default::default(),
        }
    }
}
impl AllocateModelMetadata for ModelHitArea {
    fn allocate() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Hit Area".into(),
            mesh: ModelTargetRef::Unspecified,
            extensions: Default::default(),
        }
    }
}
