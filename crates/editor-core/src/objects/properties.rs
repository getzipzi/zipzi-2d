//! Inspector field eligibility and deformer display payloads.
use super::ObjectKind;
use serde_json::{Value, json};

/// Only scalar authoring fields with existing SDK validation get inspector controls.
pub fn editable_property(kind: ObjectKind, field: &str) -> bool {
    use ObjectKind as K;
    match kind {
        K::Document => matches!(
            field,
            "canvas.width" | "canvas.height" | "canvas.pixels_per_unit" | "canvas.flag"
        ),
        K::Part => matches!(field, "name" | "enabled" | "draw_order"),
        K::Deformer => matches!(field, "name" | "enabled"),
        K::Parameter => matches!(
            field,
            "name" | "minimum" | "maximum" | "default_value" | "decimal_places" | "repeat"
        ),
        K::Glue => matches!(field, "name" | "intensity"),
        K::Mesh => field == "enabled",
        K::Offscreen => matches!(field, "name" | "enabled"),
        K::Texture => field == "name",
        K::Motion => matches!(field, "name" | "looping" | "fade_in" | "fade_out"),
        K::Expression => matches!(field, "name" | "fade_in" | "fade_out"),
        _ => false,
    }
}

pub(super) fn deformer_properties(x: &kasane_core::Transform) -> Value {
    json!({"name":x.name,"runtime_id":x.runtime_id,"kind":x.kind(),"part_id":x.part(),"parent_id":x.parent(),"appearance":x.appearance,"enabled":x.enabled,"warp":x.warp(),"rotation":x.rotation()})
}
