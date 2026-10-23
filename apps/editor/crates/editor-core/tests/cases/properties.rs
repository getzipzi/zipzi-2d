use super::support;
use editor_core::property as schema;
use editor_core::*;

fn patch(
    doc: &Document,
    target: &ObjectRef,
    changes: &[(PropertyKey, PropertyValue)],
) -> PropertyPatch {
    let current = doc.property_values(target).unwrap();
    let groups: Vec<_> = property_specs(target.kind)
        .iter()
        .filter(|spec| changes.iter().any(|(id, _)| *id == spec.key))
        .collect();
    PropertyPatch {
        scope: doc.property_scope(target),
        expected: property_specs(target.kind)
            .iter()
            .filter(|s| groups.iter().any(|group| s.same_group(group)))
            .map(|s| (s.key, current[&s.key].clone()))
            .collect(),
        changes: changes.iter().cloned().collect(),
    }
}

#[test]
fn mesh_properties_validate_preserve_payload_and_roundtrip() -> anyhow::Result<()> {
    let (mut doc, assets) = support::sample_document()?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let original = doc.property_values(&object)?;
    let mut payload = doc.object_index().get(&object).unwrap().properties.clone();
    assert_eq!(
        original[&schema::mesh::DRAW_ORDER.key],
        PropertyValue::OptionalFloat(None)
    );
    assert_eq!(
        original[&schema::mesh::OPACITY.key],
        PropertyValue::Float(1.)
    );

    let changes = patch(
        &doc,
        &object,
        &[
            (
                schema::mesh::DRAW_ORDER.key,
                PropertyValue::OptionalFloat(Some(-12.5)),
            ),
            (schema::mesh::OPACITY.key, PropertyValue::Float(0.35)),
        ],
    );
    doc.set_object_properties(&object, &changes)?;
    payload["draw_order"] = serde_json::json!(-12.5);
    payload["appearance"]["opacity"] = serde_json::json!(0.35f32);
    assert_eq!(doc.object_index().get(&object).unwrap().properties, payload);
    let updated = doc.property_values(&object)?;
    for invalid in [-0.1, 1.1, f32::INFINITY, f32::NAN] {
        let rejected = patch(
            &doc,
            &object,
            &[
                (
                    schema::mesh::DRAW_ORDER.key,
                    PropertyValue::OptionalFloat(Some(100.)),
                ),
                (schema::mesh::OPACITY.key, PropertyValue::Float(invalid)),
            ],
        );
        assert!(doc.set_object_properties(&object, &rejected).is_err());
        assert_eq!(doc.property_values(&object)?, updated);
        assert_eq!(doc.history_lengths(), (1, 0));
    }
    doc.undo()?;
    assert_eq!(doc.property_values(&object)?, original);
    doc.redo()?;
    assert_eq!(doc.property_values(&object)?, updated);
    let saved = doc.save(&assets.path().join("mesh-properties.kasane"))?;
    assert_eq!(
        Document::open(&saved.manifest)?.property_values(&object)?,
        updated
    );

    doc.set_object_property(&object, "draw_order", serde_json::Value::Null)?;
    assert_eq!(
        doc.property_values(&object)?[&schema::mesh::DRAW_ORDER.key],
        PropertyValue::OptionalFloat(None)
    );
    assert_eq!(
        doc.property_values(&object)?[&schema::mesh::OPACITY.key],
        PropertyValue::Float(0.35)
    );
    Ok(())
}

