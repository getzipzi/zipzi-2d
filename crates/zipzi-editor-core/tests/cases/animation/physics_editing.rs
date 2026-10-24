use super::super::support;
use zipzi_editor_core::*;

fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (document, assets) = support::combined_animation_document()?;
    let mut editor = crate::support::editor_from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    let id = editor.active_id().unwrap();
    editor.set_animation_preview_mode(id, AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    Ok((editor, assets))
}
fn sample(
    editor: &mut Editor,
    evaluator: &mut AnimationEvaluator,
) -> anyhow::Result<AnimationEvaluated> {
    let request = editor.animation_request().expect("changed preview");
    let frame = evaluator.evaluate(&request, |_, _| true)?;
    assert!(editor.present_animation(frame.identity, frame.snapshot.clone()));
    Ok(frame)
}

fn target(editor: &Editor) -> ObjectRef {
    let asset = editor.document().unwrap().physics_definition().unwrap();
    ObjectRef::new(ObjectKind::PhysicsGroup, asset.data.settings[0].id.clone())
}
fn patch(editor: &Editor, object: &ObjectRef, spec: PropertySpec, value: f32) -> PropertyPatch {
    let current = editor.document().unwrap().property_values(object).unwrap();
    PropertyPatch {
        scope: PropertyScope::Object,
        expected: current
            .into_iter()
            .filter(|(key, _)| {
                *key == spec.key || property_spec(*key).is_some_and(|other| other.same_group(&spec))
            })
            .collect(),
        changes: [(spec.key, PropertyValue::Float(value))].into(),
    }
}
fn indexed(editor: &Editor, mut spec: PropertySpec, index: usize) -> PropertySpec {
    let rig = &editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .data
        .settings[0];
    spec.key = spec.key.for_entity(&rig.vertices[index].id);
    spec
}
#[test]
fn returning_from_manual_physics_rebuilds_missing_or_other_document_editing_cache()
-> anyhow::Result<()> {
    for warm_other_document in [false, true] {
        let mut evaluator = AnimationEvaluator::default();
        if warm_other_document {
            let (mut other, _assets) = setup()?;
            other.set_animation_preview_mode(
                other.active_id().unwrap(),
                AnimationPreviewMode::Editing,
            )?;
            sample(&mut other, &mut evaluator)?;
        }
        let (mut editor, _assets) = setup()?;
        let id = editor.active_id().unwrap();
        editor.set_animation_preview_mode(id, AnimationPreviewMode::Editing)?;
        // The first editing frame may be coalesced away before manual mode starts.
        editor.set_physics_manual(id, true)?;
        editor.advance_animation_clock(0.25);
        sample(&mut editor, &mut evaluator)?;
        editor.set_physics_manual(id, false)?;
        let request = editor.animation_request().unwrap();
        let resumed = evaluator.evaluate(&request, |_, _| true)?;
        let fresh = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(resumed.snapshot, fresh.snapshot);
        assert!(editor.present_animation(resumed.identity, resumed.snapshot));
    }
    Ok(())
}

