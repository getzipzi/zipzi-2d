use super::*;
use editor_core::EditingContext;
use editor_core::property as schema;

fn parameter() -> PropertyForm {
    let mut form = PropertyForm::default();
    form.sync(
        ObjectKind::Parameter,
        [
            (
                schema::parameter::NAME.key,
                PropertyValue::Text("Parameter".into()),
            ),
            (
                schema::parameter::RUNTIME_ID.key,
                PropertyValue::Text("Param".into()),
            ),
            (schema::parameter::MINIMUM.key, PropertyValue::Float(0.)),
            (schema::parameter::MAXIMUM.key, PropertyValue::Float(1.)),
            (
                schema::parameter::DEFAULT_VALUE.key,
                PropertyValue::Float(0.),
            ),
            (
                schema::parameter::DECIMAL_PLACES.key,
                PropertyValue::Integer(2),
            ),
            (schema::parameter::REPEAT.key, PropertyValue::Bool(false)),
        ]
        .into(),
    );
    form
}
fn spec(id: PropertyKey) -> &'static PropertySpec {
    property_specs(ObjectKind::Parameter)
        .iter()
        .find(|s| s.key == id)
        .unwrap()
}

#[test]
fn each_field_uses_committed_dependencies_and_cannot_submit_another_draft() {
    let mut form = parameter();
    form.edit(spec(schema::parameter::MINIMUM.key), "10".into());
    form.edit(spec(schema::parameter::MAXIMUM.key), "20".into());
    form.edit(spec(schema::parameter::DEFAULT_VALUE.key), "15".into());
    assert!(
        form.patch(ObjectKind::Parameter, schema::parameter::MINIMUM.key)
            .is_err()
    );
    assert!(
        form.patch(ObjectKind::Parameter, schema::parameter::DEFAULT_VALUE.key)
            .is_err()
    );
    let patch = form
        .patch(ObjectKind::Parameter, schema::parameter::MAXIMUM.key)
        .unwrap();
    assert_eq!(patch.changes.len(), 1);
    assert_eq!(
        patch.expected[&schema::parameter::MINIMUM.key],
        PropertyValue::Float(0.)
    );
    assert_eq!(
        patch.expected[&schema::parameter::DEFAULT_VALUE.key],
        PropertyValue::Float(0.)
    );
}

#[test]
fn incomplete_input_survives_render_and_reload_restores_committed_value() {
    let mut form = parameter();
    form.edit(spec(schema::parameter::MINIMUM.key), "-".into());
    form.sync(ObjectKind::Parameter, form.current.clone());
    assert_eq!(form.fields[&schema::parameter::MINIMUM.key].raw, "-");
    assert!(
        form.patch(ObjectKind::Parameter, schema::parameter::MINIMUM.key)
            .is_err()
    );
    form.reload_field(schema::parameter::MINIMUM.key);
    assert_eq!(form.fields[&schema::parameter::MINIMUM.key].raw, "0");
    assert!(!form.dirty());
}

#[test]
fn external_changes_conflict_only_with_the_edited_field_and_restore_latest_value() {
    let mut form = parameter();
    form.edit(spec(schema::parameter::DEFAULT_VALUE.key), "0.75".into());
    let mut current = form.current.clone();
    current.insert(schema::parameter::MAXIMUM.key, PropertyValue::Float(0.5));
    form.sync(ObjectKind::Parameter, current.clone());
    assert_eq!(form.fields[&schema::parameter::MAXIMUM.key].raw, "0.5");
    assert!(
        form.patch(ObjectKind::Parameter, schema::parameter::DEFAULT_VALUE.key)
            .is_err()
    );
    current.insert(
        schema::parameter::DEFAULT_VALUE.key,
        PropertyValue::Float(0.3),
    );
    form.sync(ObjectKind::Parameter, current);
    assert!(form.fields[&schema::parameter::DEFAULT_VALUE.key].conflict);
    form.reload_field(schema::parameter::DEFAULT_VALUE.key);
    assert_eq!(
        form.fields[&schema::parameter::DEFAULT_VALUE.key].raw,
        "0.3"
    );
}