#[test]
fn mesh_properties_preview_updates_evaluation_and_cancels_or_commits_once() -> anyhow::Result<()> {
    let (doc, _assets) = support::sample_document()?;
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let mut editor = Editor::from_document(doc);
    support::attach_viewports(&mut editor);
    for (id, value) in [
        (schema::mesh::OPACITY.key, PropertyValue::Float(0.4)),
        (
            schema::mesh::DRAW_ORDER.key,
            PropertyValue::OptionalFloat(Some(42.)),
        ),
    ] {
        let before = editor.document().unwrap().property_values(&object)?;
        let history = editor.document().unwrap().history_lengths();
        let changes = patch(editor.document().unwrap(), &object, &[(id, value.clone())]);
        let context = editor.editing_context_token().unwrap();
        let token = editor.begin_property_edit(context, object.clone(), &changes)?;
        editor.update_property_edit(&token, &changes)?;
        let scene = editor.preview()?.scene;
        let mesh = scene
            .runtime_frame
            .drawables
            .iter()
            .find(|d| d.id == object.id)
            .unwrap();
        match id {
            key if key == schema::mesh::OPACITY.key => assert_eq!(mesh.opacity, 0.4),
            key if key == schema::mesh::DRAW_ORDER.key => assert_eq!(mesh.draw_order, 42),
            _ => unreachable!(),
        }
        assert_eq!(editor.document().unwrap().property_values(&object)?, before);
        assert_eq!(editor.document().unwrap().history_lengths(), history);
        editor.cancel_property_edit(&token);
        assert_eq!(editor.document().unwrap().property_values(&object)?, before);
        let scene = editor.preview()?.scene;
        let mesh = scene
            .runtime_frame
            .drawables
            .iter()
            .find(|d| d.id == object.id)
            .unwrap();
        match id {
            key if key == schema::mesh::OPACITY.key => assert_eq!(mesh.opacity, 1.),
            key if key == schema::mesh::DRAW_ORDER.key => assert_eq!(mesh.draw_order, 0),
            _ => unreachable!(),
        }
        let token = editor.begin_property_edit(context, object.clone(), &changes)?;
        editor.update_property_edit(&token, &changes)?;
        editor.update_property_edit(&token, &changes)?;
        editor.finish_property_edit(&token)?;
        assert_eq!(
            editor.document().unwrap().property_values(&object)?[&id],
            value
        );
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(editor.document().unwrap().property_values(&object)?, before);
    }
    Ok(())
}

#[test]
fn properties_parameter_group_is_atomic_and_uses_domain_dependencies() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_parameters(1)?;
    let object = ObjectRef::new(ObjectKind::Parameter, &doc.parameters()[0].id);
    let old = doc.property_values(&object)?;
    let changes = patch(
        &doc,
        &object,
        &[
            (schema::parameter::MINIMUM.key, PropertyValue::Float(10.)),
            (schema::parameter::MAXIMUM.key, PropertyValue::Float(20.)),
            (
                schema::parameter::DEFAULT_VALUE.key,
                PropertyValue::Float(15.),
            ),
        ],
    );
    doc.set_object_properties(&object, &changes)?;
    assert_eq!(doc.history_lengths(), (1, 0));
    assert_eq!(doc.parameter_draft(&object.id)?.minimum, 10.);
    doc.undo()?;
    assert_eq!(doc.property_values(&object)?, old);
    doc.redo()?;
    assert_eq!(doc.parameter_draft(&object.id)?.default_value, 15.);
    let history = doc.history_lengths();
    let invalid = patch(
        &doc,
        &object,
        &[(
            schema::parameter::DEFAULT_VALUE.key,
            PropertyValue::Float(25.),
        )],
    );
    assert!(doc.set_object_properties(&object, &invalid).is_err());
    assert_eq!(doc.history_lengths(), history);
    assert_eq!(doc.parameter_draft(&object.id)?.default_value, 15.);

    let (mut bound, _assets) = support::sample_document_with_binding(true)?;
    let parameter = ObjectRef::new(ObjectKind::Parameter, &bound.parameters()[0].id);
    let before = bound.property_values(&parameter)?;
    let shrink = patch(
        &bound,
        &parameter,
        &[(schema::parameter::MAXIMUM.key, PropertyValue::Float(0.5))],
    );
    assert!(bound.set_object_properties(&parameter, &shrink).is_err());
    assert_eq!(bound.property_values(&parameter)?, before);
    assert_eq!(bound.history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn properties_reject_stale_groups_but_merge_unrelated_changes() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_parts()?;
    let object = support::object(ObjectKind::Part, 101);
    let rename = patch(
        &doc,
        &object,
        &[(
            schema::part::NAME.key,
            PropertyValue::Text("Renamed".into()),
        )],
    );
    doc.set_object_property(&object, "enabled", serde_json::json!(false))?;
    doc.set_object_properties(&object, &rename)?;
    let values = doc.property_values(&object)?;
    assert_eq!(
        values[&schema::part::NAME.key],
        PropertyValue::Text("Renamed".into())
    );
    assert_eq!(
        values[&schema::part::ENABLED.key],
        PropertyValue::Bool(false)
    );
    let history = doc.history_lengths();
    assert!(doc.set_object_properties(&object, &rename).is_err());
    assert_eq!(doc.history_lengths(), history);
    let unchanged = patch(
        &doc,
        &object,
        &[(
            schema::part::NAME.key,
            values[&schema::part::NAME.key].clone(),
        )],
    );
    doc.set_object_properties(&object, &unchanged)?;
    assert_eq!(doc.history_lengths(), history);
    Ok(())
}

