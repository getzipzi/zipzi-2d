use super::*;
use crate::property::part as fields;
use crate::PropertyValue;
use serde_json::json;
use zipzi_core::{
    BindingAxis, BindingAxisEditing, OffscreenLookup, Part, PartKeyform, SceneBinding, SceneTrack,
};

fn fixture(bound: bool) -> Result<(Document, ObjectRef, String)> {
    let mut doc = Document::new_blank()?;
    let part = ObjectRef::new(ObjectKind::Part, uuid::Uuid::new_v4().to_string());
    let parameter = uuid::Uuid::new_v4().to_string();
    doc.session.edit("Fixture", None, |edit| {
        edit.create_part(Part {
            id: part.id.clone(),
            name: "Eyes".into(),
            ..Default::default()
        })?;
        if bound {
            edit.create_parameter(zipzi_core::Parameter {
                id: parameter.clone(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            edit.create_scene_binding(SceneBinding {
                id: uuid::Uuid::new_v4().to_string(),
                axes: vec![BindingAxis::new(parameter.clone(), vec![0., 1.])],
                track: SceneTrack::Part {
                    target_id: part.id.clone().into(),
                    keyforms: [0., 1.]
                        .map(|key| PartKeyform {
                            keys: vec![key],
                            draw_order: 0.,
                            ..Default::default()
                        })
                        .to_vec(),
                },
            })?;
        }
        Ok(())
    })?;
    doc.invalidate_projection(true);
    Ok((doc, part, parameter))
}

fn native_lock(doc: &mut Document, id: &str, locked: bool) -> Result<()> {
    doc.session.edit("Native lock", None, |edit| {
        let mut state = edit.candidate_document().editor_state().clone();
        state.objects.entry(id.into()).or_default().locked = locked;
        edit.replace_editor_state(state)
    })?;
    doc.invalidate_projection(true);
    Ok(())
}

#[test]
fn part_offscreen_toggle_preserves_part_and_roundtrips_history_and_project() -> Result<()> {
    let (mut doc, part, _) = fixture(false)?;
    let original = doc.model().get_part(&part.id).unwrap().clone();
    let before = doc.history_lengths();
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    let os = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    assert!(os.enabled);
    assert_eq!(doc.model().get_part(&part.id), Some(&original));
    assert_eq!(doc.history_lengths().0, before.0 + 1);
    let tree = doc.part_tree();
    let node = tree.iter().find(|node| node.id == part.id).unwrap();
    assert!(node.offscreen);
    assert!(node.children.is_empty());
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    assert_eq!(doc.history_lengths().0, before.0 + 1);
    doc.set_object_property(&part, fields::OPACITY.path, json!(0.4))?;
    doc.set_object_property(&part, fields::MULTIPLY.path, json!([0.2, 0.3, 0.4]))?;
    let stored = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    let directory = tempfile::tempdir()?;
    let receipt = doc.save(&directory.path().join("offscreen.zipzi"))?;
    let reopened = Document::open(&receipt.path)?;
    assert_eq!(reopened.model().offscreen_for_part(&part.id), Some(&stored));
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(false))?;
    assert!(doc.model().offscreen_for_part(&part.id).is_none());
    assert_eq!(doc.model().get_part(&part.id), Some(&original));
    doc.undo()?;
    assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&stored));
    doc.redo()?;
    assert!(
        !doc.part_tree()
            .iter()
            .find(|node| node.id == part.id)
            .unwrap()
            .offscreen
    );
    Ok(())
}

