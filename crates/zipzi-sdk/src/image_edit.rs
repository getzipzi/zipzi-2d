//! Image creation metadata; resource preparation remains an independent lease.
use crate::{EditSession, PreparedImageContent, SdkError};
use serde::{Deserialize, Serialize};

zipzi_core::api_record! {
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageCreateSpec {
    pub id: Option<String>,
    pub name: String,
}
}
impl EditSession<'_> {
    pub fn create_image_spec(
        &mut self,
        spec: ImageCreateSpec,
        image: &PreparedImageContent,
    ) -> Result<String, SdkError> {
        let id = spec.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.create_prepared_image(id.clone(), spec.name, image)?;
        Ok(id)
    }

    /// Pin only the requested immutable candidate image, without copying its
    /// document or registering content in the formal session.
    pub fn memory_image(&self, id: &str) -> Option<PreparedImageContent> {
        let asset = self.candidate.as_ref()?.get_asset(id)?;
        let content = asset.content.as_ref()?;
        self.prepared_images
            .iter()
            .rev()
            .find(|image| image.content() == content)
            .cloned()
            .or_else(|| self.resource_store.memory_image(asset))
    }
}