#[test]
fn properties_lock_type_and_dependency_baselines_guard_writes() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_parameters(1)?;
    let object = ObjectRef::new(ObjectKind::Parameter, &doc.parameters()[0].id);
    let mut incomplete = patch(
        &doc,
        &object,
        &[(schema::parameter::MAXIMUM.key, PropertyValue::Float(2.))],
    );
    incomplete.expected.remove(&schema::parameter::MINIMUM.key);
    assert!(doc.set_object_properties(&object, &incomplete).is_err());
    let wrong_type = patch(
        &doc,
        &object,
        &[(schema::parameter::NAME.key, PropertyValue::Bool(true))],
    );
    assert!(doc.set_object_properties(&object, &wrong_type).is_err());
    let nonfinite = patch(
        &doc,
        &object,
        &[(
            schema::parameter::MAXIMUM.key,
            PropertyValue::Float(f32::INFINITY),
        )],
    );
    assert!(doc.set_object_properties(&object, &nonfinite).is_err());
    let (mut locked, _assets) = support::sample_document_with_parts()?;
    let part = support::object(ObjectKind::Part, 101);
    let rename = patch(
        &locked,
        &part,
        &[(schema::part::NAME.key, PropertyValue::Text("Locked".into()))],
    );
    locked.set_objects_locked(std::slice::from_ref(&part), true)?;
    assert!(locked.set_object_properties(&part, &rename).is_err());
    assert_eq!(locked.history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn properties_preview_isolated_cancelable_and_commits_one_history_step() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let object = editor.document().unwrap().object_index().objects[0]
        .reference
        .clone();
    let context = editor.editing_context_token().unwrap();
    let initial = patch(
        editor.document().unwrap(),
        &object,
        &[(
            schema::document::PADDING_PIXELS.key,
            PropertyValue::Float(300.),
        )],
    );
    let token = editor.begin_property_edit(context, object.clone(), &initial)?;
    editor.update_property_edit(&token, &initial)?;
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        300.
    );
    assert_eq!(
        editor.document().unwrap().property_values(&object)?[&schema::document::PADDING_PIXELS.key],
        PropertyValue::Float(50.)
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    let mut latest = initial.clone();
    latest.changes.insert(
        schema::document::PADDING_PIXELS.key,
        PropertyValue::Float(400.),
    );
    editor.update_property_edit(&token, &latest)?;
    editor.finish_property_edit(&token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        400.
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        50.
    );
    let modified = editor.document().unwrap().modified();
    let token = editor.begin_property_edit(context, object.clone(), &initial)?;
    editor.update_property_edit(&token, &initial)?;
    editor.cancel_property_edit(&token);
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        50.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 1));
    assert_eq!(editor.document().unwrap().modified(), modified);
    Ok(())
}

