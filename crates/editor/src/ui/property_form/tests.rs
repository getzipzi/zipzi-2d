use super::*;

#[test]
fn definitions_cover_domain_fields_once_without_submission_overrides() {
    use ObjectKind::*;
    for kind in [
        Document,
        Part,
        Mesh,
        Deformer,
        Offscreen,
        Parameter,
        ParameterGroup,
        Glue,
        Motion,
        Expression,
        Texture,
        Physics,
        Unresolved,
    ] {
        let definition = definition(kind);
        let fields: std::collections::HashSet<_> =
            definition.fields().map(|s| s.binding.id).collect();
        assert_eq!(fields.len(), definition.fields().count());
        assert_eq!(
            fields,
            property_specs(kind).iter().map(|spec| spec.id).collect()
        );
    }
}
fn parameter() -> PropertyForm {
    let mut form = PropertyForm::default();
    form.sync(
        ObjectKind::Parameter,
        [
            (PropertyId::Name, PropertyValue::Text("Parameter".into())),
            (PropertyId::RuntimeId, PropertyValue::Text("Param".into())),
            (PropertyId::Minimum, PropertyValue::Float(0.)),
            (PropertyId::Maximum, PropertyValue::Float(1.)),
            (PropertyId::DefaultValue, PropertyValue::Float(0.)),
            (PropertyId::DecimalPlaces, PropertyValue::Integer(2)),
            (PropertyId::Repeat, PropertyValue::Bool(false)),
        ]
        .into(),
    );
    form
}
fn spec(id: PropertyId) -> &'static PropertySpec {
    property_specs(ObjectKind::Parameter)
        .iter()
        .find(|s| s.id == id)
        .unwrap()
}

#[test]
fn each_field_uses_committed_dependencies_and_cannot_submit_another_draft() {
    let mut form = parameter();
    form.edit(spec(PropertyId::Minimum), "10".into());
    form.edit(spec(PropertyId::Maximum), "20".into());
    form.edit(spec(PropertyId::DefaultValue), "15".into());
    assert!(
        form.patch(ObjectKind::Parameter, PropertyId::Minimum)
            .is_err()
    );
    assert!(
        form.patch(ObjectKind::Parameter, PropertyId::DefaultValue)
            .is_err()
    );
    let patch = form
        .patch(ObjectKind::Parameter, PropertyId::Maximum)
        .unwrap();
    assert_eq!(patch.changes.len(), 1);
    assert_eq!(
        patch.expected[&PropertyId::Minimum],
        PropertyValue::Float(0.)
    );
    assert_eq!(
        patch.expected[&PropertyId::DefaultValue],
        PropertyValue::Float(0.)
    );
}

#[test]
fn incomplete_input_survives_render_and_reload_restores_committed_value() {
    let mut form = parameter();
    form.edit(spec(PropertyId::Minimum), "-".into());
    form.sync(ObjectKind::Parameter, form.current.clone());
    assert_eq!(form.fields[&PropertyId::Minimum].raw, "-");
    assert!(
        form.patch(ObjectKind::Parameter, PropertyId::Minimum)
            .is_err()
    );
    form.reload_field(PropertyId::Minimum);
    assert_eq!(form.fields[&PropertyId::Minimum].raw, "0");
    assert!(!form.dirty());
}

#[test]
fn external_changes_conflict_only_with_the_edited_field_and_restore_latest_value() {
    let mut form = parameter();
    form.edit(spec(PropertyId::DefaultValue), "0.75".into());
    let mut current = form.current.clone();
    current.insert(PropertyId::Maximum, PropertyValue::Float(0.5));
    form.sync(ObjectKind::Parameter, current.clone());
    assert_eq!(form.fields[&PropertyId::Maximum].raw, "0.5");
    assert!(
        form.patch(ObjectKind::Parameter, PropertyId::DefaultValue)
            .is_err()
    );
    current.insert(PropertyId::DefaultValue, PropertyValue::Float(0.3));
    form.sync(ObjectKind::Parameter, current);
    assert!(form.fields[&PropertyId::DefaultValue].conflict);
    form.reload_field(PropertyId::DefaultValue);
    assert_eq!(form.fields[&PropertyId::DefaultValue].raw, "0.3");
}

#[test]
fn optional_numbers_preserve_resource_default_and_explicit_zero() {
    let mut form = PropertyForm::default();
    form.sync(
        ObjectKind::Motion,
        [(PropertyId::FadeIn, PropertyValue::OptionalFloat(None))].into(),
    );
    assert!(!form.numeric_visible(PropertyId::FadeIn));
    let spec = property_specs(ObjectKind::Motion)
        .iter()
        .find(|s| s.id == PropertyId::FadeIn)
        .unwrap();
    form.set_value(spec, PropertyValue::OptionalFloat(Some(0.)));
    assert!(form.numeric_visible(PropertyId::FadeIn));
    assert_eq!(
        form.patch(ObjectKind::Motion, spec.id).unwrap().changes[&spec.id],
        PropertyValue::OptionalFloat(Some(0.))
    );
    form.edit(spec, "".into());
    assert!(form.patch(ObjectKind::Motion, spec.id).is_err());
    form.reload_field(spec.id);
    assert!(!form.numeric_visible(spec.id));
}

#[test]
fn save_settles_valid_and_invalid_input_with_the_same_history_policy_as_blur() {
    let mut editor = Editor::new().unwrap();
    let target = PropertyTarget {
        document: editor.active_id().unwrap(),
        context: EditingContext::Modeling,
        object: editor.document().unwrap().object_index().objects[0]
            .reference
            .clone(),
    };
    let mut forms = PropertyForms::default();
    forms.sync(
        &target,
        editor
            .document()
            .unwrap()
            .property_values(&target.object)
            .unwrap(),
    );
    let width = property_specs(ObjectKind::Document)
        .iter()
        .find(|s| s.id == PropertyId::OriginX)
        .unwrap();
    forms
        .entries
        .get_mut(&target)
        .unwrap()
        .edit(width, "512".into());
    assert!(forms.prepare_save(&mut editor, target.document).is_none());
    assert_eq!(editor.document().unwrap().model_space().origin.x, 512.);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    forms
        .entries
        .get_mut(&target)
        .unwrap()
        .edit(width, "-".into());
    assert!(forms.prepare_save(&mut editor, target.document).is_some());
    assert_eq!(forms.entries[&target].fields[&width.id].raw, "512");
    assert!(!forms.dirty_document(target.document));
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
}

#[test]
fn formatting_is_not_history_and_invalid_numbers_never_become_zero() {
    let mut form = parameter();
    form.edit(spec(PropertyId::Maximum), "1.000".into());
    assert!(!form.dirty());
    assert!(
        form.patch(ObjectKind::Parameter, PropertyId::Maximum)
            .unwrap()
            .changes
            .is_empty()
    );
    for raw in ["NaN", "inf", "1e80", "-"] {
        assert!(spec(PropertyId::Maximum).parse(raw).is_err());
    }
    for raw in ["2.5", "-1", "10"] {
        assert!(spec(PropertyId::DecimalPlaces).parse(raw).is_err());
    }
}
