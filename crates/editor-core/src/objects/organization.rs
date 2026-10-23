use super::*;
use serde_json::json;
impl ObjectIndex {
    pub(crate) fn refresh_organization_document(&mut self, m: &Model) -> bool {
        let object = ObjectRef::new(ObjectKind::Document, m.id());
        let Some(&slot) = self.structure.slots.get(&object) else {
            return false;
        };
        self.objects[slot].properties["display_info"] = json!(m.display_info());
        self.objects[slot].properties["draw_groups"] = json!(m.draw_order_groups());
        true
    }

    pub(crate) fn refresh_organization_properties(
        &mut self,
        m: &Model,
        kind: kasane_sdk::ObjectKind,
        id: &str,
    ) -> bool {
        let (object, name, properties, enabled) = match kind {
            kasane_sdk::ObjectKind::Parameter => {
                let Some(p) = m.get_parameter(id) else {
                    return false;
                };
                (
                    ObjectRef::new(ObjectKind::Parameter, id),
                    p.name.clone(),
                    json!(p),
                    None,
                )
            }
            kasane_sdk::ObjectKind::CdiParameterGroup => {
                let Some(g) = m.get_parameter_group(id) else {
                    return false;
                };
                (
                    ObjectRef::new(ObjectKind::ParameterGroup, id),
                    g.name.clone(),
                    json!(g),
                    None,
                )
            }
            kasane_sdk::ObjectKind::Part => {
                let Some(p) = m.get_part(id) else {
                    return false;
                };
                (
                    ObjectRef::new(ObjectKind::Part, id),
                    p.name.clone(),
                    json!(p),
                    Some(p.enabled),
                )
            }
            _ => return false,
        };
        let Some(&slot) = self.structure.slots.get(&object) else {
            return false;
        };
        let name = if name.is_empty() { id.into() } else { name };
        if self.objects[slot].name != name
            || self.objects[slot].properties != properties
            || self.objects[slot].enabled != enabled
        {
            let info = &mut self.objects[slot];
            info.name = name;
            info.properties = properties;
            info.enabled = enabled;
        }
        true
    }
}
