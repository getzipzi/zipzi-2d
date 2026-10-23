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

pub fn number_bounds(values: &super::PropertyValues, id: super::PropertyKey) -> (f64, f64) {
    let value = |key| {
        values
            .get(&key)
            .and_then(super::PropertyValue::number)
            .unwrap_or(0.) as f32
    };
    let (min, max, default) = (
        value(MINIMUM.key),
        value(MAXIMUM.key),
        value(DEFAULT_VALUE.key),
    );
    if id == MINIMUM.key {
        (-(f32::MAX as f64), f64::from(default.min(max.next_down())))
    } else if id == MAXIMUM.key {
        (f64::from(default.max(min.next_up())), f32::MAX as f64)
    } else {
        assert_eq!(id, DEFAULT_VALUE.key);
        (f64::from(min), f64::from(max))
    }
}

/// Validate only this field against committed dependencies, never sibling drafts.
/// The native validator is also used by actual parameter publication.
pub fn validate_change(
    values: &super::PropertyValues,
    id: super::PropertyKey,
    candidate: &super::PropertyValue,
) -> anyhow::Result<()> {
    if ![MINIMUM.key, MAXIMUM.key, DEFAULT_VALUE.key].contains(&id) {
        return Ok(());
    }
    let number = |key| {
        if key == id {
            candidate.number()
        } else {
            values.get(&key).and_then(super::PropertyValue::number)
        }
        .ok_or_else(|| anyhow::anyhow!("Missing parameter range dependency"))
    };
    Ok(kasane_core::document::validate_parameter_range(
        number(MINIMUM.key)? as f32,
        number(MAXIMUM.key)? as f32,
        number(DEFAULT_VALUE.key)? as f32,
    )?)
}