#[test]
fn properties_preview_old_tokens_and_changed_targets_cannot_mutate_new_edits() -> anyhow::Result<()>
{
    let mut editor = Editor::new()?;
    let object = editor.document().unwrap().object_index().objects[0]
        .reference
        .clone();
    let context = editor.editing_context_token().unwrap();
    let changes = patch(
        editor.document().unwrap(),
        &object,
        &[(
            schema::document::PADDING_PIXELS.key,
            PropertyValue::Float(300.),
        )],
    );
    let old = editor.begin_property_edit(context, object.clone(), &changes)?;
    editor.cancel_property_edit(&old);
    let new = editor.begin_property_edit(context, object.clone(), &changes)?;
    assert!(editor.update_property_edit(&old, &changes).is_err());
    editor.cancel_property_edit(&old);
    assert!(editor.property_edit_owned(&new));
    let other_field = patch(
        editor.document().unwrap(),
        &object,
        &[(
            PropertyKey::new(ObjectKind::Document, "name"),
            PropertyValue::Text("unsupported".into()),
        )],
    );
    assert!(editor.update_property_edit(&new, &other_field).is_err());
    let mut forged = new.clone();
    forged.object.id = "another-object".into();
    assert!(!editor.property_edit_owned(&forged));
    assert!(editor.update_property_edit(&forged, &changes).is_err());
    editor.update_property_edit(&new, &changes)?;
    editor.end_editing(context.document, EditingLifecycle::WindowDeactivated)?;
    assert!(!editor.property_edit_owned(&new));
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        50.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn properties_preview_returning_to_baseline_and_navigation_do_not_commit() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let object = editor.document().unwrap().object_index().objects[0]
        .reference
        .clone();
    let context = editor.editing_context_token().unwrap();
    let mut changes = patch(
        editor.document().unwrap(),
        &object,
        &[(
            schema::document::PADDING_PIXELS.key,
            PropertyValue::Float(300.),
        )],
    );
    let token = editor.begin_property_edit(context, object.clone(), &changes)?;
    editor.update_property_edit(&token, &changes)?;
    changes.changes.clear();
    editor.update_property_edit(&token, &changes)?;
    editor.finish_property_edit(&token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    changes.changes.insert(
        schema::document::PADDING_PIXELS.key,
        PropertyValue::Float(500.),
    );
    let token = editor.begin_property_edit(context, object.clone(), &changes)?;
    editor.update_property_edit(&token, &changes)?;
    editor.execute(EditorCommand::Navigate(object))?;
    assert!(!editor.property_edit_owned(&token));
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        50.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn properties_rejected_commands_preserve_the_active_preview() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let object = editor.document().unwrap().object_index().objects[0]
        .reference
        .clone();
    let context = editor.editing_context_token().unwrap();
    let changes = patch(
        editor.document().unwrap(),
        &object,
        &[(
            schema::document::PADDING_PIXELS.key,
            PropertyValue::Float(300.),
        )],
    );
    let token = editor.begin_property_edit(context, object.clone(), &changes)?;
    editor.update_property_edit(&token, &changes)?;

    let invalid = patch(
        editor.document().unwrap(),
        &object,
        &[(
            schema::document::PADDING_PIXELS.key,
            PropertyValue::Float(f32::NAN),
        )],
    );
    assert!(
        editor
            .execute(EditorCommand::SetObjectProperties {
                object: object.clone(),
                patch: invalid.clone(),
            })
            .is_err()
    );
    assert!(
        editor
            .execute(EditorCommand::SetObjectProperty {
                object: object.clone(),
                field: "export_settings.padding_pixels".into(),
                value: serde_json::Value::Null,
            })
            .is_err()
    );
    assert!(
        editor
            .begin_property_edit(context, object.clone(), &invalid)
            .is_err()
    );
    // Read-only capabilities cannot start a preview.
    let unsupported = patch(
        editor.document().unwrap(),
        &object,
        &[(
            PropertyKey::new(ObjectKind::Document, "name"),
            PropertyValue::Text("unsupported".into()),
        )],
    );
    assert!(
        editor
            .begin_property_edit(context, object, &unsupported)
            .is_err()
    );

    assert!(editor.property_edit_owned(&token));
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        300.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.cancel_property_edit(&token);
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        50.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn properties_new_preview_replaces_previous_candidate_without_publishing_it() -> anyhow::Result<()>
{
    let mut editor = Editor::new()?;
    let object = editor.document().unwrap().object_index().objects[0]
        .reference
        .clone();
    let context = editor.editing_context_token().unwrap();
    let mut changes = patch(
        editor.document().unwrap(),
        &object,
        &[(
            schema::document::PADDING_PIXELS.key,
            PropertyValue::Float(300.),
        )],
    );
    let first = editor.begin_property_edit(context, object.clone(), &changes)?;
    editor.update_property_edit(&first, &changes)?;
    changes.changes.insert(
        schema::document::PADDING_PIXELS.key,
        PropertyValue::Float(400.),
    );
    let second = editor.begin_property_edit(context, object, &changes)?;
    assert!(!editor.property_edit_owned(&first));
    assert!(editor.property_edit_owned(&second));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.update_property_edit(&second, &changes)?;
    editor.cancel_property_edit(&first);
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        400.
    );
    editor.finish_property_edit(&second)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        50.
    );
    Ok(())
}

