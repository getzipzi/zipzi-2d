//! Document authoring properties. Identity, storage and editing policy live together.
use super::{
    PropertyEffect, PropertyKey, PropertyPatch, PropertySpec, PropertyType, PropertyValue,
};
use crate::ObjectKind;
use anyhow::{Result, ensure};
use zipzi_core::{
    document::{
        Model3Settings, ModelParameterGroup, ModelTargetRef, ReferenceEntry,
        ReferenceEntryAllocation,
    },
    state::DocumentState,
};

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
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const MINIMUM_EXTENT: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "minimum_extent",
    "export_settings.minimum_extent",
    PropertyType::Float,
)
.range(0., f32::MAX as f64, 1.)
.scalar()
.affects(PropertyEffect::Properties)
.preview();

pub const EYE_BLINK_PARAMETERS: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "eye_blink_parameters",
    "eye_blink_parameters",
    PropertyType::References,
);
pub const LIP_SYNC_PARAMETERS: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "lip_sync_parameters",
    "lip_sync_parameters",
    PropertyType::References,
);
pub const BREATH_PARAMETERS: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "breath_parameters",
    "breath_parameters",
    PropertyType::References,
);

const EFFECT_FIELDS: [(PropertySpec, &str); 2] = [
    (EYE_BLINK_PARAMETERS, "EyeBlink"),
    (LIP_SYNC_PARAMETERS, "LipSync"),
];

pub fn is_motion_effect_field(key: PropertyKey) -> bool {
    key == BREATH_PARAMETERS.key || is_model3_effect_field(key)
}

pub(crate) fn is_model3_effect_field(key: PropertyKey) -> bool {
    EFFECT_FIELDS.iter().any(|(spec, _)| spec.key == key)
}

fn member_key(entry: &ReferenceEntry<ModelTargetRef>) -> String {
    match &entry.target {
        ModelTargetRef::Resolved { object_id } => object_id.clone(),
        ModelTargetRef::Unresolved { runtime_id } => format!("runtime:{runtime_id}"),
        ModelTargetRef::Unspecified => format!("unconfigured:{}", entry.id),
    }
}

pub(super) fn payload(model: &DocumentState) -> serde_json::Value {
    let mut payload = serde_json::json!({ "export_settings": model.setup().export });
    for (spec, name) in EFFECT_FIELDS {
        let ids: Vec<_> = model
            .model3_settings()
            .parameter_group_named(name)
            .into_iter()
            .flat_map(|group| group.parameters.iter().map(member_key))
            .collect();
        payload[spec.path] = serde_json::json!(ids);
    }
    payload[BREATH_PARAMETERS.path] = serde_json::json!(model.breath_preview_parameters());
    payload
}

pub(crate) fn patch_motion_effects(
    model: &DocumentState,
    patch: &PropertyPatch,
) -> Result<Model3Settings> {
    let mut settings = model.model3_settings().clone();
    for (spec, name) in EFFECT_FIELDS {
        let Some(PropertyValue::References(ids)) = patch.changes.get(&spec.key) else {
            continue;
        };
        let previous = settings.parameter_group_named(name);
        if previous.is_none() && ids.is_empty() {
            continue;
        }
        let existing: std::collections::HashMap<_, _> = previous
            .into_iter()
            .flat_map(|group| {
                group
                    .parameters
                    .iter()
                    .map(|entry| (member_key(entry), entry))
            })
            .collect();
        let mut parameters = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(entry) = existing.get(id) {
                parameters.push((*entry).clone());
            } else {
                ensure!(model.parameter(id).is_some(), "Unknown parameter {id}");
                parameters.push(ReferenceEntry::new(ModelTargetRef::Resolved {
                    object_id: id.clone(),
                }));
            }
        }
        let groups = settings.groups.get_or_insert_with(Vec::new);
        if let Some(group) = groups.iter_mut().find(|group| group.name == name) {
            group.parameters = parameters;
        } else {
            groups.push(ModelParameterGroup {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.into(),
                parameters,
                extensions: Default::default(),
            });
        }
    }
    Ok(settings)
}

pub const FIELDS: &[PropertySpec] = &[
    PIXELS_PER_UNIT,
    PADDING_FRACTION,
    PADDING_PIXELS,
    MINIMUM_EXTENT,
    EYE_BLINK_PARAMETERS,
    LIP_SYNC_PARAMETERS,
    BREATH_PARAMETERS,
];
