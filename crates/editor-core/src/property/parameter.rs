//! Parameter authoring properties. Identity, storage and editing policy live together.
use super::{PropertyEffect, PropertySpec, PropertyType};
use crate::ObjectKind;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Parameter, "name", "name", PropertyType::Text);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Parameter,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const MINIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::Parameter,
    "minimum",
    "minimum",
    PropertyType::Float,
)
.grouped("range")
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const MAXIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::Parameter,
    "maximum",
    "maximum",
    PropertyType::Float,
)
.grouped("range")
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const DEFAULT_VALUE: PropertySpec = PropertySpec::new(
    ObjectKind::Parameter,
    "default_value",
    "default_value",
    PropertyType::Float,
)
.grouped("range")
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const DECIMAL_PLACES: PropertySpec = PropertySpec::new(
    ObjectKind::Parameter,
    "decimal_places",
    "decimal_places",
    PropertyType::Integer,
)
.range(0., 9., 1.)
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const REPEAT: PropertySpec = PropertySpec::new(
    ObjectKind::Parameter,
    "repeat",
    "repeat",
    PropertyType::Bool,
);
pub const KIND: PropertySpec =
    PropertySpec::new(ObjectKind::Parameter, "kind", "kind", PropertyType::Text);
pub const GROUP_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Parameter,
    "group_id",
    "group_id",
    PropertyType::Text,
)
.allow_empty();

pub const FIELDS: &[PropertySpec] = &[
    NAME,
    RUNTIME_ID,
    MINIMUM,
    MAXIMUM,
    DEFAULT_VALUE,
    DECIMAL_PLACES,
    REPEAT,
    KIND,
    GROUP_ID,
];
