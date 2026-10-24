//! Expression authoring properties. Identity, storage and editing policy live together.
use super::{PropertyEffect, PropertySpec, PropertyType};
use crate::ObjectKind;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Expression, "name", "name", PropertyType::Text);
pub const FADE_IN: PropertySpec = PropertySpec::new(
    ObjectKind::Expression,
    "fade_in",
    "fade_in",
    PropertyType::OptionalFloat,
)
.range(0., f32::MAX as f64, 0.1)
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const FADE_OUT: PropertySpec = PropertySpec::new(
    ObjectKind::Expression,
    "fade_out",
    "fade_out",
    PropertyType::OptionalFloat,
)
.range(0., f32::MAX as f64, 0.1)
.scalar()
.affects(PropertyEffect::Properties)
.preview();

pub const FIELDS: &[PropertySpec] = &[NAME, FADE_IN, FADE_OUT];
