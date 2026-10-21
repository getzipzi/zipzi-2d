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
        .preview();
pub const INPUT_WEIGHT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "input_weight",
    "input_weight",
    PropertyType::Float,
)
.range(0.0, 100.0, 0.1)
.indexed()
.preview();
pub const OUTPUT_WEIGHT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_weight",
    "output_weight",
    PropertyType::Float,
)
.range(0.0, 100.0, 0.1)
.indexed()
.preview();
pub const OUTPUT_SCALE: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "output_scale",
    "output_scale",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.indexed()
.preview();
pub const RADIUS: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "radius",
    "radius",
    PropertyType::Float,
)
.range(0.0, 10000.0, 0.1)
.indexed()
.preview();
pub const MOBILITY: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "mobility",
    "mobility",
    PropertyType::Float,
)
.range(0.0, 1.0, 0.01)
.indexed()
.preview();
pub const DELAY: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "delay",
    "delay",
    PropertyType::Float,
)
.range(0.0, 10000.0, 0.01)
.indexed()
.preview();
pub const ACCELERATION: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "acceleration",
    "acceleration",
    PropertyType::Float,
)
.range(0.0, 10000.0, 0.01)
.indexed()
.preview();
pub const INPUT_PARAMETER: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "input_parameter",
    "input_parameter",
    PropertyType::Text,
)
.indexed();
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
.indexed();
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
.preview();
pub const POSITION_DEFAULT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "position_default",
    "position_default",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("position")
.preview();
pub const POSITION_MAXIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "position_maximum",
    "position_maximum",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("position")
.preview();
pub const ANGLE_MINIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "angle_minimum",
    "angle_minimum",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("angle")
.preview();
pub const ANGLE_DEFAULT: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "angle_default",
    "angle_default",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("angle")
.preview();
pub const ANGLE_MAXIMUM: PropertySpec = PropertySpec::new(
    ObjectKind::PhysicsGroup,
    "angle_maximum",
    "angle_maximum",
    PropertyType::Float,
)
.range(-10000.0, 10000.0, 0.1)
.grouped("angle")
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
) -> Result<&'a kasane_core::physics::PhysicsSetting> {
    let prefix = format!("{}/", asset.id);
    let id = object
        .id
        .strip_prefix(&prefix)
        .filter(|_| object.kind == ObjectKind::PhysicsGroup)
        .ok_or_else(|| anyhow::anyhow!("Physics group changed"))?;
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
    let parameter = |id: &str| {
        PropertyValue::Text(
            asset
                .parameter_bindings
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("unresolved:{id}")),
        )
    };
    for (i, input) in rig.inputs.iter().enumerate() {
        values.insert(INPUT_WEIGHT.key.at(i), PropertyValue::Float(input.weight));
        values.insert(
            INPUT_AXIS.key.at(i),
            PropertyValue::Text(format!("{:?}", input.kind)),
        );
        values.insert(INPUT_REFLECT.key.at(i), PropertyValue::Bool(input.reflect));
        values.insert(INPUT_PARAMETER.key.at(i), parameter(&input.source.id));
    }
    for (i, output) in rig.outputs.iter().enumerate() {
        values.insert(OUTPUT_WEIGHT.key.at(i), PropertyValue::Float(output.weight));
        values.insert(OUTPUT_SCALE.key.at(i), PropertyValue::Float(output.scale));
        values.insert(
            OUTPUT_AXIS.key.at(i),
            PropertyValue::Text(format!("{:?}", output.kind)),
        );
        values.insert(
            OUTPUT_REFLECT.key.at(i),
            PropertyValue::Bool(output.reflect),
        );
        values.insert(
            OUTPUT_PARAMETER.key.at(i),
            parameter(&output.destination.id),
        );
        values.insert(
            OUTPUT_NODE.key.at(i),
            PropertyValue::Text(output.vertex_index.to_string()),
        );
    }
    for (i, vertex) in rig.vertices.iter().enumerate().skip(1) {
        for (spec, value) in [
            (RADIUS, vertex.radius),
            (MOBILITY, vertex.mobility),
            (DELAY, vertex.delay),
            (ACCELERATION, vertex.acceleration),
        ] {
            values.insert(spec.key.at(i), PropertyValue::Float(value));
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
        let index = key.index().unwrap_or(0);
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
                index,
                value: number(),
            },
            "input_axis" => InputAxis {
                index,
                value: axis()?,
            },
            "input_reflect" => InputReflect {
                index,
                value: boolean(),
            },
            "input_parameter" => InputParameter {
                index,
                parameter_id: value.text(),
            },
            "output_weight" => OutputWeight {
                index,
                value: number(),
            },
            "output_scale" => OutputScale {
                index,
                value: number(),
            },
            "output_axis" => OutputAxis {
                index,
                value: axis()?,
            },
            "output_reflect" => OutputReflect {
                index,
                value: boolean(),
            },
            "output_parameter" => OutputParameter {
                index,
                parameter_id: value.text(),
            },
            "output_node" => OutputVertex {
                index,
                value: value.text().parse()?,
            },
            "radius" => VertexRadius {
                index,
                value: number(),
            },
            "mobility" => VertexMobility {
                index,
                value: number(),
            },
            "delay" => VertexDelay {
                index,
                value: number(),
            },
            "acceleration" => VertexAcceleration {
                index,
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
