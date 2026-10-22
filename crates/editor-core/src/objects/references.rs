use super::*;
impl ObjectIndex {
    pub(crate) fn refresh_reference_labels(&mut self, model: &Model, ids: &[String]) -> bool {
        for id in ids {
            let (kind, name) = if let Some(g) = model.get_model_parameter_group(id) {
                (ObjectKind::ModelParameterGroup, g.name.as_str())
            } else if let Some(a) = model.get_model_hit_area(id) {
                (ObjectKind::ModelHitArea, a.name.as_str())
            } else {
                return false;
            };
            let r = ObjectRef::new(kind, id);
            let Some(&slot) = self.structure.slots.get(&r) else {
                return false;
            };
            self.objects[slot].name = if name.is_empty() {
                id.clone()
            } else {
                name.into()
            };
            self.objects[slot].properties["name"] = serde_json::json!(name);
        }
        if let Some(&slot) = self
            .structure
            .slots
            .get(&ObjectRef::new(ObjectKind::Document, model.id()))
        {
            self.objects[slot].properties["model_settings"] =
                serde_json::json!(model.model3_settings());
        }
        true
    }
}
