use super::support;
use editor_core::{property::mesh as fields, *};
use serde_json::json;

#[test]
fn mesh_property_scope_matches_the_evaluated_repeat_endpoint() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::rectangle_document(true)?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let parameter = ObjectRef::new(ObjectKind::Parameter, &doc.parameters()[0].id);
    doc.set_object_property(&parameter, "repeat", json!(true))?;
    let first = doc.property_scope(&object);
    doc.set_preview_parameter(&parameter.id, 1.)?;
    let snapshot = doc.snapshot()?;
    assert_eq!(snapshot.runtime_frame.parameters[0].value, 0.);
    assert_eq!(doc.property_scope(&object), first);
    doc.set_object_property(&object, "appearance.opacity", json!(0.4))?;
    assert_eq!(doc.snapshot()?.runtime_frame.drawables[0].opacity, 0.4);
    doc.undo()?;
    assert_eq!(doc.snapshot()?.runtime_frame.drawables[0].opacity, 1.);
    Ok(())
}

fn patch(
    doc: &Document,
    object: &ObjectRef,
    spec: PropertySpec,
    value: PropertyValue,
) -> PropertyPatch {
    let values = doc.property_values(object).unwrap();
    PropertyPatch {
        scope: if spec.keyframed {
            doc.property_scope(object)
        } else {
            PropertyScope::Object
        },
        expected: property_specs(object.kind)
            .iter()
            .filter(|s| s.same_group(&spec))
            .map(|s| (s.key, values[&s.key].clone()))
            .collect(),
        changes: [(spec.key, value)].into(),
    }
}

#[test]
fn mesh_properties_colors_masks_culling_and_blends_validate_roundtrip_and_undo()
-> anyhow::Result<()> {
    let (mut doc, assets) = support::sample_document_with_parts()?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let source = doc.layers()[1].id.clone();
    let original = doc.property_values(&object)?;
    for (path, value) in [
        ("appearance.multiply", json!([0.5, 0.25, 1.])),
        ("appearance.screen", json!([0., 0.1, 0.3])),
        ("masks", json!([source])),
        ("inverted_mask", json!(true)),
        ("culling", json!(true)),
        ("color_blend", json!("overlay")),
        ("alpha_blend", json!("atop")),
    ] {
        doc.set_object_property(&object, path, value)?;
    }
    let values = doc.property_values(&object)?;
    let snapshot = doc.snapshot()?;
    let draw = snapshot
        .runtime_frame
        .drawables
        .iter()
        .find(|d| d.id == object.id)
        .unwrap();
    assert_eq!(&draw.multiply_color[..3], &[0.5, 0.25, 1.]);
    assert_eq!(&draw.screen_color[..3], &[0., 0.1, 0.3]);
    assert_eq!(draw.raw_blend_mode, Some(12 | (1 << 8)));
    assert!(draw.inverted_mask && !draw.double_sided);
    assert_eq!(draw.masks, vec![source.clone()]);
    let history = doc.history_lengths();
    for (path, value) in [
        ("appearance.multiply", json!([1., -0.1, 0.])),
        ("appearance.screen", json!([0., 1.])),
        ("masks", json!([source.clone(), source])),
        ("masks", json!(["missing"])),
        ("color_blend", json!("invalid")),
        ("color_blend", json!("add_compatible")),
        ("deformer_id", json!("")),
    ] {
        assert!(
            doc.set_object_property(&object, path, value).is_err(),
            "{path}"
        );
        assert_eq!(doc.property_values(&object)?, values);
        assert_eq!(doc.history_lengths(), history);
    }
    let saved = doc.save(&assets.path().join("complete-mesh.kasane"))?;
    assert_eq!(
        Document::open(&saved.manifest)?.property_values(&object)?,
        values
    );
    for _ in 0..7 {
        doc.undo()?;
    }
    assert_eq!(doc.property_values(&object)?, original);
    for _ in 0..7 {
        doc.redo()?;
    }
    assert_eq!(doc.property_values(&object)?, values);
    Ok(())
}