#[test]
fn identity_fields_preserve_scene_objects_references_and_undo() -> anyhow::Result<()> {
    let (mut doc, assets) = support::sample_document_with_scene_objects()?;
    for kind in [ObjectKind::Mesh, ObjectKind::Part, ObjectKind::Deformer] {
        let objects: Vec<_> = doc
            .object_index()
            .objects
            .iter()
            .filter(|info| info.reference.kind == kind)
            .map(|info| info.reference.clone())
            .collect();
        let object = &objects[0];
        let original = doc.property_values(object)?;
        let references = format!(
            "{:?}",
            doc.object_index().outgoing(object).collect::<Vec<_>>()
        );
        let history = doc.history_lengths();
        doc.set_object_property(object, "name", serde_json::json!("Renamed object"))?;
        doc.set_object_property(
            object,
            "runtime_id",
            serde_json::json!(format!("Updated{kind:?}")),
        )?;
        assert_eq!(
            doc.property_values(object)?[&PropertyKey::new(kind, "name")],
            PropertyValue::Text("Renamed object".into())
        );
        assert_eq!(
            references,
            format!(
                "{:?}",
                doc.object_index().outgoing(object).collect::<Vec<_>>()
            )
        );
        assert_eq!(doc.history_lengths().0, history.0 + 2);
        if let Some(other) = objects.get(1) {
            let duplicate =
                doc.property_values(other)?[&PropertyKey::new(kind, "runtime_id")].json();
            assert!(
                doc.set_object_property(object, "runtime_id", duplicate)
                    .is_err()
            );
            assert_eq!(doc.history_lengths().0, history.0 + 2);
        }
        doc.undo()?;
        doc.undo()?;
        assert_eq!(doc.property_values(object)?, original);
    }
    let saved = doc.save(&assets.path().join("identity.kasane"))?;
    Document::open(&saved.manifest)?;
    Ok(())
}

