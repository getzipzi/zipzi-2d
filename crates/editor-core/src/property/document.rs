//! Document authoring properties. Identity, storage and editing policy live together.
use super::{PropertySpec, PropertyType};
use crate::ObjectKind;

pub const PIXELS_PER_UNIT: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "pixels_per_unit",
    "export_settings.pixels_per_unit",
    PropertyType::Float,
)
.range(f32::MIN_POSITIVE as f64, f32::MAX as f64, 1.);
pub const PADDING_FRACTION: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "padding_fraction",
    "export_settings.padding_fraction",
    PropertyType::Float,
)
.range(0., 10., 0.01);
pub const PADDING_PIXELS: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "padding_pixels",
    "export_settings.padding_pixels",
    PropertyType::Float,
)
.range(0., f32::MAX as f64, 1.)
.preview();
pub const MINIMUM_EXTENT: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "minimum_extent",
    "export_settings.minimum_extent",
    PropertyType::Float,
)
.range(0., f32::MAX as f64, 1.)
.preview();

pub const FIELDS: &[PropertySpec] = &[
    PIXELS_PER_UNIT,
    PADDING_FRACTION,
    PADDING_PIXELS,
    MINIMUM_EXTENT,
];