#[test]
fn part_offscreen_stored_form_edits_preserve_aliases_and_neutral_sentinels() -> Result<()> {
    let (mut doc, part, parameter) = fixture(true)?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    let binding = doc.model().binding_for_scene(&part.id).unwrap().clone();
    let mut os = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    os.keyforms = vec![zipzi_core::OffscreenKeyform {
        opacity: 0.8,
        ..Default::default()
    }];
    os.part_keyform_indices = vec![0, 0];
    doc.session.edit("Aliased fixture", None, |edit| {
        edit.replace_offscreen(os.clone())
    })?;
    doc.invalidate_projection(true);
    assert!(matches!(
        doc.property_scope(&part),
        PropertyScope::OffscreenKeyform { .. }
    ));
    doc.set_object_property(&part, fields::OPACITY.path, json!(0.25))?;
    let edited = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    assert_eq!(edited.keyforms[0], os.keyforms[0]);
    assert_eq!(edited.part_keyform_indices, vec![1, 0]);
    assert_eq!(edited.keyforms[1].opacity, 0.25);
    assert_eq!(doc.model().binding_for_scene(&part.id), Some(&binding));
    doc.set_preview_parameter(&parameter, 1.)?;
    assert_eq!(
        doc.property_values(&part)?[&fields::OPACITY.key],
        PropertyValue::Float(0.8)
    );
    doc.undo()?;
    let mut neutral = os.clone();
    neutral.part_keyform_indices = vec![-7, -1];
    doc.session.edit("Neutral fixture", None, |edit| {
        edit.replace_offscreen(neutral.clone())
    })?;
    doc.invalidate_projection(true);
    doc.set_preview_parameter(&parameter, 0.)?;
    doc.set_object_property(&part, fields::SCREEN.path, json!([0.1, 0.2, 0.3]))?;
    let model = doc.model();
    let result = model.offscreen_for_part(&part.id).unwrap();
    assert_eq!(result.part_keyform_indices[1], -1);
    assert_eq!(result.keyforms[0], neutral.keyforms[0]);
    let form = &result.keyforms[result.keyform_index(0, Some(2)).unwrap().unwrap()];
    assert_eq!(form.opacity, 1.);
    assert_eq!(form.screen, Some([0.1, 0.2, 0.3]));
    Ok(())
}

#[test]
fn part_offscreen_interpolated_appearance_is_guarded_and_static_settings_remain_editable(
) -> Result<()> {
    let (mut doc, part, parameter) = fixture(true)?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    doc.set_preview_parameter(&parameter, 0.5)?;
    assert_eq!(
        doc.property_scope(&part),
        PropertyScope::InterpolatedOffscreen
    );
    let before = doc.history_lengths();
    assert!(doc
        .set_object_property(&part, fields::OPACITY.path, json!(0.5))
        .is_err());
    assert_eq!(doc.history_lengths(), before);
    doc.set_object_property(&part, fields::COLOR_BLEND.path, json!("screen"))?;
    doc.set_object_property(&part, fields::INVERTED_MASK.path, json!(true))?;
    let os = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    assert_eq!(os.blend_mode & 255, 10);
    assert_eq!(os.flags, 12);
    assert!(os.keyforms.is_empty());
    Ok(())
}

#[test]
fn part_offscreen_preview_is_cancelable_and_commits_one_history_step() -> Result<()> {
    let (mut doc, part, _) = fixture(false)?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    let before = doc.history_lengths();
    let patch = doc.object_property_patch(&part, fields::OPACITY.path, &json!(0.3))?;
    let serial = doc.begin_property_edit(&part, &patch)?;
    doc.update_property_edit(&part, serial, &patch)?;
    assert!(doc.property_edit_matches(serial, &part));
    assert_eq!(doc.snapshot()?.viewport_frame.offscreens[0].opacity, 0.3);
    assert_eq!(
        doc.property_values(&part)?[&fields::OPACITY.key],
        PropertyValue::Float(1.)
    );
    doc.cancel_edit();
    assert_eq!(doc.history_lengths(), before);
    assert_eq!(doc.snapshot()?.viewport_frame.offscreens[0].opacity, 1.);
    let serial = doc.begin_property_edit(&part, &patch)?;
    doc.update_property_edit(&part, serial, &patch)?;
    doc.finish_edit()?;
    assert_eq!(doc.history_lengths().0, before.0 + 1);
    doc.undo()?;
    assert_eq!(doc.snapshot()?.viewport_frame.offscreens[0].opacity, 1.);
    Ok(())
}

