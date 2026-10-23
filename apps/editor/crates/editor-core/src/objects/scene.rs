use super::*;
impl ObjectIndex {
    pub(crate) fn refresh_scene_label(&mut self, model: &Model, object: &ObjectRef) -> bool {
        let name = match object.kind {
            ObjectKind::Part => model.get_part(&object.id).map(|v| v.name.as_str()),
            ObjectKind::Mesh => model.get_mesh(&object.id).map(|v| v.name.as_str()),
            ObjectKind::Deformer => model.get_transform(&object.id).map(|v| v.name.as_str()),
            ObjectKind::Offscreen => model.get_offscreen(&object.id).map(|v| v.name.as_str()),
            ObjectKind::Glue => model.get_glue(&object.id).map(|v| v.name.as_str()),
            ObjectKind::DrawOrderGroup => {
                if object.id == model.id() {
                    Some("Drawing")
                } else {
                    model.get_part(&object.id).map(|v| v.name.as_str())
                }
            }
            _ => None,
        };
        let Some(name) = name else { return false };
        let Some(&slot) = self.structure.slots.get(object) else {
            return false;
        };
        let info = &mut self.objects[slot];
        info.name = if name.is_empty() {
            object.id.clone()
        } else {
            name.into()
        };
        if object.kind != ObjectKind::DrawOrderGroup {
            info.properties["name"] = serde_json::json!(name);
        }
        true
    }
}
