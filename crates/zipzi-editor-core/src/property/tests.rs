use super::*;

#[test]
fn same_field_name_uses_object_local_paths_and_distinct_keys() -> Result<()> {
    const PART_ENABLED: PropertySpec =
        PropertySpec::new(ObjectKind::Part, "enabled", "enabled", PropertyType::Bool);
    const MESH_ENABLED: PropertySpec = PropertySpec::new(
        ObjectKind::Mesh,
        "enabled",
        "appearance.enabled",
        PropertyType::Bool,
    );
    let mut payload = serde_json::json!({
        "enabled": true,
        "appearance": { "enabled": false, "opacity": 0.5 }
    });
    assert_eq!(PART_ENABLED.key.name(), MESH_ENABLED.key.name());
    assert_ne!(PART_ENABLED.key, MESH_ENABLED.key);
    let values: PropertyValues = [
        (PART_ENABLED.key, PART_ENABLED.read(&payload)?),
        (MESH_ENABLED.key, MESH_ENABLED.read(&payload)?),
    ]
    .into();
    assert_eq!(values.len(), 2);
    assert_eq!(values[&PART_ENABLED.key], PropertyValue::Bool(true));
    assert_eq!(values[&MESH_ENABLED.key], PropertyValue::Bool(false));
    MESH_ENABLED.write(&mut payload, &PropertyValue::Bool(true))?;
    PART_ENABLED.write(&mut payload, &PropertyValue::Bool(false))?;
    assert_eq!(
        payload,
        serde_json::json!({
            "enabled": false, "appearance": { "enabled": true, "opacity": 0.5 }
        })
    );
    assert!(
        MESH_ENABLED
            .write(&mut payload, &PropertyValue::Text("bad".into()))
            .is_err()
    );
    Ok(())
}

#[test]
fn field_policies_and_dependency_groups_are_owned_by_descriptors() -> Result<()> {
    let field = PropertySpec::new(
        ObjectKind::Part,
        "custom",
        "metadata.custom",
        PropertyType::Text,
    )
    .trimmed()
    .allow_empty();
    assert_eq!(
        field.parse("  value  ")?,
        PropertyValue::Text("value".into())
    );
    assert_eq!(field.parse("  ")?, PropertyValue::Text(String::new()));
    assert!(part::NAME.parse("  ").is_err());
    assert_eq!(
        part::NAME.parse("  value  ")?,
        PropertyValue::Text("  value  ".into())
    );
    assert!(parameter::MINIMUM.same_group(&parameter::MAXIMUM));
    assert!(!parameter::NAME.same_group(&parameter::RUNTIME_ID));
    assert!(!parameter::NAME.same_group(&part::NAME));
    let foreign_range = PropertySpec::new(
        ObjectKind::Mesh,
        "minimum",
        "range.minimum",
        PropertyType::Float,
    )
    .grouped("range");
    assert!(!parameter::MINIMUM.same_group(&foreign_range));
    Ok(())
}

#[test]
fn object_schemas_have_unique_keys_and_paths_in_their_own_namespace() {
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
    ] {
        let fields = property_specs(kind);
        let mut keys = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        for spec in fields {
            assert_eq!(spec.key.object_kind(), kind);
            assert!(keys.insert(spec.key), "duplicate key {:?}", spec.key);
            assert!(paths.insert(spec.path), "duplicate path {}", spec.path);
            assert!(!spec.key.name().is_empty());
            assert!(spec.path.split('.').all(|segment| !segment.is_empty()));
        }
    }
}

#[test]
fn preview_capability_write_backend_and_effects_are_independent() {
    let structural = PropertySpec::new(ObjectKind::Mesh, "custom", "custom", PropertyType::Float);
    let preview = structural.preview();
    assert!(preview.preview);
    assert_eq!(preview.write, PropertyWrite::ReplaceObject);
    assert_eq!(preview.effect, PropertyEffect::Structure);

    let scalar = structural.scalar().affects(PropertyEffect::DrawOrder);
    assert!(!scalar.preview);
    assert_eq!(scalar.preview().write, PropertyWrite::ScalarPatch);
    assert_eq!(scalar.preview().effect, PropertyEffect::DrawOrder);
    assert_eq!(mesh::NAME.write, PropertyWrite::ReplaceObject);
    assert_eq!(mesh::NAME.effect, PropertyEffect::Properties);
    assert!(!mesh::NAME.preview);
}
