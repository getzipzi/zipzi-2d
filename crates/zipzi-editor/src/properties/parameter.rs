//! Parameter fields, choices, dependent ranges and candidate validation live together.
use super::*;
use zipzi_editor_core::property::{parameter as domain, parameter_group};

pub(super) const PARAMETER: PropertyProvider = PropertyProvider {
    fields: parameter_fields,
    ..PropertyProvider::DEFAULT
};
pub(super) const GROUP: PropertyProvider = PropertyProvider {
    fields: |_| {
        FormDefinition::new(&[
            (parameter_group::NAME, "properties.name", FieldEditor::Text),
            (
                parameter_group::RUNTIME_ID,
                "parameter.group_id",
                FieldEditor::Text,
            ),
        ])
    },
    ..PropertyProvider::DEFAULT
};

fn parameter_fields(_: ObjectKind) -> FormDefinition {
    use FieldEditor::*;
    use zipzi_editor_core::property::parameter::*;
    let field = |id, label, editor| FieldSpec::new(id, label, editor);
    FormDefinition {
        fields: vec![
            field(NAME, "properties.name", Text),
            field(RUNTIME_ID, "properties.runtime_id", Text),
            field(DECIMAL_PLACES, "properties.decimal_places", Number),
            field(REPEAT, "properties.repeat", Toggle),
            field(MINIMUM, "properties.minimum", Number).bounds(range),
            field(MAXIMUM, "properties.maximum", Number).bounds(range),
            field(DEFAULT_VALUE, "properties.default_value", Number).bounds(range),
            field(KIND, "parameter.kind", Select).choices(false, kinds),
            field(GROUP_ID, "parameter.group", Select).choices(true, groups),
        ],
        ..Default::default()
    }
}

fn kinds(_: &Document) -> Vec<Choice> {
    vec![
        Choice {
            label: t!("parameter.normal").into(),
            value: "normal".into(),
        },
        Choice {
            label: "BlendShape".into(),
            value: "blend_shape".into(),
        },
    ]
}

fn groups(document: &Document) -> Vec<Choice> {
    let mut choices = vec![Choice {
        label: t!("parameter.ungrouped").into(),
        value: String::new(),
    }];
    choices.extend(
        document
            .parameter_group_options()
            .into_iter()
            .map(|group| Choice {
                label: group.path.into(),
                value: group.id,
            }),
    );
    choices
}

fn range(values: &PropertyValues, id: PropertyKey) -> (f64, f64) {
    domain::number_bounds(values, id)
}
