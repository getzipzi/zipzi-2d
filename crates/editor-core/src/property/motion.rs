//! Motion authoring properties. Identity, storage and editing policy live together.
use super::{PropertyEffect, PropertySpec, PropertyType};
use crate::ObjectKind;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Motion, "name", "name", PropertyType::Text);
pub const LOOPING: PropertySpec =
    PropertySpec::new(ObjectKind::Motion, "looping", "looping", PropertyType::Bool);
pub const FADE_IN: PropertySpec = PropertySpec::new(
    ObjectKind::Motion,
    "fade_in",
    "fade_in",
    PropertyType::OptionalFloat,
)
.range(0., f32::MAX as f64, 0.1)
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const FADE_OUT: PropertySpec = PropertySpec::new(
    ObjectKind::Motion,
    "fade_out",
    "fade_out",
    PropertyType::OptionalFloat,
)
.range(0., f32::MAX as f64, 0.1)
.scalar()
.affects(PropertyEffect::Properties)
.preview();

pub const DURATION: PropertySpec = PropertySpec::new(
    ObjectKind::Motion,
    "duration",
    "duration",
    PropertyType::Float,
)
.range(0.001, 1e6, 0.01)
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const FPS: PropertySpec =
    PropertySpec::new(ObjectKind::Motion, "fps", "fps", PropertyType::Float)
        .range(0.001, 1e5, 1.)
        .scalar()
        .affects(PropertyEffect::Properties)
        .preview();

pub const FIELDS: &[PropertySpec] = &[NAME, DURATION, FPS, LOOPING, FADE_IN, FADE_OUT];