#[test]
fn mesh_properties_part_assignment_uses_tree_order_locks_and_preserves_draw_groups()
-> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_parts()?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let body = support::object(ObjectKind::Part, 102);
    let before = doc.object_index();
    let original = doc.property_values(&object)?;
    doc.set_objects_locked(std::slice::from_ref(&body), true)?;
    let history = doc.history_lengths();
    assert!(
        doc.set_object_property(&object, "part_id", json!(body.id))
            .is_err()
    );
    assert_eq!(doc.history_lengths(), history);
    doc.undo()?;
    let history = doc.history_lengths();
    doc.set_object_property(&object, "part_id", json!(body.id))?;
    assert_eq!(
        doc.property_values(&object)?[&fields::PART.key],
        PropertyValue::Text(body.id.clone())
    );
    assert_eq!(doc.history_lengths(), (history.0 + 1, 0));
    assert_eq!(doc.object_index().draw_groups, before.draw_groups);
    let body_node = doc
        .mesh_tree()
        .into_iter()
        .find(|node| node.id == body.id)
        .unwrap();
    assert!(body_node.children.iter().any(|node| node.id == object.id));
    doc.undo()?;
    assert_eq!(doc.property_values(&object)?, original);
    doc.redo()?;
    assert_eq!(
        doc.property_values(&object)?[&fields::PART.key],
        PropertyValue::Text(body.id)
    );
    Ok(())
}

#[test]
fn mesh_properties_all_supported_blend_pairs_keep_both_dimensions() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document()?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    for (color_code, color) in fields::COLOR_MODES.iter().enumerate() {
        doc.set_object_property(&object, "alpha_blend", json!("over"))?;
        doc.set_object_property(&object, "color_blend", json!(color))?;
        for (alpha_code, alpha) in fields::ALPHA_MODES.iter().enumerate() {
            if matches!(color_code, 1 | 2) && alpha_code != 0 {
                continue;
            }
            doc.set_object_property(&object, "alpha_blend", json!(alpha))?;
            let values = doc.property_values(&object)?;
            assert_eq!(
                values[&fields::COLOR_BLEND.key],
                PropertyValue::Text((*color).into())
            );
            assert_eq!(
                values[&fields::ALPHA_BLEND.key],
                PropertyValue::Text((*alpha).into())
            );
            let snapshot = doc.snapshot()?;
            let draw = &snapshot.runtime_frame.drawables[0];
            let expected = (color_code >= 3 || alpha_code != 0)
                .then_some((color_code | (alpha_code << 8)) as u32);
            assert_eq!(draw.raw_blend_mode, expected);
        }
    }
    Ok(())
}

#[test]
fn mixed_mesh_descriptors_preserve_the_stored_keyform_draw_order() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::rectangle_document(true)?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    doc.set_object_property(&object, "draw_order", serde_json::json!(42.))?;
    for (spec, value) in [
        (fields::OPACITY, PropertyValue::Float(0.4)),
        (fields::MULTIPLY, PropertyValue::Color([0.2, 0.5, 1.])),
        (fields::SCREEN, PropertyValue::Color([0.3, 0., 0.])),
    ] {
        let before = doc.property_values(&object)?;
        let mut change = patch(&doc, &object, spec, value.clone());
        let name = patch(
            &doc,
            &object,
            fields::NAME,
            PropertyValue::Text("Renamed".into()),
        );
        change.expected.extend(name.expected);
        change.changes.extend(name.changes);
        doc.set_object_properties(&object, &change)?;
        let after = doc.property_values(&object)?;
        assert_eq!(after[&spec.key], value);
        assert_eq!(
            after[&fields::DRAW_ORDER.key],
            PropertyValue::OptionalFloat(Some(42.))
        );
        assert_eq!(
            after[&fields::NAME.key],
            PropertyValue::Text("Renamed".into())
        );
        doc.undo()?;
        assert_eq!(doc.property_values(&object)?, before);
    }
    Ok(())
}

