//! Object-specific inspector definitions and providers. The panel and form engine
//! consume this interface without knowing parameter, scene or animation rules.
mod animation;
mod details;
mod document;
pub(crate) mod form;
mod mesh;
mod parameter;
mod scene;

use crate::t;
use editor_core::{
    Document, DocumentId, EditingContext, Editor, ObjectKind, ObjectRef, PropertyKey, PropertySpec,
    PropertyType, PropertyValue, PropertyValues,
};
use gpui_kit::SharedString;

pub(crate) use animation::expression_info;
pub(crate) use details::{DetailKey, DetailRow, Details};
pub(crate) use scene::part_name;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PropertyTarget {
    pub document: DocumentId,
    pub context: EditingContext,
    pub object: ObjectRef,
    pub scope: editor_core::PropertyScope,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Choice {
    pub label: SharedString,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldEditor {
    Text,
    Select,
    Toggle,
    Number,
    OptionalNumber,
    Color,
    References,
}

#[derive(Clone, Copy)]
pub(crate) struct FieldSpec {
    pub binding: PropertySpec,
    pub label: &'static str,
    pub editor: FieldEditor,
    pub searchable: bool,
    pub choices: Option<fn(&Document) -> Vec<Choice>>,
    pub scale: f64,
    bounds: Option<fn(&PropertyValues, PropertyKey) -> (f64, f64)>,
}
impl FieldSpec {
    fn new(binding: PropertySpec, label: &'static str, editor: FieldEditor) -> Self {
        assert!(matches!(
            (editor, binding.value_type),
            (FieldEditor::Text | FieldEditor::Select, PropertyType::Text)
                | (FieldEditor::Toggle, PropertyType::Bool)
                | (
                    FieldEditor::Number,
                    PropertyType::Integer | PropertyType::Float
                )
                | (FieldEditor::OptionalNumber, PropertyType::OptionalFloat)
                | (FieldEditor::Color, PropertyType::Color)
                | (FieldEditor::References, PropertyType::References)
        ));
        Self {
            binding,
            label,
            editor,
            searchable: false,
            choices: None,
            bounds: None,
            scale: 1.,
        }
    }

    fn choices(mut self, searchable: bool, choices: fn(&Document) -> Vec<Choice>) -> Self {
        assert!(matches!(
            self.editor,
            FieldEditor::Select | FieldEditor::References
        ));
        self.searchable = searchable;
        self.choices = Some(choices);
        self
    }

    fn percent(mut self) -> Self {
        self.scale = 100.;
        self
    }

    fn bounds(mut self, bounds: fn(&PropertyValues, PropertyKey) -> (f64, f64)) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn number_bounds(&self, values: &PropertyValues) -> (f64, f64) {
        self.bounds
            .map_or((self.binding.min, self.binding.max), |bounds| {
                bounds(values, self.binding.key)
            })
    }
}

#[derive(Default)]
pub(crate) struct FormDefinition {
    pub fields: Vec<FieldSpec>,
}
impl FormDefinition {
    fn new(fields: &[(PropertySpec, &'static str, FieldEditor)]) -> Self {
        Self {
            fields: fields
                .iter()
                .map(|&(binding, label, editor)| FieldSpec::new(binding, label, editor))
                .collect(),
        }
    }
    pub fn fields(&self) -> impl Iterator<Item = &FieldSpec> {
        self.fields.iter()
    }
    pub fn field(&self, id: PropertyKey) -> Option<FieldSpec> {
        self.fields().find(|spec| spec.binding.key == id).copied()
    }
}

pub(crate) struct PropertyProvider {
    fields: fn(ObjectKind) -> FormDefinition,
    pub type_name: fn(&Document, &ObjectRef) -> SharedString,
    pub editable_in: fn(EditingContext) -> bool,
    pub show_readonly: fn(&str) -> bool,
    pub validate_change: fn(&PropertyValues, PropertyKey, &PropertyValue) -> anyhow::Result<()>,
}
impl PropertyProvider {
    const DEFAULT: Self = Self {
        fields: |_| FormDefinition::default(),
        type_name,
        editable_in: |context| context == EditingContext::Modeling,
        show_readonly: |_| true,
        validate_change: |_, _, _| Ok(()),
    };
}

fn type_name(_: &Document, object: &ObjectRef) -> SharedString {
    let key = format!("properties.type_{:?}", object.kind);
    t!(&key).into()
}

pub(crate) fn provider(kind: ObjectKind) -> &'static PropertyProvider {
    match kind {
        ObjectKind::Parameter => &parameter::PARAMETER,
        ObjectKind::ParameterGroup => &parameter::GROUP,
        ObjectKind::Motion | ObjectKind::Expression => &animation::PROVIDER,
        ObjectKind::Document => &document::PROVIDER,
        _ => &scene::PROVIDER,
    }
}

pub(crate) fn definition(kind: ObjectKind) -> FormDefinition {
    (provider(kind).fields)(kind)
}

pub(crate) fn target(editor: &Editor) -> Option<PropertyTarget> {
    let inspection = editor.active_inspection()?;
    let object = inspection.object().or_else(|| {
        if inspection.context != EditingContext::Modeling {
            return None;
        }
        editor
            .document()?
            .object_index()
            .objects
            .first()
            .map(|info| info.reference.clone())
    })?;
    Some(PropertyTarget {
        document: inspection.document,
        context: inspection.context,
        scope: editor.document()?.property_scope(&object),
        object,
    })
}

pub(crate) fn empty_message() -> SharedString {
    t!("workspace.select_motion").into()
}

fn details_source(editor: &Editor) -> Option<details::DetailSource> {
    match editor.editing_context()? {
        EditingContext::Animation => animation::details_source(editor),
        EditingContext::Modeling => None,
    }
}

#[cfg(test)]
mod tests;