#[test]
fn part_offscreen_removal_preserves_nested_parts_and_their_composites() -> Result<()> {
    let (mut doc, part, _) = fixture(false)?;
    let child = ObjectRef::new(ObjectKind::Part, uuid::Uuid::new_v4().to_string());
    doc.session.edit("Child Part", None, |edit| {
        edit.create_part(Part {
            id: child.id.clone(),
            parent_id: part.id.clone(),
            name: "Child".into(),
            ..Default::default()
        })
    })?;
    doc.invalidate_projection(true);
    doc.set_object_property(&child, fields::OFFSCREEN.path, json!(true))?;
    doc.set_object_property(&child, fields::OPACITY.path, json!(0.7))?;
    let stored_child = doc.model().offscreen_for_part(&child.id).unwrap().clone();
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    let parent_os = doc.model().offscreen_for_part(&part.id).unwrap().id.clone();
    assert_eq!(
        doc.snapshot()?
            .viewport_frame
            .offscreens
            .iter()
            .find(|os| os.id == stored_child.id)
            .unwrap()
            .parent_offscreen_id,
        Some(parent_os)
    );
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(false))?;
    assert_eq!(
        doc.model().offscreen_for_part(&child.id),
        Some(&stored_child)
    );
    let tree = doc.part_tree();
    let parent = tree.iter().find(|node| node.id == part.id).unwrap();
    assert!(!parent.offscreen);
    assert_eq!(parent.children.len(), 1);
    assert_eq!(parent.children[0].id, child.id);
    assert!(parent.children[0].offscreen);
    assert_eq!(
        doc.snapshot()?
            .viewport_frame
            .offscreens
            .iter()
            .find(|os| os.id == stored_child.id)
            .unwrap()
            .parent_offscreen_id,
        None
    );
    Ok(())
}

#[test]
fn part_offscreen_stale_component_baselines_and_locks_reject_without_history() -> Result<()> {
    let (mut doc, part, _) = fixture(false)?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    let stale = doc.object_property_patch(&part, fields::OPACITY.path, &json!(0.5))?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(false))?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    let before = doc.history_lengths();
    assert!(doc.set_object_properties(&part, &stale).is_err());
    assert_eq!(doc.history_lengths(), before);
    let os = ObjectRef::new(
        ObjectKind::Offscreen,
        doc.model().offscreen_for_part(&part.id).unwrap().id.clone(),
    );
    for object in [os, part.clone()] {
        doc.set_objects_locked(std::slice::from_ref(&object), true)?;
        let before = doc.history_lengths();
        assert!(doc
            .set_object_property(&part, fields::OPACITY.path, json!(0.5))
            .is_err());
        assert!(doc
            .set_object_property(&part, fields::OFFSCREEN.path, json!(false))
            .is_err());
        assert_eq!(doc.history_lengths(), before);
        doc.set_objects_locked(std::slice::from_ref(&object), false)?;
    }
    Ok(())
}

