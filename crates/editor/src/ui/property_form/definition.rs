//! Presentation order and field layouts are declared here, independently
//! of the domain's capability order. Sections do not define transaction scopes.
use super::*;
use editor_core::PropertyType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldEditor {
    Text,
    Toggle,
    Number,
    OptionalNumber,
}

#[derive(Clone, Copy)]
pub(crate) struct FieldSpec {
    pub binding: PropertySpec,
    pub label: &'static str,
    pub editor: FieldEditor,
}

#[derive(Default)]
pub(crate) struct FormDefinition {
    pub fields: Vec<FieldSpec>,
}

impl FormDefinition {
    pub fn fields(&self) -> impl Iterator<Item = &FieldSpec> {
        self.fields.iter()
    }

    pub fn field(&self, id: PropertyId) -> Option<FieldSpec> {
        self.fields().find(|field| field.binding.id == id).copied()
    }
}

pub(crate) fn definition(kind: ObjectKind) -> FormDefinition {
    use FieldEditor::*;
    use PropertyId::*;
    let field = |id, label, editor| {
        let binding = *property_specs(kind)
            .iter()
            .find(|binding| binding.id == id)
            .expect("configured field must have a domain binding");
        assert!(matches!(
            (editor, binding.value_type),
            (Text, PropertyType::Text)
                | (Toggle, PropertyType::Bool)
                | (Number, PropertyType::Integer | PropertyType::Float)
                | (OptionalNumber, PropertyType::OptionalFloat)
        ));
        FieldSpec {
            binding,
            label,
            editor,
        }
    };
    let fields = match kind {
        ObjectKind::Part => vec![
            field(Name, "properties.name", Text),
            field(RuntimeId, "properties.runtime_id", Text),
            field(Enabled, "properties.enabled", Toggle),
            field(DrawOrder, "properties.draw_order", Number),
        ],
        ObjectKind::Deformer | ObjectKind::Offscreen | ObjectKind::Mesh => vec![
            field(Name, "properties.name", Text),
            field(RuntimeId, "properties.runtime_id", Text),
            field(Enabled, "properties.enabled", Toggle),
        ],
        ObjectKind::Texture => vec![field(Name, "properties.name", Text)],
        ObjectKind::Parameter => vec![
            field(Name, "properties.name", Text),
            field(RuntimeId, "properties.runtime_id", Text),
            field(DecimalPlaces, "properties.decimal_places", Number),
            field(Repeat, "properties.repeat", Toggle),
            field(Minimum, "properties.minimum", Number),
            field(Maximum, "properties.maximum", Number),
            field(DefaultValue, "properties.default_value", Number),
        ],
        ObjectKind::Glue => vec![
            field(Name, "properties.name", Text),
            field(RuntimeId, "properties.runtime_id", Text),
            field(Intensity, "properties.intensity", Number),
        ],
        ObjectKind::Motion => vec![
            field(Name, "properties.name", Text),
            field(Looping, "properties.looping", Toggle),
            field(FadeIn, "properties.fade_in", OptionalNumber),
            field(FadeOut, "properties.fade_out", OptionalNumber),
        ],
        ObjectKind::Expression => vec![
            field(Name, "properties.name", Text),
            field(FadeIn, "properties.fade_in", OptionalNumber),
            field(FadeOut, "properties.fade_out", OptionalNumber),
        ],
        _ => vec![],
    };
    FormDefinition { fields }
}
