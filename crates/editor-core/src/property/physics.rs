//! Physics property capabilities and typed SDK patch translation.
use super::*;
use kasane_core::{
    document::{
        PhysicsAsset, PhysicsAssetProperty, PhysicsPatch, PhysicsProperty, PhysicsRigPatch,
    },
    physics::{PhysicsAxis, PhysicsRange},
};
pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::PhysicsGroup, "name", "name", PropertyType::Text).trimmed();
pub const FPS: PropertySpec =
    PropertySpec::new(ObjectKind::PhysicsGroup, "fps", "fps", PropertyType::Float)
        .range(0.0, 240.0, 1.0)
        .affects(PropertyEffect::Properties)
        .preview();
pub const INPUT_WEIGHT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "input_weight",
    "input_weight",
    PropertyType::Float,
)
.range(0.0, 100.0, 0.1)
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const OUTPUT_WEIGHT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_weight",
    "output_weight",
    PropertyType::Float,
)
.range(0.0, 100.0, 0.1)
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const OUTPUT_SCALE: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_scale",
    "output_scale",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const RADIUS: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "radius",
    "radius",
    PropertyType::Float,
)
.range(0.0, 10000.0, 0.1)
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const MOBILITY: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "mobility",
    "mobility",
    PropertyType::Float,
)
.range(0.0, 1.0, 0.01)
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const DELAY: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "delay",
    "delay",
    PropertyType::Float,
)
.range(0.0, 10000.0, 0.01)
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const ACCELERATION: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "acceleration",
    "acceleration",
    PropertyType::Float,
)
.range(0.0, 10000.0, 0.01)
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const INPUT_PARAMETER: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "input_parameter",
    "input_parameter",
    PropertyType::Text,
)
.indexed()
.allow_empty();
pub const INPUT_AXIS: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "input_axis",
    "input_axis",
    PropertyType::Text,
)
.indexed();
pub const INPUT_REFLECT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "input_reflect",
    "input_reflect",
    PropertyType::Bool,
)
.indexed();
pub const OUTPUT_PARAMETER: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_parameter",
    "output_parameter",
    PropertyType::Text,
)
.indexed()
.allow_empty();
pub const OUTPUT_AXIS: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_axis",
    "output_axis",
    PropertyType::Text,
)
.indexed();
pub const OUTPUT_REFLECT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_reflect",
    "output_reflect",
    PropertyType::Bool,
)
.indexed();
pub const OUTPUT_NODE: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_node",
    "output_node",
    PropertyType::Text,
)
.indexed();
pub const POSITION_MINIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "position_minimum",
    "position_minimum",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("position")
.affects(PropertyEffect::Properties)
.preview();
pub const POSITION_DEFAULT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "position_default",
    "position_default",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("position")
.affects(PropertyEffect::Properties)
.preview();
pub const POSITION_MAXIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "position_maximum",
    "position_maximum",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("position")
.affects(PropertyEffect::Properties)
.preview();
pub const ANGLE_MINIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "angle_minimum",
    "angle_minimum",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("angle")
.affects(PropertyEffect::Properties)
.preview();
pub const ANGLE_DEFAULT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "angle_default",
    "angle_default",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("angle")
.affects(PropertyEffect::Properties)
.preview();
pub const ANGLE_MAXIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "angle_maximum",
    "angle_maximum",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("angle")