#[test]
fn part_offscreen_deletion_cleans_owned_blends_but_retains_shared_animation_and_locks() -> Result<()>
{
    use zipzi_core::{
        BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
        DeltaKeyforms, DeltaOffscreenKeyform, Parameter, ParameterKind,
    };
    let (mut doc, part, ordinary_parameter) = fixture(true)?;
    let other = ObjectRef::new(ObjectKind::Part, uuid::Uuid::new_v4().to_string());
    doc.session.edit("Other Part", None, |edit| {
        edit.create_part(Part {
            id: other.id.clone(),
            ..Default::default()
        })
    })?;
    doc.invalidate_projection(true);
    for target in [&part, &other] {
        doc.set_object_property(target, fields::OFFSCREEN.path, json!(true))?;
    }
    let offscreen = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    let other_offscreen = doc.model().offscreen_for_part(&other.id).unwrap().clone();
    let ordinary_binding = doc.model().binding_for_scene(&part.id).unwrap().clone();
    let blend_parameter = uuid::Uuid::new_v4().to_string();
    let table = uuid::Uuid::new_v4().to_string();
    let constraint = uuid::Uuid::new_v4().to_string();
    let own_blend = uuid::Uuid::new_v4().to_string();
    let other_blend = uuid::Uuid::new_v4().to_string();
    doc.session.edit("Shared BlendShape", None, |edit| {
        edit.create_parameter(Parameter {
            id: blend_parameter.clone(),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
        edit.create_blend_key_table(BlendShapeKeyTable {
            id: table.clone(),
            parameter_id: blend_parameter.clone(),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: Default::default(),
            base_sample_id: Default::default(),
        })?;
        edit.create_blend_constraint(BlendShapeConstraint {
            id: constraint.clone(),
            parameter_id: ordinary_parameter.clone(),
            keys: vec![0., 1.],
            weights: vec![1., 1.],
            samples: Default::default(),
        })?;
        for (id, target) in [
            (&own_blend, &offscreen.id),
            (&other_blend, &other_offscreen.id),
        ] {
            edit.create_blend_binding(BlendShapeBinding {
                id: id.clone(),
                target_id: target.clone(),
                target_kind: BlendShapeTargetKind::Offscreen,
                key_table_id: table.clone(),
                constraint_ids: vec![constraint.clone()],
                keyforms: DeltaKeyforms::Offscreen(vec![
                    DeltaOffscreenKeyform::default(),
                    DeltaOffscreenKeyform {
                        opacity: -0.4,
                        ..Default::default()
                    },
                ]),
            })?;
        }
        Ok(())
    })?;
    doc.invalidate_projection(true);
    let own = doc.model().get_blend_binding(&own_blend).unwrap().clone();
    let retained = doc.model().get_blend_binding(&other_blend).unwrap().clone();
    let table_before = doc.model().get_blend_key_table(&table).unwrap().clone();
    let constraint_before = doc
        .model()
        .get_blend_constraint(&constraint)
        .unwrap()
        .clone();
    native_lock(&mut doc, &own_blend, true)?;
    let history = doc.history_lengths();
    assert!(doc
        .set_object_property(&part, fields::OFFSCREEN.path, json!(false))
        .is_err());
    assert_eq!(doc.history_lengths(), history);
    assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&offscreen));
    assert_eq!(doc.model().get_blend_binding(&own_blend), Some(&own));
    native_lock(&mut doc, &own_blend, false)?;
    // Shared dependencies are read-only during component deletion, so their
    // locks must neither block deletion nor be removed as an incidental effect.
    for id in [&table, &constraint] {
        native_lock(&mut doc, id, true)?;
    }
    let history = doc.history_lengths();
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(false))?;
    assert_eq!(doc.history_lengths().0, history.0 + 1);
    let model = doc.model();
    assert!(model.get_offscreen(&offscreen.id).is_none());
    assert!(model.get_blend_binding(&own_blend).is_none());
    assert!(!model.editor_state().objects.contains_key(&own_blend));
    assert!(model.get_parameter(&ordinary_parameter).is_some());
    assert!(model.get_parameter(&blend_parameter).is_some());
    assert_eq!(model.binding_for_scene(&part.id), Some(&ordinary_binding));
    assert_eq!(
        model.get_offscreen(&other_offscreen.id),
        Some(&other_offscreen)
    );
    assert_eq!(model.get_blend_binding(&other_blend), Some(&retained));
    assert_eq!(model.get_blend_key_table(&table), Some(&table_before));
    assert_eq!(
        model.get_blend_constraint(&constraint),
        Some(&constraint_before)
    );
    assert!(model.editor_state().get(&table).locked);
    assert!(model.editor_state().get(&constraint).locked);
    drop(model);
    doc.undo()?;
    assert_eq!(doc.model().get_offscreen(&offscreen.id), Some(&offscreen));
    assert_eq!(doc.model().get_blend_binding(&own_blend), Some(&own));
    doc.redo()?;
    assert!(doc.model().get_blend_binding(&own_blend).is_none());
    assert_eq!(doc.model().get_blend_binding(&other_blend), Some(&retained));
    Ok(())
}

#[test]
fn part_offscreen_part_deletion_reports_hidden_components_and_restores_them_on_undo() -> Result<()>
{
    use crate::PaletteProjection;
    let (mut doc, part, _) = fixture(true)?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    let offscreen = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    let binding = doc.model().binding_for_scene(&part.id).unwrap().clone();
    let plan =
        doc.plan_tree_deletion(PaletteProjection::Organization, std::slice::from_ref(&part))?;
    assert!(plan.removed_ids().contains(&offscreen.id));
    let owned = plan.page(zipzi_sdk::DeletionSection::Owned, 0, 256)?;
    assert!(owned.rows.iter().any(|row| matches!(row,
        zipzi_sdk::DeletionRow::Removal { removal } if removal.object.id == binding.id)));
    let history = doc.history_lengths();
    doc.execute_tree_deletion(&plan)?;
    assert_eq!(doc.history_lengths().0, history.0 + 1);
    assert!(doc.model().get_part(&part.id).is_none());
    assert!(doc.model().get_offscreen(&offscreen.id).is_none());
    assert!(doc.model().get_scene_binding(&binding.id).is_none());
    doc.undo()?;
    assert_eq!(doc.model().get_offscreen(&offscreen.id), Some(&offscreen));
    assert_eq!(doc.model().get_scene_binding(&binding.id), Some(&binding));
    assert!(
        doc.part_tree()
            .iter()
            .find(|node| node.id == part.id)
            .unwrap()
            .offscreen
    );
    doc.redo()?;
    assert!(doc.model().get_part(&part.id).is_none());
    assert!(doc.model().get_offscreen(&offscreen.id).is_none());
    Ok(())
}

