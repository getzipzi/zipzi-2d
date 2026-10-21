//! Physics fields use the ordinary property form and typed local SDK patches.
use super::*;
use kasane_core::document::PhysicsAsset;

pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    choices,
    editable_in: |context| context == EditingContext::Animation,
    ..PropertyProvider::DEFAULT
};
pub(crate) use editor_core::property::physics::NAME;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NumberField {
    Fps,
    InputWeight(usize),
    OutputWeight(usize),
    OutputScale(usize),
    Vertex(usize, usize),
    Normalization(bool, usize),
    Manual(String),
}
impl NumberField {
    pub(crate) fn spec(&self, rig: &kasane_core::document::PhysicsSetting) -> Option<PropertySpec> {
        use editor_core::property::physics as schema;
        let (base, index) = match *self {
            Self::Fps => (schema::FPS, None),
            Self::InputWeight(i) => (schema::INPUT_WEIGHT, Some(i)),
            Self::OutputWeight(i) => (schema::OUTPUT_WEIGHT, Some(i)),
            Self::OutputScale(i) => (schema::OUTPUT_SCALE, Some(i)),
            Self::Vertex(i, n) => (
                [
                    schema::RADIUS,
                    schema::MOBILITY,
                    schema::DELAY,
                    schema::ACCELERATION,
                ][n],
                Some(i),
            ),
            Self::Normalization(angle, n) => (
                if angle {
                    [
                        schema::ANGLE_MINIMUM,
                        schema::ANGLE_DEFAULT,
                        schema::ANGLE_MAXIMUM,
                    ][n]
                } else {
                    [
                        schema::POSITION_MINIMUM,
                        schema::POSITION_DEFAULT,
                        schema::POSITION_MAXIMUM,
                    ][n]
                },
                None,
            ),
            Self::Manual(_) => return None,
        };
        let mut spec = base;
        if let Some(i) = index {
            spec.key = field_key(base.key.name(), i, rig);
        }
        Some(spec)
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Fps => "timeline.fps",
            Self::InputWeight(_) | Self::OutputWeight(_) => "physics.weight",
            Self::OutputScale(_) => "physics.scale",
            Self::Vertex(_, 0) => "physics.length",
            Self::Vertex(_, 1) => "physics.mobility",
            Self::Vertex(_, 2) => "physics.delay",
            Self::Vertex(_, _) => "physics.acceleration",
            Self::Normalization(_, 0) => "physics.minimum",
            Self::Normalization(_, 1) => "physics.center",
            Self::Normalization(_, _) => "physics.maximum",
            Self::Manual(_) => "physics.test_input",
        }
    }
}

pub(crate) fn field_key(
    name: &'static str,
    index: usize,
    rig: &kasane_core::document::PhysicsSetting,
) -> PropertyKey {
    let id = if name.starts_with("input_") {
        &rig.inputs[index].id
    } else if name.starts_with("output_") {
        &rig.outputs[index].id
    } else {
        &rig.vertices[index].id
    };
    PropertyKey::new(ObjectKind::PhysicsGroup, name).for_entity(id)
}
fn mapping_specs(asset: &PhysicsAsset, object: &ObjectRef) -> Vec<FieldSpec> {
    let Ok(group) = resolve(asset, object) else {
        return vec![];
    };
    let rig = asset.data.settings.iter().find(|r| r.id == group).unwrap();
    let mut specs = vec![];
    for (output, count) in [(false, rig.inputs.len()), (true, rig.outputs.len())] {
        for i in 0..count {
            for (name, label, editor) in if output {
                vec![
                    ("output_parameter", "physics.parameter", FieldEditor::Select),
                    ("output_axis", "physics.axis", FieldEditor::Select),
                    ("output_node", "physics.sample_node", FieldEditor::Select),
                    ("output_reflect", "physics.reflect", FieldEditor::Toggle),
                ]
            } else {
                vec![
                    ("input_parameter", "physics.parameter", FieldEditor::Select),
                    ("input_axis", "physics.axis", FieldEditor::Select),
                    ("input_reflect", "physics.reflect", FieldEditor::Toggle),
                ]
            } {
                let binding = editor_core::property_spec(field_key(name, i, rig)).unwrap();
                let mut spec = FieldSpec::new(binding, label, editor);
                spec.searchable = name.ends_with("parameter");
                specs.push(spec);
            }
        }
    }
    specs
}
fn choices(document: &Document, object: &ObjectRef, key: PropertyKey) -> Option<Vec<Choice>> {
    let asset = document.physics_definition()?;
    let group = resolve(asset, object).ok()?;
    let rig = asset.data.settings.iter().find(|r| r.id == group)?;
    let pairs: Vec<(String, String)> = if key.name().ends_with("parameter") {
        document
            .parameters()
            .into_iter()
            .map(|p| (p.id, p.name))
            .chain(std::iter::once((String::new(), "—".into())))
            .collect()
    } else if key.name().ends_with("axis") {
        vec![
            ("X".into(), "X".into()),
            ("Y".into(), "Y".into()),
            ("Angle".into(), t!("physics.angle").into_owned()),
        ]
    } else if key.name() == "output_node" {
        (1..rig.vertices.len())
            .map(|i| {
                (
                    rig.vertices[i].id.clone(),
                    format!("{} {i}", t!("physics.segment")),
                )
            })
            .collect()
    } else {
        return None;
    };
    Some(
        pairs
            .into_iter()
            .map(|(value, label)| Choice {
                value,
                label: label.into(),
            })
            .collect(),
    )
}
fn resolve<'a>(asset: &'a PhysicsAsset, object: &ObjectRef) -> anyhow::Result<&'a str> {
    editor_core::property::physics::group(asset, object).map(|rig| rig.id.as_str())
}

fn fields(asset: &PhysicsAsset, object: &ObjectRef) -> anyhow::Result<Vec<NumberField>> {
    let rig = editor_core::property::physics::group(asset, object)?;
    let mut fields = vec![NumberField::Fps];
    fields.extend((0..rig.inputs.len()).map(NumberField::InputWeight));
    for i in 0..rig.outputs.len() {
        fields.extend([NumberField::OutputWeight(i), NumberField::OutputScale(i)]);
    }
    for i in 1..rig.vertices.len() {
        for field in 0..4 {
            fields.push(NumberField::Vertex(i, field));
        }
    }
    for angle in [false, true] {
        for field in 0..3 {
            fields.push(NumberField::Normalization(angle, field));
        }
    }
    Ok(fields)
}
pub(super) fn definition(editor: &Editor, object: &ObjectRef) -> FormDefinition {
    let Some(asset) = editor.document().and_then(|d| d.physics_definition()) else {
        return FormDefinition::default();
    };
    let mut definition = FormDefinition::new(&[(NAME, "properties.name", FieldEditor::Text)]);
    definition.fields.extend(
        fields(asset, object)
            .unwrap_or_default()
            .into_iter()
            .map(|field| {
                let mut spec = FieldSpec::new(
                    field
                        .spec(editor_core::property::physics::group(asset, object).unwrap())
                        .unwrap(),
                    field.label(),
                    FieldEditor::Number,
                );
                spec.live_text = true;
                spec
            }),
    );
    definition.fields.extend(mapping_specs(asset, object));
    definition
}
