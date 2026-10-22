use super::*;
impl ObjectIndex {
    pub(crate) fn refresh_resource_labels(&mut self, model: &Model, ids: &[String]) -> bool {
        for id in ids {
            let Some(asset) = model.get_asset(id) else {
                return false;
            };
            let Some(&slot) = self
                .structure
                .slots
                .get(&ObjectRef::new(ObjectKind::Texture, id))
            else {
                return false;
            };
            self.objects[slot].name = if asset.name.is_empty() {
                id.clone()
            } else {
                asset.name.clone()
            };
            self.objects[slot].properties["name"] = serde_json::json!(asset.name);
        }
        true
    }
    pub(crate) fn refresh_document_metadata(&mut self, model: &Model) -> bool {
        let Some(&slot) = self
            .structure
            .slots
            .get(&ObjectRef::new(ObjectKind::Document, model.id()))
        else {
            return false;
        };
        let properties = &mut self.objects[slot].properties;
        properties["export_settings"] = serde_json::json!(model.export_settings());
        properties["source_canvas"] = serde_json::json!(model.source_canvas());
        properties["model_settings"] = serde_json::json!(model.model3_settings());
        true
    }
}