#[test]
fn parameter_runtime_id_keeps_animation_and_physics_bindings_and_roundtrips() -> anyhow::Result<()>
{
    let (mut doc, assets) = support::combined_animation_document()?;
    let parameter = ObjectRef::new(ObjectKind::Parameter, &doc.parameters()[0].id);
    let references = format!(
        "{:?}",
        doc.object_index().incoming(&parameter).collect::<Vec<_>>()
    );
    let original = doc.property_values(&parameter)?[&schema::parameter::RUNTIME_ID.key].clone();
    doc.set_object_property(&parameter, "runtime_id", serde_json::json!("ParamRenamed"))?;
    assert_eq!(
        doc.parameter_draft(&parameter.id)?.runtime_id,
        "ParamRenamed"
    );
    assert_eq!(
        references,
        format!(
            "{:?}",
            doc.object_index().incoming(&parameter).collect::<Vec<_>>()
        )
    );
    let saved = doc.save(&assets.path().join("renamed.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(
        reopened.parameter_draft(&parameter.id)?.runtime_id,
        "ParamRenamed"
    );
    doc.undo()?;
    assert_eq!(
        doc.property_values(&parameter)?[&schema::parameter::RUNTIME_ID.key],
        original
    );
    Ok(())
}

#[test]
fn rejected_range_drag_keeps_the_last_valid_candidate_until_release() -> anyhow::Result<()> {
    let (doc, _assets) = support::sample_document_with_parameters(1)?;
    let parameter = ObjectRef::new(ObjectKind::Parameter, &doc.parameters()[0].id);
    let mut editor = Editor::from_document(doc);
    let context = editor.editing_context_token().unwrap();
    let valid = patch(
        editor.document().unwrap(),
        &parameter,
        &[(schema::parameter::MAXIMUM.key, PropertyValue::Float(2.))],
    );
    let token = editor.begin_property_edit(context, parameter.clone(), &valid)?;
    editor.update_property_edit(&token, &valid)?;
    let invalid = patch(
        editor.document().unwrap(),
        &parameter,
        &[(schema::parameter::MAXIMUM.key, PropertyValue::Float(-1.))],
    );
    assert!(editor.update_property_edit(&token, &invalid).is_err());
    assert_eq!(
        editor
            .document()
            .unwrap()
            .parameter_draft(&parameter.id)?
            .maximum,
        2.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.finish_property_edit(&token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor
            .document()
            .unwrap()
            .parameter_draft(&parameter.id)?
            .maximum,
        1.
    );
    Ok(())
}

#[test]
fn animation_property_candidates_are_detached_and_disappear_on_cancel() -> anyhow::Result<()> {
    let (doc, _assets) = support::animation_document()?;
    let mut editor = support::editor_from_document(doc);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let selection = editor.document().unwrap().animation_catalog().motions[0]
        .selection
        .clone();
    let object = ObjectRef::new(ObjectKind::Motion, &selection.clip_id);
    editor.select_motion(editor.active_id().unwrap(), selection)?;
    let before = editor.animation_request().unwrap().source;
    let edit = patch(
        editor.document().unwrap(),
        &object,
        &[(
            schema::motion::FADE_IN.key,
            PropertyValue::OptionalFloat(Some(3.)),
        )],
    );
    let token = editor.begin_property_edit(
        editor.editing_context_token().unwrap(),
        object.clone(),
        &edit,
    )?;
    editor.update_property_edit(&token, &edit)?;
    let preview = editor.animation_request().unwrap();
    assert_eq!(
        preview
            .source
            .document()
            .get_motion(&object.id)
            .unwrap()
            .fade_in,
        Some(3.)
    );
    assert_ne!(
        before.document().get_motion(&object.id).unwrap().fade_in,
        Some(3.)
    );
    AnimationEvaluator::default().evaluate(&preview, |_, _| true)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.cancel_property_edit(&token);
    let after = editor.animation_request().unwrap().source;
    assert!(after.properties.is_none());
    assert_eq!(
        after.document().get_motion(&object.id).unwrap().fade_in,
        before.document().get_motion(&object.id).unwrap().fade_in
    );
    assert_eq!(
        preview
            .source
            .document()
            .get_motion(&object.id)
            .unwrap()
            .fade_in,
        Some(3.)
    );
    Ok(())
}

#[test]
fn parameter_range_preview_clamps_display_value_and_committed_value_on_release()
-> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_parameters(1)?;
    let object = ObjectRef::new(ObjectKind::Parameter, &doc.parameters()[0].id);
    doc.set_object_property(&object, "maximum", serde_json::json!(20.))?;
    doc.set_object_property(&object, "default_value", serde_json::json!(15.))?;
    doc.set_preview_parameter(&object.id, 0.)?;
    let mut editor = Editor::from_document(doc);
    let edit = patch(
        editor.document().unwrap(),
        &object,
        &[(schema::parameter::MINIMUM.key, PropertyValue::Float(10.))],
    );
    let token =
        editor.begin_property_edit(editor.editing_context_token().unwrap(), object, &edit)?;
    editor.update_property_edit(&token, &edit)?;
    let value = &editor.document().unwrap().parameters()[0];
    assert_eq!((value.minimum, value.value), (10., 10.));
    editor.finish_property_edit(&token)?;
    let value = &editor.document().unwrap().parameters()[0];
    assert_eq!((value.minimum, value.value), (10., 10.));
    Ok(())
}

#[test]
fn document_export_properties_preserve_geometry_and_support_history() -> anyhow::Result<()> {
    let (mut doc, assets) = support::rectangle_document(false)?;
    let object = doc
        .object_index()
        .objects
        .iter()
        .find(|o| o.reference.kind == ObjectKind::Document)
        .unwrap()
        .reference
        .clone();
    let source = doc.source_canvas();
    let before = doc.snapshot()?;
    let settings = doc.export_settings();
    let changes = patch(
        &doc,
        &object,
        &[
            (
                schema::document::PADDING_PIXELS.key,
                PropertyValue::Float(25.),
            ),
            (
                schema::document::MINIMUM_EXTENT.key,
                PropertyValue::Float(80.),
            ),
        ],
    );
    doc.set_object_properties(&object, &changes)?;
    assert_eq!(doc.source_canvas(), source);
    assert_eq!(
        doc.snapshot()?.runtime_frame.drawables,
        before.runtime_frame.drawables
    );
    assert_eq!(doc.export_settings().padding_pixels, 25.);
    doc.undo()?;
    assert_eq!(doc.export_settings(), settings);
    doc.redo()?;
    assert_eq!(doc.export_settings().padding_pixels, 25.);
    let invalid = patch(
        &doc,
        &object,
        &[(
            schema::document::PADDING_PIXELS.key,
            PropertyValue::Float(f32::NAN),
        )],
    );
    assert!(doc.set_object_properties(&object, &invalid).is_err());
    let saved = doc.save(&assets.path().join("settings.kasane"))?;
    assert_eq!(
        Document::open(&saved.manifest)?.export_settings(),
        doc.export_settings()
    );
    Ok(())
}

#[test]
fn parameter_group_identity_properties_use_group_validation_and_history() -> anyhow::Result<()> {
    let mut doc = Document::new_blank()?;
    let group = doc.create_parameter_group(
        ParameterGroupDraft {
            name: "Face".into(),
            runtime_id: "GroupFace".into(),
            parent_id: None,
        },
        doc.parameter_model_revision(),
    )?;
    let other = doc.create_parameter_group(
        ParameterGroupDraft {
            name: "Eyes".into(),
            runtime_id: "GroupEyes".into(),
            parent_id: Some(group.clone()),
        },
        doc.parameter_model_revision(),
    )?;
    let target = ObjectRef::new(ObjectKind::ParameterGroup, &other);
    let original = doc.property_values(&target)?;
    let rename = patch(
        &doc,
        &target,
        &[(
            schema::parameter_group::NAME.key,
            PropertyValue::Text("Lids".into()),
        )],
    );
    doc.set_object_properties(&target, &rename)?;
    assert_eq!(doc.parameter_group_draft(&other)?.name, "Lids");
    assert_eq!(
        doc.parameter_group_draft(&other)?.parent_id.as_deref(),
        Some(group.as_str())
    );
    let history = doc.history_lengths();
    let duplicate = patch(
        &doc,
        &target,
        &[(
            schema::parameter_group::RUNTIME_ID.key,
            PropertyValue::Text("GroupFace".into()),
        )],
    );
    assert!(doc.set_object_properties(&target, &duplicate).is_err());
    assert_eq!(doc.history_lengths(), history);
    assert_eq!(doc.parameter_group_draft(&other)?.runtime_id, "GroupEyes");
    assert!(
        doc.set_object_properties(&target, &rename).is_err(),
        "stale baseline must be rejected"
    );
    doc.undo()?;
    assert_eq!(doc.property_values(&target)?, original);
    doc.redo()?;
    assert_eq!(doc.parameter_group_draft(&other)?.name, "Lids");
    Ok(())
}

#[test]
fn parameter_choice_properties_validate_and_undo_membership_and_kind() -> anyhow::Result<()> {
    let mut doc = Document::new_blank()?;
    let parameter = doc.create_parameter(NewParameter::default())?;
    let group = doc.create_parameter_group(
        ParameterGroupDraft {
            name: "Face".into(),
            runtime_id: "GroupFace".into(),
            parent_id: None,
        },
        doc.parameter_model_revision(),
    )?;
    let target = ObjectRef::new(ObjectKind::Parameter, &parameter);
    let assignment = patch(
        &doc,
        &target,
        &[(
            schema::parameter::GROUP_ID.key,
            PropertyValue::Text(group.clone()),
        )],
    );
    doc.set_object_properties(&target, &assignment)?;
    assert_eq!(
        doc.parameter_draft(&parameter)?.group_id.as_deref(),
        Some(group.as_str())
    );
    doc.undo()?;
    assert!(doc.parameter_draft(&parameter)?.group_id.is_none());
    doc.redo()?;
    let invalid = patch(
        &doc,
        &target,
        &[(
            schema::parameter::GROUP_ID.key,
            PropertyValue::Text("missing".into()),
        )],
    );
    let history = doc.history_lengths();
    assert!(doc.set_object_properties(&target, &invalid).is_err());
    assert_eq!(doc.history_lengths(), history);
    let kind = patch(
        &doc,
        &target,
        &[(
            schema::parameter::KIND.key,
            PropertyValue::Text("blend_shape".into()),
        )],
    );
    doc.set_object_properties(&target, &kind)?;
    assert_eq!(
        doc.parameter_draft(&parameter)?.kind,
        kasane_core::ParameterKind::BlendShape
    );
    doc.undo()?;
    assert_eq!(
        doc.parameter_draft(&parameter)?.kind,
        kasane_core::ParameterKind::Normal
    );
    let ungroup = patch(
        &doc,
        &target,
        &[(
            schema::parameter::GROUP_ID.key,
            PropertyValue::Text(String::new()),
        )],
    );
    doc.set_object_properties(&target, &ungroup)?;
    assert!(doc.parameter_draft(&parameter)?.group_id.is_none());
    Ok(())
}

#[test]
fn object_scoped_keys_reject_foreign_fields_and_resolve_canonical_paths() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_parts()?;
    let object = support::object(ObjectKind::Part, 101);
    let baseline = doc.property_values(&object)?;
    let history = doc.history_lengths();
    let foreign = PropertyPatch {
        scope: PropertyScope::Object,
        expected: [(schema::mesh::ENABLED.key, PropertyValue::Bool(true))].into(),
        changes: [(schema::mesh::ENABLED.key, PropertyValue::Bool(false))].into(),
    };
    assert!(doc.set_object_properties(&object, &foreign).is_err());
    assert_eq!(doc.property_values(&object)?, baseline);
    assert_eq!(doc.history_lengths(), history);

    // Patches carry identities, not caller-selected storage paths. Even a
    // descriptor with a different path cannot redirect an authorized write.
    let alternate = PropertySpec::new(ObjectKind::Part, "enabled", "name", PropertyType::Bool);
    let edit = patch(
        &doc,
        &object,
        &[(alternate.key, PropertyValue::Bool(false))],
    );
    doc.set_object_properties(&object, &edit)?;
    let values = doc.property_values(&object)?;
    assert_eq!(
        values[&schema::part::ENABLED.key],
        PropertyValue::Bool(false)
    );
    assert_eq!(
        values[&schema::part::NAME.key],
        baseline[&schema::part::NAME.key]
    );
    doc.undo()?;
    assert_eq!(doc.property_values(&object)?, baseline);
    Ok(())
}
