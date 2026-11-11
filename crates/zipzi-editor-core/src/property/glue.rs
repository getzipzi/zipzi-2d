//! Glue authoring properties. Identity, storage and editing policy live together.
use super::{PropertyEffect, PropertySpec, PropertyType};
use crate::ObjectKind;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Glue, "name", "name", PropertyType::Text);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Glue,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const INTENSITY: PropertySpec = PropertySpec::new(
    ObjectKind::Glue,
    "intensity",
    "intensity",
    PropertyType::Float,
)
.range(0., 1., 0.01)
.scalar()
.affects(PropertyEffect::Properties)
.preview();

pub const FIELDS: &[PropertySpec] = &[NAME, RUNTIME_ID, INTENSITY];
