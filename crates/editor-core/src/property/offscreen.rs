//! Offscreen authoring properties. Identity, storage and editing policy live together.
use super::{PropertySpec, PropertyType};
use crate::ObjectKind;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Offscreen, "name", "name", PropertyType::Text);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Offscreen,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const ENABLED: PropertySpec = PropertySpec::new(
    ObjectKind::Offscreen,
    "enabled",
    "enabled",
    PropertyType::Bool,
);

pub const FIELDS: &[PropertySpec] = &[NAME, RUNTIME_ID, ENABLED];
