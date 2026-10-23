use super::*;
use editor_core::property as schema;
use editor_core::property_specs;

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
            definition.fields().map(|s| s.binding.key).collect();
        assert_eq!(fields.len(), definition.fields().count());
        if kind == Document {
            assert!(fields.is_empty());
        } else {
            assert_eq!(
                fields,
                property_specs(kind).iter().map(|spec| spec.key).collect()
            );
        }
    }
}

#[test]
fn parameter_provider_resolves_current_group_choices_and_search_policy() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let id = document.create_parameter_group(
        editor_core::ParameterGroupDraft {
            name: "Face".into(),
            runtime_id: "GroupFace".into(),
            parent_id: None,
        },
        document.parameter_model_revision(),
    )?;
    let definition = definition(ObjectKind::Parameter);
    let group = definition.field(schema::parameter::GROUP_ID.key).unwrap();
    let kind = definition.field(schema::parameter::KIND.key).unwrap();
    assert!(group.searchable);
    assert!(!kind.searchable);
    let choices = group.choices.unwrap()(&document);
    assert!(choices[0].value.is_empty());
    assert_eq!(choices[1].value, id);
    assert_eq!(choices[1].label.as_ref(), "Face");
    let mut draft = document.parameter_group_draft(&id)?;
    draft.name = "Eyes".into();
    document.update_parameter_group(&id, draft, document.parameter_model_revision())?;
    assert_eq!(group.choices.unwrap()(&document)[1].label.as_ref(), "Eyes");
    assert_eq!(
        kind.choices.unwrap()(&document)
            .iter()
            .map(|choice| choice.value.as_str())
            .collect::<Vec<_>>(),
        vec!["normal", "blend_shape"]
    );
    Ok(())
}

#[test]
fn parameter_range_provider_uses_committed_dependencies_and_preserves_valid_endpoints() {
    let definition = definition(ObjectKind::Parameter);
    let values: PropertyValues = [
        (schema::parameter::MINIMUM.key, PropertyValue::Float(-1.)),
        (schema::parameter::MAXIMUM.key, PropertyValue::Float(1.)),
        (
            schema::parameter::DEFAULT_VALUE.key,
            PropertyValue::Float(0.),
        ),
    ]
    .into();
    let bounds = |id| definition.field(id).unwrap().number_bounds(&values);
    assert_eq!(bounds(schema::parameter::MINIMUM.key).1, 0.);
    assert_eq!(bounds(schema::parameter::MAXIMUM.key).0, 0.);
    assert_eq!(bounds(schema::parameter::DEFAULT_VALUE.key), (-1., 1.));
    let validate = editor_core::property::parameter::validate_change;
    assert!(
        validate(
            &values,
            schema::parameter::MINIMUM.key,
            &PropertyValue::Float(1.)
        )
        .is_err()
    );
    assert!(
        validate(
            &values,
            schema::parameter::DEFAULT_VALUE.key,
            &PropertyValue::Float(2.)
        )
        .is_err()
    );
    assert!(
        validate(
            &values,
            schema::parameter::DEFAULT_VALUE.key,
            &PropertyValue::Float(1.)
        )
        .is_ok()
    );
    let endpoint_values = [
        (schema::parameter::MINIMUM.key, PropertyValue::Float(0.)),
        (schema::parameter::MAXIMUM.key, PropertyValue::Float(1.)),
        (
            schema::parameter::DEFAULT_VALUE.key,
            PropertyValue::Float(0.),
        ),
    ]
    .into();
    assert_eq!(
        definition
            .field(schema::parameter::MAXIMUM.key)
            .unwrap()
            .number_bounds(&endpoint_values)
            .0,
        f64::from(0_f32.next_up())
    );
}

#[test]
fn providers_own_context_editability_and_readonly_field_visibility() {
    for kind in [
        ObjectKind::Parameter,
        ObjectKind::ParameterGroup,
        ObjectKind::Part,
        ObjectKind::Mesh,
        ObjectKind::Motion,
        ObjectKind::Expression,
    ] {
        assert!((provider(kind).editable_in)(EditingContext::Modeling));
        assert_eq!(
            (provider(kind).editable_in)(EditingContext::Animation),
            matches!(kind, ObjectKind::Motion | ObjectKind::Expression)
        );
    }
    assert!(!(provider(ObjectKind::Document).show_readonly)(
        "export_settings.padding_pixels"
    ));
    assert!(!(provider(ObjectKind::Document).show_readonly)(
        "source_canvas.width"
    ));
    assert!((provider(ObjectKind::Document).show_readonly)(
        "canvas.width"
    ));
    assert!((provider(ObjectKind::Part).show_readonly)(
        "export_settings.padding_pixels"
    ));
}

#[test]
fn ui_fields_bind_object_descriptors_without_a_global_path_lookup() {
    let nested_enabled = PropertySpec::new(
        ObjectKind::Mesh,
        "enabled",
        "appearance.enabled",
        PropertyType::Bool,
    );
    let flat = FieldSpec::new(
        schema::part::ENABLED,
        "properties.enabled",
        FieldEditor::Toggle,
    );
    let nested = FieldSpec::new(nested_enabled, "properties.enabled", FieldEditor::Toggle);
    assert_eq!(flat.label, nested.label);
    assert_ne!(flat.binding.key, nested.binding.key);
    assert_eq!(flat.binding.path, "enabled");
    assert_eq!(nested.binding.path, "appearance.enabled");
}
