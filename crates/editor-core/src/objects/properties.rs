//! Inspector field eligibility and deformer display payloads.
use super::ObjectKind;
use serde_json::{Value, json};

impl super::ObjectIndex {
    /// These mesh fields cannot change identities, relation edges or draw groups.
    pub(crate) fn refresh_mesh_presentation(
        &mut self,
        model: &kasane_core::Document,
        object: &super::ObjectRef,
    ) -> bool {
        let Some(mesh) = model.get_mesh(&object.id) else {
            return false;
        };
        let Some(&slot) = self.slots.get(object) else {
            return false;
        };
        let info = &mut self.objects[slot];
        info.name = if mesh.name.is_empty() {
            object.id.clone()
        } else {
            mesh.name.clone()
        };
        info.enabled = Some(mesh.enabled);
        info.properties["name"] = json!(mesh.name);
        info.properties["enabled"] = json!(mesh.enabled);
        info.properties["appearance"] = json!(mesh.appearance);
        true
    }

    /// Appearance candidates preserve identities and relation edges. Copy their
    /// payloads without rebuilding bindings or influence paths.
    pub(crate) fn refresh_property_preview(
        &mut self,
        model: &kasane_core::Document,
        object: &super::ObjectRef,
        keyform_order_changed: bool,
    ) -> bool {
        let Ok(values) = crate::property::values_from(model, object) else {
            return false;
        };
        let Some(&slot) = self.slots.get(object) else {
            return false;
        };
        let mut draw_order_changed = keyform_order_changed;
        for spec in crate::property_specs(object.kind)
            .iter()
            .filter(|s| s.preview)
        {
            let Some(value) = values.get(&spec.key) else {
                return false;
            };
            let mut field = &mut self.objects[slot].properties;
            for key in spec.path.split('.') {
                let Some(next) = field.get_mut(key) else {
                    return false;
                };
                field = next;
            }
            let value = value.json();
            draw_order_changed |= spec.refresh_draw_groups && *field != value;
            *field = value;
        }
        if draw_order_changed {
            // Draw-order candidates also affect the resolved group bounds.
            self.draw_groups = kasane_core::draw_order::resolved_groups(model);
        }
        true
    }
}

/// Writable authoring fields are guarded by the same descriptor policy as the inspector.
pub fn editable_property(kind: ObjectKind, field: &str) -> bool {
    crate::property_specs(kind)
        .iter()
        .any(|spec| spec.path == field && !spec.readonly)
}

pub(super) fn deformer_properties(x: &kasane_core::Transform) -> Value {
    json!({"name":x.name,"runtime_id":x.runtime_id,"kind":x.kind(),"part_id":x.part(),"parent_id":x.parent(),"appearance":x.appearance,"enabled":x.enabled,"warp":x.warp(),"rotation":x.rotation()})
}