#[test]
fn part_offscreen_enabled_recovers_native_visibility_without_replacing_appearance() -> Result<()> {
    for bound in [false, true] {
        let (mut doc, part, _) = fixture(bound)?;
        doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
        doc.set_object_property(&part, fields::OPACITY.path, json!(0.4))?;
        doc.set_object_property(&part, fields::COMPOSITE_VISIBLE.path, json!(false))?;
        let original = doc.model().offscreen_for_part(&part.id).unwrap().clone();
        assert!(doc.model().get_part(&part.id).unwrap().enabled);
        assert_eq!(
            doc.property_values(&part)?[&fields::ENABLED.key],
            PropertyValue::Bool(false)
        );
        // Editing unrelated fields must preserve the raw Part flag and the
        // hidden native visibility state rather than normalizing either one.
        doc.set_object_property(&part, fields::NAME.path, json!("Renamed"))?;
        assert!(doc.model().get_part(&part.id).unwrap().enabled);
        assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&original));
        let binding = doc.model().binding_for_scene(&part.id).cloned();
        let history = doc.history_lengths();
        doc.set_object_property(&part, fields::ENABLED.path, json!(true))?;
        let mut visible = original.clone();
        visible.enabled = true;
        assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&visible));
        assert_eq!(doc.snapshot()?.viewport_frame.offscreens[0].opacity, 0.4);
        assert!(doc.snapshot()?.viewport_frame.offscreens[0].enabled);
        assert_eq!(doc.model().binding_for_scene(&part.id), binding.as_ref());
        assert_eq!(doc.history_lengths().0, history.0 + 1);
        doc.undo()?;
        assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&original));
        assert_eq!(
            doc.property_values(&part)?[&fields::ENABLED.key],
            PropertyValue::Bool(false)
        );
        doc.redo()?;
        assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&visible));
        let history = doc.history_lengths();
        doc.set_object_property(&part, fields::ENABLED.path, json!(true))?;
        assert_eq!(doc.history_lengths(), history);
    }
    Ok(())
}

#[test]
fn part_offscreen_enabled_recovery_honors_component_locks_atomically() -> Result<()> {
    let (mut doc, part, _) = fixture(false)?;
    doc.set_object_property(&part, fields::OFFSCREEN.path, json!(true))?;
    doc.set_object_property(&part, fields::OPACITY.path, json!(0.4))?;
    doc.set_object_property(&part, fields::COMPOSITE_VISIBLE.path, json!(false))?;
    let os = doc.model().offscreen_for_part(&part.id).unwrap().clone();
    native_lock(&mut doc, &os.id, true)?;
    for raw_part_enabled in [true, false] {
        doc.session.edit("Raw Part visibility", None, |edit| {
            let mut part = edit
                .candidate_document()
                .get_part(&part.id)
                .unwrap()
                .clone();
            part.enabled = raw_part_enabled;
            edit.replace_part(part)
        })?;
        doc.invalidate_projection(true);
        let history = doc.history_lengths();
        assert!(doc
            .set_object_property(&part, fields::ENABLED.path, json!(true))
            .is_err());
        assert_eq!(doc.history_lengths(), history);
        assert_eq!(
            doc.model().get_part(&part.id).unwrap().enabled,
            raw_part_enabled
        );
        assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&os));
    }
    native_lock(&mut doc, &os.id, false)?;
    let history = doc.history_lengths();
    doc.set_object_property(&part, fields::ENABLED.path, json!(true))?;
    assert!(doc.model().get_part(&part.id).unwrap().enabled);
    assert!(doc.model().offscreen_for_part(&part.id).unwrap().enabled);
    assert_eq!(doc.history_lengths().0, history.0 + 1);
    doc.undo()?;
    assert!(!doc.model().get_part(&part.id).unwrap().enabled);
    assert_eq!(doc.model().offscreen_for_part(&part.id), Some(&os));
    Ok(())
}
