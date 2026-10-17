//! Inspector field eligibility and deformer display payloads.
use super::ObjectKind;
use serde_json::{Value, json};

/// Only scalar authoring fields with existing SDK validation get inspector controls.
pub fn editable_property(kind: ObjectKind, field: &str) -> bool {
    crate::property_specs(kind)
        .iter()
        .any(|spec| spec.id.path() == field)
}

pub(super) fn deformer_properties(x: &kasane_core::Transform) -> Value {
    json!({"name":x.name,"runtime_id":x.runtime_id,"kind":x.kind(),"part_id":x.part(),"parent_id":x.parent(),"appearance":x.appearance,"enabled":x.enabled,"warp":x.warp(),"rotation":x.rotation()})
}