#[test]
fn physics_candidates_cancel_commit_and_undo_as_one_edit() -> anyhow::Result<()> {
    use property::physics as schema;
    let (mut editor, _assets) = setup()?;
    let object = target(&editor);
    let original = editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .as_ref()
        .clone();
    let spec = indexed(&editor, schema::MOBILITY, 1);
    let initial = patch(&editor, &object, spec, 0.2);
    let token = editor.begin_property_edit(
        editor.editing_context_token().unwrap(),
        object.clone(),
        &initial,
    )?;
    let old_index = editor.document().unwrap().object_index();
    let old_payload = old_index.get(&object).unwrap().properties.clone();
    for value in [0.2, 0.4, 0.7] {
        let patch = patch(&editor, &object, spec, value);
        editor.update_property_edit(&token, &patch)?;
        let doc = editor.document().unwrap();
        assert_eq!(
            doc.object_index().get(&object).unwrap().properties["vertices"][1]["mobility"],
            serde_json::json!(value)
        );
        assert_eq!(old_index.get(&object).unwrap().properties, old_payload);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert_eq!(
        editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .settings[0]
            .vertices[1]
            .mobility,
        0.7
    );
    editor.cancel_property_edit(&token);
    assert_eq!(
        editor.document().unwrap().physics_definition().as_deref(),
        Some(&original)
    );
    let spec = indexed(&editor, schema::RADIUS, 1);
    let patch = patch(&editor, &object, spec, 8.);
    let token =
        editor.begin_property_edit(editor.editing_context_token().unwrap(), object, &patch)?;
    editor.update_property_edit(&token, &patch)?;
    editor.finish_property_edit(&token)?;
    let committed = editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .as_ref()
        .clone();
    assert_eq!(committed.data.settings[0].vertices[1].radius, 8.);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().physics_definition().as_deref(),
        Some(&original)
    );
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        editor.document().unwrap().physics_definition().as_deref(),
        Some(&committed)
    );
    Ok(())
}
#[test]
fn rejected_physics_write_preserves_candidate_and_context_switch_cancels_it() -> anyhow::Result<()>
{
    use property::physics as schema;
    let (mut editor, _assets) = setup()?;
    let object = target(&editor);
    let original = editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .as_ref()
        .clone();
    let valid = patch(&editor, &object, schema::FPS, 60.);
    let token = editor.begin_property_edit(
        editor.editing_context_token().unwrap(),
        object.clone(),
        &valid,
    )?;
    editor.update_property_edit(&token, &valid)?;
    let invalid = patch(&editor, &object, schema::FPS, f32::NAN);
    assert!(editor.update_property_edit(&token, &invalid).is_err());
    assert!(editor.property_edit_owned(&token));
    assert_eq!(
        editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .meta
            .fps,
        Some(60.)
    );
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert!(!editor.property_edit_owned(&token));
    assert_eq!(
        editor.document().unwrap().physics_definition().as_deref(),
        Some(&original)
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn manual_physics_and_bypass_replay_without_document_or_timeline_writes() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    let original = editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .as_ref()
        .clone();
    let input = original.data.settings[0].inputs[0]
        .source
        .parameter_id()
        .unwrap()
        .to_owned();
    let output = original.data.settings[0].outputs[0]
        .destination
        .parameter_id()
        .unwrap()
        .to_owned();
    let mut evaluator = AnimationEvaluator::default();
    editor.set_physics_manual(id, true)?;
    assert!(editor.animation().unwrap().playing);
    editor.set_physics_input(id, &input, 1.)?;
    editor.advance_animation_clock(0.5);
    let simulated = sample(&mut editor, &mut evaluator)?;
    assert!(simulated.snapshot.active_motions.is_empty());
    assert!(!simulated.snapshot.physics.is_empty());
    let observed = &simulated.snapshot.physics[0];
    assert_eq!(observed.particles.len(), 2);
    assert!(observed.outputs[0].is_some());
    assert_eq!(
        observed.outputs[0].as_ref().unwrap().after,
        simulated.snapshot.parameters[&output]
    );
    assert_ne!(simulated.snapshot.parameters[&output], 0.);
    editor.set_physics_bypass(id, true)?;
    let bypassed = sample(&mut editor, &mut evaluator)?;
    assert!(bypassed.snapshot.physics.is_empty());
    assert_eq!(bypassed.snapshot.parameters[&output], 0.);
    editor.set_physics_bypass(id, false)?;
    let replayed = sample(&mut editor, &mut evaluator)?;
    assert_eq!(simulated.snapshot, replayed.snapshot);
    editor.set_physics_input(id, &input, -1.)?;
    editor.advance_animation_clock(1.);
    let request = editor.animation_request().unwrap();
    assert_eq!(
        evaluator.evaluate(&request, |_, _| true)?.snapshot,
        AnimationEvaluator::default()
            .evaluate(&request, |_, _| true)?
            .snapshot
    );
    editor.set_physics_manual(id, false)?;
    assert_eq!(editor.animation().unwrap().time, 0.);
    assert_eq!(editor.animation().unwrap().edit_time, 0.);
    assert_eq!(
        editor.document().unwrap().physics_definition().as_deref(),
        Some(&original)
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn stabilized_manual_preview_replays_and_restores_animation_position() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    let motion = editor.document().unwrap().animation_catalog().motions[0]
        .selection
        .clone();
    editor.select_motion(id, motion.clone())?;
    editor.seek_timeline(id, 0.35)?;
    let position = (
        editor.animation().unwrap().time,
        editor.animation().unwrap().edit_time,
    );
    let asset = editor.document().unwrap().physics_definition().unwrap();
    let input = asset.data.settings[0].inputs[0]
        .source
        .parameter_id()
        .unwrap()
        .to_owned();
    editor.set_physics_manual(id, true)?;
    editor.set_physics_input(id, &input, 1.)?;
    editor.stabilize_physics_preview(id)?;
    let mut evaluator = AnimationEvaluator::default();
    let stable = sample(&mut editor, &mut evaluator)?;
    assert_eq!(stable.snapshot.time, 0.);
    assert_eq!(stable.snapshot.physics[0].inputs[0], 1.);
    assert!(!stable.snapshot.physics[0].particles.is_empty());
    editor.advance_animation_clock(0.25);
    let request = editor.animation_request().unwrap();
    assert_eq!(
        evaluator.evaluate(&request, |_, _| true)?.snapshot,
        AnimationEvaluator::default()
            .evaluate(&request, |_, _| true)?
            .snapshot
    );
    assert!(editor.animation().unwrap().playing);
    editor.pause_animation();
    editor.reset_physics_inputs(id)?;
    assert!(!editor.animation().unwrap().playing);
    editor.set_physics_manual(id, false)?;
    let animation = editor.animation().unwrap();
    assert_eq!(animation.motion.as_ref(), Some(&motion));
    assert_eq!(animation.time, position.0);
    assert_eq!(animation.edit_time, position.1);
    assert!(animation.physics_preview.baseline.is_empty());
    assert!(!animation.physics_preview.stabilized);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn manual_inputs_update_at_the_same_clock_without_cancelling_inflight_frames() -> anyhow::Result<()>
{
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    let asset = editor.document().unwrap().physics_definition().unwrap();
    let parameter = asset.data.settings[0].inputs[0]
        .source
        .parameter_id()
        .unwrap()
        .to_owned();
    editor.set_physics_manual(id, true)?;
    let before = editor.animation_request().unwrap();
    let old_frame = AnimationEvaluator::default().evaluate(&before, |_, _| true)?;
    editor.set_physics_input(id, &parameter, 0.7)?;
    let after = editor
        .animation_request()
        .expect("input changes need a sample even at unchanged time");
    assert_eq!(
        after.identity.preview_generation,
        before.identity.preview_generation
    );
    assert_eq!(
        after.identity.acceptance_floor,
        before.identity.acceptance_floor
    );
    assert!(after.identity.request > before.identity.request);
    assert_eq!(after.physics.inputs.last().unwrap().value, 0.7);
    assert!(editor.present_animation(old_frame.identity, old_frame.snapshot));
    editor.advance_animation_clock(0.1);
    let frame = sample(&mut editor, &mut AnimationEvaluator::default())?;
    assert!((frame.snapshot.physics[0].inputs[0] - 0.7).abs() < 0.0001);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn global_transport_leaves_manual_trial_and_plays_the_selected_animation() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    let motion = editor.document().unwrap().animation_catalog().motions[0]
        .selection
        .clone();
    editor.select_motion(id, motion)?;
    editor.seek_timeline(id, 0.25)?;
    let timeline_time = editor.animation().unwrap().time;
    editor.set_physics_manual(id, true)?;
    editor.advance_animation_clock(1.);
    editor.toggle_animation()?;
    let animation = editor.animation().unwrap();
    assert!(!animation.physics_preview.manual);
    assert!(animation.playing);
    assert_eq!(animation.time, timeline_time);
    editor.advance_animation_clock(0.5);
    assert_eq!(editor.animation().unwrap().time, 0.5);
    editor.toggle_animation()?;
    assert!(!editor.animation().unwrap().playing);
    Ok(())
}

#[test]
fn physics_authoring_preserves_field_identity_after_reordering_and_does_not_activate_motion()
-> anyhow::Result<()> {
    use zipzi_core::document::{PhysicsEdit, PhysicsTarget};
    let (mut editor, _assets) = setup()?;
    let doc = editor.active_id().unwrap();
    let rig = editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .data
        .settings[0]
        .clone();
    let first = rig.inputs[0].id.clone();
    let inserted = uuid::Uuid::new_v4().to_string();
    let plan = editor.plan_physics_authoring(
        doc,
        PhysicsEdit::CreateInput {
            group_id: rig.id.clone(),
            id: inserted.clone(),
            before: Some(first.clone()),
        },
    )?;
    let result = editor.execute_physics_authoring(doc, &plan)?;
    assert!(result.suggested_selection.iter().any(|v| v.id == inserted));
    assert!(editor.animation().unwrap().motion.is_none());
    let doc = editor.document().unwrap();
    let index = doc.object_index();
    let capabilities =
        doc.physics_authoring_capabilities(&ObjectRef::new(ObjectKind::PhysicsInput, &inserted))?;
    assert_eq!(
        capabilities.domain.patch,
        zipzi_sdk::CapabilityState::Available
    );
    assert!(capabilities.session_reason.is_none());
    assert!(
        index
            .get(&ObjectRef::new(ObjectKind::PhysicsInput, &inserted))
            .is_some()
    );
    assert_eq!(
        doc.physics_definition().unwrap().data.settings[0].inputs[0].source,
        PhysicsTarget::Unspecified
    );
    let object = ObjectRef::new(ObjectKind::PhysicsGroup, &rig.id);
    let key = property::physics::INPUT_WEIGHT.key.for_entity(&first);
    let values = doc.property_values(&object)?;
    let patch = PropertyPatch {
        scope: PropertyScope::Object,
        expected: [(key, values[&key].clone())].into(),
        changes: [(key, PropertyValue::Float(45.))].into(),
    };
    let token = editor.begin_property_edit(
        editor.editing_context_token().unwrap(),
        object.clone(),
        &patch,
    )?;
    editor.update_property_edit(&token, &patch)?;
    editor.finish_property_edit(&token)?;
    let rig = &editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .data
        .settings[0];
    assert_eq!(rig.inputs[0].weight, 100.);
    assert_eq!(rig.inputs[1].id, first);
    assert_eq!(rig.inputs[1].weight, 45.);
    Ok(())
}
#[test]
fn physics_projection_slots_repair_in_place_without_misidentifying_observation()
-> anyhow::Result<()> {
    use zipzi_core::document::{
        PhysicsEdit, PhysicsPatch, PhysicsProperty, PhysicsRigPatch, PhysicsTarget,
    };
    let (mut editor, _assets) = setup()?;
    let doc = editor.active_id().unwrap();
    let asset = editor
        .document()
        .unwrap()
        .physics_definition()
        .unwrap()
        .as_ref()
        .clone();
    let g = &asset.data.settings[0];
    let active = g.inputs[0].id.clone();
    let target = g.inputs[0].source.parameter_id().unwrap().to_owned();
    let pending = uuid::Uuid::new_v4().to_string();
    let p = editor.plan_physics_authoring(
        doc,
        PhysicsEdit::CreateInput {
            group_id: g.id.clone(),
            id: pending.clone(),
            before: Some(active.clone()),
        },
    )?;
    editor.execute_physics_authoring(doc, &p)?;
    assert!(editor.document().unwrap().animation_catalog().diagnostics.iter().any(|d|matches!(d,AnimationDiagnostic::UnconfiguredPhysicsChannel{channel_id} if channel_id==&pending)));
    editor.set_physics_manual(doc, true)?;
    editor.stabilize_physics_preview(doc)?;
    let mut eval = AnimationEvaluator::default();
    let frame = sample(&mut editor, &mut eval)?;
    assert_eq!(frame.snapshot.physics[0].input_ids, vec![active.clone()]);
    let p = editor.plan_physics_authoring(
        doc,
        PhysicsEdit::Patch {
            patch: PhysicsPatch {
                asset_id: asset.id.clone(),
                asset_changes: vec![],
                rig: Some(PhysicsRigPatch {
                    group_id: g.id.clone(),
                    changes: vec![PhysicsProperty::InputParameter {
                        id: pending.clone(),
                        target: PhysicsTarget::Resolved {
                            parameter_id: target,
                        },
                    }],
                }),
            },
        },
    )?;
    editor.execute_physics_authoring(doc, &p)?;
    let frame = sample(&mut editor, &mut eval)?;
    assert_eq!(frame.snapshot.physics[0].input_ids, vec![pending, active]);
    Ok(())
}

#[test]
fn physics_asset_fps_refreshes_every_cached_group_through_history() -> anyhow::Result<()> {
    use zipzi_core::document::{PhysicsAssetProperty, PhysicsEdit, PhysicsPatch};
    let (document, _assets) = support::combined_animation_document()?;
    let mut document = document;
    let asset = document.physics_definition().unwrap().as_ref().clone();
    let old_fps = asset.data.meta.fps.unwrap_or(30.);
    let second = uuid::Uuid::new_v4().to_string();
    let plan = document.plan_physics_authoring(PhysicsEdit::CreateGroup {
        asset_id: asset.id.clone(),
        id: second,
        name: "Second".into(),
        before: None,
    })?;
    document.execute_physics_authoring(&plan)?;
    let groups: Vec<_> = document
        .physics_definition()
        .unwrap()
        .data
        .settings
        .iter()
        .map(|g| ObjectRef::new(ObjectKind::PhysicsGroup, &g.id))
        .collect();
    let retained = document.object_index();
    let plan = document.plan_physics_authoring(PhysicsEdit::Patch {
        patch: PhysicsPatch {
            asset_id: asset.id.clone(),
            asset_changes: vec![PhysicsAssetProperty::Fps(Some(75.))],
            rig: None,
        },
    })?;
    document.execute_physics_authoring(&plan)?;
    for group in &groups {
        assert_eq!(
            document.object_index().get(group).unwrap().properties["fps"],
            75.
        );
        assert_eq!(retained.get(group).unwrap().properties["fps"], old_fps);
    }
    document.undo()?;
    for group in &groups {
        assert_eq!(
            document.object_index().get(group).unwrap().properties["fps"],
            old_fps
        );
    }
    document.redo()?;
    for group in &groups {
        assert_eq!(
            document.object_index().get(group).unwrap().properties["fps"],
            75.
        );
    }
    Ok(())
}

#[test]
fn physics_axis_patch_refreshes_channel_labels_through_history() -> anyhow::Result<()> {
    use zipzi_core::document::{PhysicsEdit, PhysicsPatch, PhysicsProperty, PhysicsRigPatch};
    use zipzi_core::physics::PhysicsAxis;
    let (mut document, _assets) = support::combined_animation_document()?;
    let asset = document.physics_definition().unwrap().as_ref().clone();
    let rig = &asset.data.settings[0];
    let input = ObjectRef::new(ObjectKind::PhysicsInput, &rig.inputs[0].id);
    let output = ObjectRef::new(ObjectKind::PhysicsOutput, &rig.outputs[0].id);
    let retained = document.object_index();
    let new_axis = |old| {
        if old == PhysicsAxis::Y {
            PhysicsAxis::X
        } else {
            PhysicsAxis::Y
        }
    };
    let input_axis = new_axis(rig.inputs[0].kind);
    let output_axis = new_axis(rig.outputs[0].kind);
    let plan = document.plan_physics_authoring(PhysicsEdit::Patch {
        patch: PhysicsPatch {
            asset_id: asset.id.clone(),
            asset_changes: vec![],
            rig: Some(PhysicsRigPatch {
                group_id: rig.id.clone(),
                changes: vec![
                    PhysicsProperty::InputAxis {
                        id: input.id.clone(),
                        value: input_axis,
                    },
                    PhysicsProperty::OutputAxis {
                        id: output.id.clone(),
                        value: output_axis,
                    },
                ],
            }),
        },
    })?;
    document.execute_physics_authoring(&plan)?;
    assert_eq!(
        document.object_index().get(&input).unwrap().name,
        format!("{input_axis:?} input")
    );
    assert_eq!(
        document.object_index().get(&output).unwrap().name,
        format!("{output_axis:?} output")
    );
    document.undo()?;
    for object in [&input, &output] {
        assert_eq!(
            document.object_index().get(object).unwrap().name,
            retained.get(object).unwrap().name
        );
    }
    document.redo()?;
    assert_eq!(
        document.object_index().get(&input).unwrap().name,
        format!("{input_axis:?} input")
    );
    assert_eq!(
        document.object_index().get(&output).unwrap().name,
        format!("{output_axis:?} output")
    );
    Ok(())
}