#[test]
fn mesh_properties_keyform_targets_preview_cancel_history_and_parameter_navigation()
-> anyhow::Result<()> {
    let (doc, assets) = support::rectangle_document(true)?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let parameter = doc.parameters()[0].id.clone();
    let mut editor = Editor::from_document(doc);
    support::attach_viewports(&mut editor);
    let original = editor.document().unwrap().property_values(&object)?;
    let base = editor
        .document()
        .unwrap()
        .object_index()
        .get(&object)
        .unwrap()
        .properties
        .clone();
    assert!(matches!(
        editor.document().unwrap().property_scope(&object),
        PropertyScope::MeshKeyform { .. }
    ));
    for (spec, value) in [
        (fields::OPACITY, PropertyValue::Float(0.4)),
        (fields::MULTIPLY, PropertyValue::Color([0.2, 0.5, 1.])),
        (fields::SCREEN, PropertyValue::Color([0.3, 0., 0.])),
        (fields::DRAW_ORDER, PropertyValue::OptionalFloat(Some(42.))),
    ] {
        let doc = editor.document().unwrap();
        let history = doc.history_lengths();
        let groups = doc.object_index().draw_groups.clone();
        let previous_index = doc.object_index();
        let change = patch(doc, &object, spec, value.clone());
        let context = editor.editing_context_token().unwrap();
        let token = editor.begin_property_edit(context, object.clone(), &change)?;
        editor.update_property_edit(&token, &change)?;
        let candidate_index = editor.document().unwrap().object_index();
        assert!(std::ptr::eq(
            previous_index.get(&object).unwrap(),
            candidate_index.get(&object).unwrap()
        ));
        assert_eq!(editor.document().unwrap().history_lengths(), history);
        assert_eq!(
            editor.document().unwrap().property_values(&object)?,
            original
        );
        let candidate = editor.preview()?.scene;
        let draw = &candidate.runtime_frame.drawables[0];
        match spec.key {
            key if key == fields::OPACITY.key => assert_eq!(draw.opacity, 0.4),
            key if key == fields::MULTIPLY.key => {
                assert_eq!(&draw.multiply_color[..3], &[0.2, 0.5, 1.])
            }
            key if key == fields::SCREEN.key => assert_eq!(&draw.screen_color[..3], &[0.3, 0., 0.]),
            _ => assert_eq!(draw.draw_order, 42),
        }
        if spec.key == fields::DRAW_ORDER.key {
            assert!(
                editor
                    .document()
                    .unwrap()
                    .object_index()
                    .draw_groups
                    .iter()
                    .any(|group| group.max_order >= 42)
            );
            let mut reset = change.clone();
            reset.changes.clear();
            editor.update_property_edit(&token, &reset)?;
            assert_eq!(
                editor.document().unwrap().object_index().draw_groups,
                groups
            );
            editor.update_property_edit(&token, &change)?;
        }
        editor.cancel_property_edit(&token);
        assert_eq!(
            editor.preview()?.scene.runtime_frame.drawables[0].opacity,
            1.
        );
        let token = editor.begin_property_edit(context, object.clone(), &change)?;
        editor.update_property_edit(&token, &change)?;
        editor.finish_property_edit(&token)?;
        assert_eq!(
            editor.document().unwrap().property_values(&object)?[&spec.key],
            value
        );
        assert_eq!(
            editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties,
            base
        );
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(
            editor.document().unwrap().property_values(&object)?,
            original
        );
    }
    // Equal values at a second key must still reject a patch captured at the first key.
    let stale = patch(
        editor.document().unwrap(),
        &object,
        fields::OPACITY,
        PropertyValue::Float(0.25),
    );
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.clone(),
        value: 1.,
    })?;
    assert!(
        editor
            .execute(EditorCommand::SetObjectProperties {
                object: object.clone(),
                patch: stale
            })
            .is_err()
    );
    editor.execute(EditorCommand::SetObjectProperty {
        object: object.clone(),
        field: "appearance.opacity".into(),
        value: json!(0.6),
    })?;
    assert_eq!(
        editor.preview()?.scene.runtime_frame.drawables[0].opacity,
        0.6
    );
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.clone(),
        value: 0.5,
    })?;
    assert_eq!(
        editor.document().unwrap().property_scope(&object),
        PropertyScope::InterpolatedMesh
    );
    assert!(
        editor
            .execute(EditorCommand::SetObjectProperty {
                object: object.clone(),
                field: "appearance.opacity".into(),
                value: json!(0.2)
            })
            .is_err()
    );
    editor.execute(EditorCommand::SetObjectProperty {
        object: object.clone(),
        field: "name".into(),
        value: json!("Renamed between keys"),
    })?;
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter,
        value: 0.,
    })?;
    assert_eq!(
        editor.document().unwrap().property_values(&object)?[&fields::OPACITY.key],
        PropertyValue::Float(1.)
    );
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &assets.path().join("keyforms.kasane"),
        false,
    )?;
    let saved = editor.finish_save(&mut task.run())?;
    let mut reopened = Document::open(&saved.manifest)?;
    reopened.set_preview_parameter(&reopened.parameters()[0].id.clone(), 1.)?;
    assert_eq!(
        reopened.property_values(&object)?[&fields::OPACITY.key],
        PropertyValue::Float(0.6)
    );
    Ok(())
}