.affects(PropertyEffect::Properties)
.preview();
pub const FIELDS: &[PropertySpec] = &[
    NAME,
    FPS,
    INPUT_WEIGHT,
    OUTPUT_WEIGHT,
    OUTPUT_SCALE,
    RADIUS,
    MOBILITY,
    DELAY,
    ACCELERATION,
    INPUT_PARAMETER,
    INPUT_AXIS,
    INPUT_REFLECT,
    OUTPUT_PARAMETER,
    OUTPUT_AXIS,
    OUTPUT_REFLECT,
    OUTPUT_NODE,
    POSITION_MINIMUM,
    POSITION_DEFAULT,
    POSITION_MAXIMUM,
    ANGLE_MINIMUM,
    ANGLE_DEFAULT,
    ANGLE_MAXIMUM,
];
pub fn group<'a>(
    asset: &'a PhysicsAsset,
    object: &ObjectRef,
) -> Result<&'a kasane_core::document::PhysicsSetting> {
    ensure!(
        object.kind == ObjectKind::PhysicsGroup,
        "Physics group changed"
    );
    let id = object.id.as_str();
    asset
        .data
        .settings
        .iter()
        .find(|rig| rig.id == id)
        .map(|rig| rig.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Physics group changed"))
}
pub fn values(model: &kasane_core::Document, object: &ObjectRef) -> Result<PropertyValues> {
    let asset = model
        .physics()
        .ok_or_else(|| anyhow::anyhow!("Physics changed"))?;
    let rig = group(asset, object)?;
    let mut values = PropertyValues::new();
    let name = asset
        .data
        .meta
        .dictionary
        .iter()
        .find(|entry| entry.id == rig.id)
        .map_or(rig.id.as_str(), |entry| entry.name.as_str());
    values.insert(NAME.key, PropertyValue::Text(name.into()));
    values.insert(
        FPS.key,
        PropertyValue::Float(asset.data.meta.fps.unwrap_or(30.)),
    );
    let parameter = |target: &kasane_core::document::PhysicsTarget| {
        PropertyValue::Text(match target {
            kasane_core::document::PhysicsTarget::Unspecified => String::new(),
            kasane_core::document::PhysicsTarget::Resolved { parameter_id } => parameter_id.clone(),
            kasane_core::document::PhysicsTarget::Unresolved { runtime_id } => {
                format!("unresolved:{runtime_id}")
            }
        })
    };
    for input in &rig.inputs {
        values.insert(
            INPUT_WEIGHT.key.for_entity(&input.id),
            PropertyValue::Float(input.weight),
        );
        values.insert(
            INPUT_AXIS.key.for_entity(&input.id),
            PropertyValue::Text(format!("{:?}", input.kind)),
        );
        values.insert(
            INPUT_REFLECT.key.for_entity(&input.id),
            PropertyValue::Bool(input.reflect),
        );
        values.insert(
            INPUT_PARAMETER.key.for_entity(&input.id),
            parameter(&input.source),
        );
    }
    for output in &rig.outputs {
        values.insert(
            OUTPUT_WEIGHT.key.for_entity(&output.id),
            PropertyValue::Float(output.weight),
        );
        values.insert(
            OUTPUT_SCALE.key.for_entity(&output.id),
            PropertyValue::Float(output.scale),
        );
        values.insert(
            OUTPUT_AXIS.key.for_entity(&output.id),
            PropertyValue::Text(format!("{:?}", output.kind)),
        );
        values.insert(
            OUTPUT_REFLECT.key.for_entity(&output.id),
            PropertyValue::Bool(output.reflect),
        );
        values.insert(
            OUTPUT_PARAMETER.key.for_entity(&output.id),
            parameter(&output.destination),
        );
        values.insert(
            OUTPUT_NODE.key.for_entity(&output.id),
            PropertyValue::Text(output.node_id.clone()),
        );
    }
    for vertex in rig.vertices.iter().skip(1) {
        for (spec, value) in [
            (RADIUS, vertex.radius),
            (MOBILITY, vertex.mobility),
            (DELAY, vertex.delay),
            (ACCELERATION, vertex.acceleration),
        ] {
            values.insert(spec.key.for_entity(&vertex.id), PropertyValue::Float(value));
        }
    }
    for (spec, value) in [
        (POSITION_MINIMUM, rig.normalization.position.minimum),
        (POSITION_DEFAULT, rig.normalization.position.default),
        (POSITION_MAXIMUM, rig.normalization.position.maximum),
        (ANGLE_MINIMUM, rig.normalization.angle.minimum),
        (ANGLE_DEFAULT, rig.normalization.angle.default),
        (ANGLE_MAXIMUM, rig.normalization.angle.maximum),
    ] {
        values.insert(spec.key, PropertyValue::Float(value));
    }
    Ok(values)
}
pub(crate) fn patch(
    model: &kasane_core::Document,
    object: &ObjectRef,
    changes: &PropertyValues,
) -> Result<PhysicsPatch> {
    let asset = model
        .physics()
        .ok_or_else(|| anyhow::anyhow!("Physics changed"))?;
    let rig = group(asset, object)?;
    let mut result = PhysicsPatch {
        asset_id: asset.id.clone(),
        asset_changes: vec![],
        rig: None,
    };
    let mut rig_patch = PhysicsRigPatch {
        group_id: rig.id.clone(),
        changes: vec![],
    };
    let mut position: Option<PhysicsRange> = None;
    let mut angle: Option<PhysicsRange> = None;
    for (key, value) in changes {
        let spec = property_spec(*key).ok_or_else(|| anyhow::anyhow!("Physics field changed"))?;
        spec.validate(value)?;
        let entity_id = key.entity_id();
        ensure!(
            key.index().is_none(),
            "Physics fields require stable entity addresses"
        );
        let id = || {
            entity_id
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Missing physics entity address"))
        };
        let target = || {
            let parameter_id = value.text();
            if parameter_id.is_empty() {
                kasane_core::document::PhysicsTarget::Unspecified
            } else {
                kasane_core::document::PhysicsTarget::Resolved { parameter_id }
            }
        };
        let number = || value.number().unwrap() as f32;
        let boolean = || {
            if let PropertyValue::Bool(value) = value {
                *value
            } else {
                unreachable!()
            }
        };
        let axis = || match value.text().as_str() {
            "X" => Ok(PhysicsAxis::X),
            "Y" => Ok(PhysicsAxis::Y),
            "Angle" => Ok(PhysicsAxis::Angle),
            _ => bail!("Invalid physics axis"),
        };
        use PhysicsProperty::*;
        let change = match key.name() {
            "name" => Name(value.text()),
            "fps" => {
                result
                    .asset_changes
                    .push(PhysicsAssetProperty::Fps(Some(number())));
                continue;
            }
            "input_weight" => InputWeight {
                id: id()?,
                value: number(),
            },
            "input_axis" => InputAxis {
                id: id()?,
                value: axis()?,
            },
            "input_reflect" => InputReflect {
                id: id()?,
                value: boolean(),
            },
            "input_parameter" => InputParameter {
                id: id()?,
                target: target(),
            },
            "output_weight" => OutputWeight {
                id: id()?,
                value: number(),
            },
            "output_scale" => OutputScale {
                id: id()?,
                value: number(),
            },
            "output_axis" => OutputAxis {
                id: id()?,
                value: axis()?,
            },
            "output_reflect" => OutputReflect {
                id: id()?,
                value: boolean(),
            },
            "output_parameter" => OutputParameter {
                id: id()?,
                target: target(),
            },
            "output_node" => OutputNode {
                id: id()?,
                node_id: value.text(),
            },
            "radius" => VertexRadius {
                id: id()?,
                value: number(),
            },
            "mobility" => VertexMobility {
                id: id()?,
                value: number(),
            },
            "delay" => VertexDelay {
                id: id()?,
                value: number(),
            },
            "acceleration" => VertexAcceleration {
                id: id()?,
                value: number(),
            },
            name if name.starts_with("position_") || name.starts_with("angle_") => {
                let range = if name.starts_with("position_") {
                    position.get_or_insert(rig.normalization.position)
                } else {
                    angle.get_or_insert(rig.normalization.angle)
                };
                if name.ends_with("minimum") {
                    range.minimum = number();
                } else if name.ends_with("default") {
                    range.default = number();
                } else {
                    range.maximum = number();
                }
                continue;
            }
            _ => bail!("Physics field changed"),
        };
        rig_patch.changes.push(change);
    }
    if let Some(range) = position {
        rig_patch
            .changes
            .push(PhysicsProperty::PositionNormalization(range));
    }
    if let Some(range) = angle {
        rig_patch
            .changes
            .push(PhysicsProperty::AngleNormalization(range));
    }
    if !rig_patch.changes.is_empty() {
        result.rig = Some(rig_patch);
    }
    Ok(result)
}