#[test]
fn optional_numbers_preserve_resource_default_and_explicit_zero() {
    let mut form = PropertyForm::default();
    form.sync(
        ObjectKind::Motion,
        [(
            schema::motion::FADE_IN.key,
            PropertyValue::OptionalFloat(None),
        )]
        .into(),
    );
    assert!(!form.numeric_visible(schema::motion::FADE_IN.key));
    let spec = property_specs(ObjectKind::Motion)
        .iter()
        .find(|s| s.key == schema::motion::FADE_IN.key)
        .unwrap();
    form.set_value(spec, PropertyValue::OptionalFloat(Some(0.)));
    assert!(form.numeric_visible(schema::motion::FADE_IN.key));
    assert_eq!(
        form.patch(ObjectKind::Motion, spec.key).unwrap().changes[&spec.key],
        PropertyValue::OptionalFloat(Some(0.))
    );
    form.edit(spec, "".into());
    assert!(form.patch(ObjectKind::Motion, spec.key).is_err());
    form.reload_field(spec.key);
    assert!(!form.numeric_visible(spec.key));
}

#[test]
fn save_settles_valid_and_invalid_input_with_the_same_history_policy_as_blur() {
    let mut editor = Editor::new().unwrap();
    let target = PropertyTarget {
        timeline: None,
        motion: None,
        scope: Default::default(),
        document: editor.active_id().unwrap(),
        context: EditingContext::Modeling,
        object: editor.document().unwrap().object_index().objects[0]
            .reference
            .clone(),
    };
    let mut forms = PropertyForms::default();
    forms.sync(
        &target,
        target.definition(&editor),
        editor
            .document()
            .unwrap()
            .property_values(&target.object)
            .unwrap(),
    );
    let width = property_specs(ObjectKind::Document)
        .iter()
        .find(|s| s.key == schema::document::PADDING_PIXELS.key)
        .unwrap();
    forms
        .entries
        .get_mut(&target)
        .unwrap()
        .edit(width, "512".into());
    assert!(
        forms
            .settle_document(&mut editor, target.document)
            .is_none()
    );
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        512.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    forms
        .entries
        .get_mut(&target)
        .unwrap()
        .edit(width, "-".into());
    assert!(
        forms
            .settle_document(&mut editor, target.document)
            .is_some()
    );
    assert_eq!(forms.entries[&target].fields[&width.key].raw, "512");
    assert!(!forms.dirty_document(target.document));
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
}

#[test]
fn formatting_is_not_history_and_invalid_numbers_never_become_zero() {
    let mut form = parameter();
    form.edit(spec(schema::parameter::MAXIMUM.key), "1.000".into());
    assert!(!form.dirty());
    assert!(
        form.patch(ObjectKind::Parameter, schema::parameter::MAXIMUM.key)
            .unwrap()
            .changes
            .is_empty()
    );
    for raw in ["NaN", "inf", "1e80", "-"] {
        assert!(spec(schema::parameter::MAXIMUM.key).parse(raw).is_err());
    }
    for raw in ["2.5", "-1", "10"] {
        assert!(
            spec(schema::parameter::DECIMAL_PLACES.key)
                .parse(raw)
                .is_err()
        );
    }
}

#[test]
fn mesh_hex_and_percent_display_preserve_precision_and_validate_input() {
    let mut form = PropertyForm::default();
    let color = PropertyValue::Color([0.1234567, 0.5678912, 0.9876543]);
    form.sync(
        ObjectKind::Mesh,
        [
            (schema::mesh::MULTIPLY.key, color.clone()),
            (schema::mesh::OPACITY.key, PropertyValue::Float(0.6)),
        ]
        .into(),
    );
    assert_eq!(form.fields[&schema::mesh::OPACITY.key].raw, "60");
    let displayed = form.fields[&schema::mesh::MULTIPLY.key].raw.clone();
    form.edit(&schema::mesh::MULTIPLY, displayed.to_lowercase());
    assert_eq!(form.fields[&schema::mesh::MULTIPLY.key].parsed, Ok(color));
    assert!(
        form.patch(ObjectKind::Mesh, schema::mesh::MULTIPLY.key)
            .unwrap()
            .changes
            .is_empty()
    );
    form.edit(&schema::mesh::OPACITY, "35.5".into());
    assert_eq!(
        form.fields[&schema::mesh::OPACITY.key].parsed,
        Ok(PropertyValue::Float(0.355))
    );
    for raw in ["#abc", "#12345678", "#GG0000", "#😀12", ""] {
        form.edit(&schema::mesh::MULTIPLY, raw.into());
        assert!(form.fields[&schema::mesh::MULTIPLY.key].parsed.is_err());
    }
    form.edit(&schema::mesh::OPACITY, "101".into());
    assert!(form.fields[&schema::mesh::OPACITY.key].parsed.is_err());
}
