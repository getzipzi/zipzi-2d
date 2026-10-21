//! Part authoring properties. Identity, storage and editing policy live together.
use super::{PropertyEffect, PropertySpec, PropertyType};
use crate::ObjectKind;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Part, "name", "name", PropertyType::Text);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Part,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const ENABLED: PropertySpec =
    PropertySpec::new(ObjectKind::Part, "enabled", "enabled", PropertyType::Bool);
pub const DRAW_ORDER: PropertySpec = PropertySpec::new(
    ObjectKind::Part,
    "draw_order",
    "draw_order",
    PropertyType::Float,
)
.range(-(f32::MAX as f64), f32::MAX as f64, 1.)
.scalar()
.affects(PropertyEffect::DrawOrder)
.preview();

pub const FIELDS: &[PropertySpec] = &[NAME, RUNTIME_ID, ENABLED, DRAW_ORDER];
