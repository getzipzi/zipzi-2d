//! Texture authoring properties. Identity, storage and editing policy live together.
use super::{PropertySpec, PropertyType};
use crate::ObjectKind;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Texture, "name", "name", PropertyType::Text);

pub const FIELDS: &[PropertySpec] = &[NAME];
